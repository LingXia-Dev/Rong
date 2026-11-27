//! Cranelift IR compiler for decoded QuickJS-NG bytecode functions.
//!
//! Compiled only on `x86_64` and `aarch64` (excluding iOS, where W^X JIT
//! allocations are disallowed without special entitlements).

#![cfg(all(
    any(target_arch = "x86_64", target_arch = "aarch64"),
    not(target_os = "ios")
))]

use crate::decoder::{DecodedFunction, Op};
use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{
    AbiParam, Block, InstBuilder, MemFlagsData, StackSlot, StackSlotData, StackSlotKind, Value,
    types,
};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};
use rong_quickjs_sys as sys;
use std::collections::HashMap;

/// Signature of every JIT-compiled JS function.
///
/// Uses only pointers and `i32` so the ABI is identical across Windows x64,
/// System V x64, and AAPCS64 (avoiding 16-byte `JSValue` struct return ABI
/// differences).
///
/// Returns `0` when execution completed (either normal return or JS exception
/// written to `*ret_out`), or `1` if the function bailed out to the interpreter.
pub type CompiledFn = unsafe extern "C" fn(
    ctx: *mut sys::JSContext,
    func_obj: *const sys::JSValue,
    this_obj: *const sys::JSValue,
    argc: i32,
    argv: *const sys::JSValue,
    ret_out: *mut sys::JSValue,
) -> i32;

// QuickJS 64-bit non-NaN-boxed JSValue layout:
//   offset 0: u   (8 bytes: int32 / f64 / ptr)
//   offset 8: tag (8 bytes: i64; negative => refcounted)
const JSVAL_SIZE: u32 = 16;
const TAG_INT: i64 = sys::JS_TAG_INT as i64;
const TAG_BOOL: i64 = sys::JS_TAG_BOOL as i64;
const TAG_NULL: i64 = sys::JS_TAG_NULL as i64;
const TAG_UNDEFINED: i64 = sys::JS_TAG_UNDEFINED as i64;
const TAG_UNINITIALIZED: i64 = sys::JS_TAG_UNINITIALIZED as i64;
const TAG_EXCEPTION: i64 = sys::JS_TAG_EXCEPTION as i64;
const TAG_FLOAT64: i64 = sys::JS_TAG_FLOAT64 as i64;

struct Helpers {
    poll_interrupts: FuncId,
    check_stack: FuncId,
    dup_value: FuncId,
    free_value: FuncId,
    to_bool: FuncId,
    call: FuncId,
    binary_slow: FuncId,
    unary_slow: FuncId,
    get_var: FuncId,
    get_var_undef: FuncId,
    put_var: FuncId,
    get_field: FuncId,
    put_field: FuncId,
    get_array_el: FuncId,
    put_array_el: FuncId,
    get_length: FuncId,
    get_closure_var: FuncId,
    put_closure_var: FuncId,
    throw_uninitialized: FuncId,
}

unsafe extern "C" fn jit_throw_uninitialized(ctx: *mut sys::JSContext) {
    unsafe {
        sys::JS_ThrowReferenceError(
            ctx,
            c"cannot access lexical declaration before initialization".as_ptr(),
        );
    }
}

pub struct CraneliftBackend {
    module: JITModule,
    fb_ctx: FunctionBuilderContext,
    helpers: Helpers,
    ptr_ty: types::Type,
}

impl CraneliftBackend {
    pub fn new() -> Result<Self, String> {
        let mut flag_builder = settings::builder();
        flag_builder
            .set("use_colocated_libcalls", "false")
            .map_err(|e| e.to_string())?;
        flag_builder
            .set("is_pic", "false")
            .map_err(|e| e.to_string())?;
        flag_builder
            .set("opt_level", "speed")
            .map_err(|e| e.to_string())?;

        let isa_builder = cranelift_native::builder().map_err(|e| e.to_string())?;
        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .map_err(|e| e.to_string())?;

        let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());

        // Register QuickJS C runtime helpers.
        builder.symbol("QJS_JitPollInterrupts", sys::QJS_JitPollInterrupts as *const u8);
        builder.symbol("QJS_JitCheckStack", sys::QJS_JitCheckStack as *const u8);
        builder.symbol("QJS_JitDupValue", sys::QJS_JitDupValue as *const u8);
        builder.symbol("QJS_JitFreeValue", sys::QJS_JitFreeValue as *const u8);
        builder.symbol("QJS_JitToBool", sys::QJS_JitToBool as *const u8);
        builder.symbol("QJS_JitCall", sys::QJS_JitCall as *const u8);
        builder.symbol("QJS_JitBinarySlow", sys::QJS_JitBinarySlow as *const u8);
        builder.symbol("QJS_JitUnarySlow", sys::QJS_JitUnarySlow as *const u8);
        builder.symbol("QJS_JitGetVar", sys::QJS_JitGetVar as *const u8);
        builder.symbol("QJS_JitGetVarUndef", sys::QJS_JitGetVarUndef as *const u8);
        builder.symbol("QJS_JitPutVar", sys::QJS_JitPutVar as *const u8);
        builder.symbol("QJS_JitGetField", sys::QJS_JitGetField as *const u8);
        builder.symbol("QJS_JitPutField", sys::QJS_JitPutField as *const u8);
        builder.symbol("QJS_JitGetArrayEl", sys::QJS_JitGetArrayEl as *const u8);
        builder.symbol("QJS_JitPutArrayEl", sys::QJS_JitPutArrayEl as *const u8);
        builder.symbol("QJS_JitGetLength", sys::QJS_JitGetLength as *const u8);
        builder.symbol("QJS_JitGetClosureVar", sys::QJS_JitGetClosureVar as *const u8);
        builder.symbol("QJS_JitPutClosureVar", sys::QJS_JitPutClosureVar as *const u8);
        builder.symbol("jit_throw_uninitialized", jit_throw_uninitialized as *const u8);

        let mut module = JITModule::new(builder);
        let ptr_ty = module.target_config().pointer_type();

        let decl = |m: &mut JITModule,
                    name: &str,
                    params: &[types::Type],
                    ret: Option<types::Type>|
         -> Result<FuncId, String> {
            let mut sig = m.make_signature();
            for &p in params {
                sig.params.push(AbiParam::new(p));
            }
            if let Some(r) = ret {
                sig.returns.push(AbiParam::new(r));
            }
            m.declare_function(name, Linkage::Import, &sig)
                .map_err(|e| e.to_string())
        };

        let helpers = Helpers {
            poll_interrupts: decl(&mut module, "QJS_JitPollInterrupts", &[ptr_ty], Some(types::I32))?,
            check_stack: decl(&mut module, "QJS_JitCheckStack", &[ptr_ty, ptr_ty], Some(types::I32))?,
            dup_value: decl(&mut module, "QJS_JitDupValue", &[ptr_ty, ptr_ty], None)?,
            free_value: decl(&mut module, "QJS_JitFreeValue", &[ptr_ty, ptr_ty], None)?,
            to_bool: decl(&mut module, "QJS_JitToBool", &[ptr_ty, ptr_ty], Some(types::I32))?,
            call: decl(
                &mut module,
                "QJS_JitCall",
                &[ptr_ty, ptr_ty, ptr_ty, types::I32, ptr_ty, ptr_ty],
                Some(types::I32),
            )?,
            binary_slow: decl(
                &mut module,
                "QJS_JitBinarySlow",
                &[ptr_ty, types::I32, ptr_ty, ptr_ty, ptr_ty],
                Some(types::I32),
            )?,
            unary_slow: decl(
                &mut module,
                "QJS_JitUnarySlow",
                &[ptr_ty, types::I32, ptr_ty, ptr_ty],
                Some(types::I32),
            )?,
            get_var: decl(
                &mut module,
                "QJS_JitGetVar",
                &[ptr_ty, types::I32, ptr_ty],
                Some(types::I32),
            )?,
            get_var_undef: decl(
                &mut module,
                "QJS_JitGetVarUndef",
                &[ptr_ty, types::I32, ptr_ty],
                Some(types::I32),
            )?,
            put_var: decl(
                &mut module,
                "QJS_JitPutVar",
                &[ptr_ty, types::I32, ptr_ty],
                Some(types::I32),
            )?,
            get_field: decl(
                &mut module,
                "QJS_JitGetField",
                &[ptr_ty, ptr_ty, types::I32, ptr_ty],
                Some(types::I32),
            )?,
            put_field: decl(
                &mut module,
                "QJS_JitPutField",
                &[ptr_ty, ptr_ty, types::I32, ptr_ty],
                Some(types::I32),
            )?,
            get_array_el: decl(
                &mut module,
                "QJS_JitGetArrayEl",
                &[ptr_ty, ptr_ty, ptr_ty, ptr_ty],
                Some(types::I32),
            )?,
            put_array_el: decl(
                &mut module,
                "QJS_JitPutArrayEl",
                &[ptr_ty, ptr_ty, ptr_ty, ptr_ty],
                Some(types::I32),
            )?,
            get_length: decl(
                &mut module,
                "QJS_JitGetLength",
                &[ptr_ty, ptr_ty, ptr_ty],
                Some(types::I32),
            )?,
            get_closure_var: decl(
                &mut module,
                "QJS_JitGetClosureVar",
                &[ptr_ty, ptr_ty, types::I16, ptr_ty],
                Some(types::I32),
            )?,
            put_closure_var: decl(
                &mut module,
                "QJS_JitPutClosureVar",
                &[ptr_ty, ptr_ty, types::I16, ptr_ty],
                Some(types::I32),
            )?,
            throw_uninitialized: decl(&mut module, "jit_throw_uninitialized", &[ptr_ty], None)?,
        };

        Ok(Self {
            module,
            fb_ctx: FunctionBuilderContext::new(),
            helpers,
            ptr_ty,
        })
    }

    pub fn compile(
        &mut self,
        decoded: &DecodedFunction,
        info: &sys::QJSJitBytecodeInfo,
    ) -> Result<CompiledFn, String> {
        let ptr_ty = self.ptr_ty;
        let mut sig = self.module.make_signature();
        sig.params.push(AbiParam::new(ptr_ty)); // ctx
        sig.params.push(AbiParam::new(ptr_ty)); // func_obj
        sig.params.push(AbiParam::new(ptr_ty)); // this_obj
        sig.params.push(AbiParam::new(types::I32)); // argc
        sig.params.push(AbiParam::new(ptr_ty)); // argv
        sig.params.push(AbiParam::new(ptr_ty)); // ret_out
        sig.returns.push(AbiParam::new(types::I32)); // 0 = done, 1 = bailout

        let func_id = self
            .module
            .declare_anonymous_function(&sig)
            .map_err(|e| e.to_string())?;

        let mut ctx = self.module.make_context();
        ctx.func.signature = sig;

        // Declare imported helpers into this function.
        let f_poll = self.module.declare_func_in_func(self.helpers.poll_interrupts, &mut ctx.func);
        let f_check_stack = self.module.declare_func_in_func(self.helpers.check_stack, &mut ctx.func);
        let f_dup = self.module.declare_func_in_func(self.helpers.dup_value, &mut ctx.func);
        let f_free = self.module.declare_func_in_func(self.helpers.free_value, &mut ctx.func);
        let f_to_bool = self.module.declare_func_in_func(self.helpers.to_bool, &mut ctx.func);
        let f_call = self.module.declare_func_in_func(self.helpers.call, &mut ctx.func);
        let f_bin_slow = self.module.declare_func_in_func(self.helpers.binary_slow, &mut ctx.func);
        let f_un_slow = self.module.declare_func_in_func(self.helpers.unary_slow, &mut ctx.func);
        let f_get_var = self.module.declare_func_in_func(self.helpers.get_var, &mut ctx.func);
        let f_get_var_undef = self.module.declare_func_in_func(self.helpers.get_var_undef, &mut ctx.func);
        let f_put_var = self.module.declare_func_in_func(self.helpers.put_var, &mut ctx.func);
        let f_get_field = self.module.declare_func_in_func(self.helpers.get_field, &mut ctx.func);
        let f_put_field = self.module.declare_func_in_func(self.helpers.put_field, &mut ctx.func);
        let f_get_array_el = self.module.declare_func_in_func(self.helpers.get_array_el, &mut ctx.func);
        let f_put_array_el = self.module.declare_func_in_func(self.helpers.put_array_el, &mut ctx.func);
        let f_get_length = self.module.declare_func_in_func(self.helpers.get_length, &mut ctx.func);
        let f_get_closure = self.module.declare_func_in_func(self.helpers.get_closure_var, &mut ctx.func);
        let f_put_closure = self.module.declare_func_in_func(self.helpers.put_closure_var, &mut ctx.func);
        let f_throw_tdz = self.module.declare_func_in_func(self.helpers.throw_uninitialized, &mut ctx.func);

        {
            let mut b = FunctionBuilder::new(&mut ctx.func, &mut self.fb_ctx);
            let entry_block = b.create_block();
            b.append_block_params_for_function_params(entry_block);
            b.switch_to_block(entry_block);

            let p_ctx = b.block_params(entry_block)[0];
            let p_func_obj = b.block_params(entry_block)[1];
            let p_this_obj = b.block_params(entry_block)[2];
            let p_argc = b.block_params(entry_block)[3];
            let p_argv = b.block_params(entry_block)[4];
            let p_ret_out = b.block_params(entry_block)[5];

            let arg_count = decoded.arg_count as u32;
            let var_count = decoded.var_count as u32;
            let stack_size = decoded.stack_size as u32;

            // Allocate one contiguous frame slot:
            //   [args: arg_count] [vars: var_count] [eval_stack: stack_size] [scratch: 2]
            // Keeping all live JSValues in one contiguous array makes cleanup
            // on return/exception a single tight loop over `0..total_live`.
            let total_slots = arg_count + var_count + stack_size;
            let frame_slots = (total_slots + 2).max(2);
            let frame_bytes = frame_slots * JSVAL_SIZE;
            let frame_slot = b.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                frame_bytes,
                3,
            ));

            let args_off = 0i32;
            let vars_off = (arg_count * JSVAL_SIZE) as i32;
            let stack_off = ((arg_count + var_count) * JSVAL_SIZE) as i32;
            let scratch0_off = (total_slots * JSVAL_SIZE) as i32;
            let scratch1_off = ((total_slots + 1) * JSVAL_SIZE) as i32;

            // Stack overflow guard in prologue.
            let stack_ok_block = b.create_block();
            let stack_ovf_block = b.create_block();
            let req_bytes = b.ins().iconst(ptr_ty, (frame_bytes as i64) + 1024);
            let ovf_call = b.ins().call(f_check_stack, &[p_ctx, req_bytes]);
            let ovf_res = b.inst_results(ovf_call)[0];
            b.ins().brif(ovf_res, stack_ovf_block, &[], stack_ok_block, &[]);

            b.switch_to_block(stack_ovf_block);
            store_raw_ptr(&mut b, p_ret_out, 0, TAG_EXCEPTION);
            let zero = b.ins().iconst(types::I32, 0);
            b.ins().return_(&[zero]);

            b.switch_to_block(stack_ok_block);

            // Initialize args[0..arg_count]: copy & dup from argv[i] if i < argc, else undefined.
            for i in 0..arg_count {
                let dst_off = args_off + (i * JSVAL_SIZE) as i32;
                let has_arg_blk = b.create_block();
                let no_arg_blk = b.create_block();
                let next_blk = b.create_block();
                let idx_val = b.ins().iconst(types::I32, i as i64);
                let cmp = b.ins().icmp(IntCC::SignedLessThan, idx_val, p_argc);
                b.ins().brif(cmp, has_arg_blk, &[], no_arg_blk, &[]);

                b.switch_to_block(has_arg_blk);
                let src_off = (i * JSVAL_SIZE) as i32;
                let u = b.ins().load(types::I64, MemFlagsData::trusted(), p_argv, src_off);
                let tag = b.ins().load(types::I64, MemFlagsData::trusted(), p_argv, src_off + 8);
                sstore(&mut b, u, frame_slot, dst_off);
                sstore(&mut b, tag, frame_slot, dst_off + 8);
                emit_dup_if_ref(&mut b, f_dup, ptr_ty, p_ctx, frame_slot, dst_off, tag);
                b.ins().jump(next_blk, &[]);

                b.switch_to_block(no_arg_blk);
                store_slot_const(&mut b, frame_slot, dst_off, 0, TAG_UNDEFINED);
                b.ins().jump(next_blk, &[]);

                b.switch_to_block(next_blk);
            }

            // Initialize vars[0..var_count] and eval_stack[0..stack_size] to undefined
            // so exception cleanup can unconditionally scan all slots safely.
            let zero64 = b.ins().iconst(types::I64, 0);
            let undef_tag = b.ins().iconst(types::I64, TAG_UNDEFINED);
            for i in 0..(var_count + stack_size) {
                let off = vars_off + (i * JSVAL_SIZE) as i32;
                sstore(&mut b, zero64, frame_slot, off);
                sstore(&mut b, undef_tag, frame_slot, off + 8);
            }

            // Shared exception epilogue block: frees all refcounted slots in
            // args + vars + eval_stack, writes JS_EXCEPTION to *ret_out, returns 0.
            let exc_block = b.create_block();

            // Create a Cranelift block for every bytecode basic-block start.
            let mut pc_blocks: HashMap<usize, Block> = HashMap::new();
            for &start_pc in &decoded.block_starts {
                pc_blocks.insert(start_pc, b.create_block());
            }

            let first_block = pc_blocks[&0];
            b.ins().jump(first_block, &[]);

            // Lower instructions block by block.
            let mut terminated = true;
            for insn in &decoded.insns {
                if let Some(&blk) = pc_blocks.get(&insn.pc) {
                    if !terminated {
                        b.ins().jump(blk, &[]);
                    }
                    b.switch_to_block(blk);
                    terminated = false;
                }
                if terminated {
                    continue;
                }

                let sp = insn.stack_before as u32;
                let slot_off = |depth: u32| -> i32 { stack_off + (depth * JSVAL_SIZE) as i32 };
                let loc_off = |idx: u16| -> i32 { vars_off + ((idx as u32) * JSVAL_SIZE) as i32 };
                let arg_off = |idx: u16| -> i32 { args_off + ((idx as u32) * JSVAL_SIZE) as i32 };

                match insn.op {
                    Op::Nop => {}

                    Op::PushI32(v) => {
                        store_slot_const(&mut b, frame_slot, slot_off(sp), v as u32 as i64, TAG_INT);
                    }
                    Op::PushUndefined => {
                        store_slot_const(&mut b, frame_slot, slot_off(sp), 0, TAG_UNDEFINED);
                    }
                    Op::PushNull => {
                        store_slot_const(&mut b, frame_slot, slot_off(sp), 0, TAG_NULL);
                    }
                    Op::PushBool(v) => {
                        store_slot_const(&mut b, frame_slot, slot_off(sp), v as i64, TAG_BOOL);
                    }
                    Op::PushThis => {
                        let u = b.ins().load(types::I64, MemFlagsData::trusted(), p_this_obj, 0);
                        let tag = b.ins().load(types::I64, MemFlagsData::trusted(), p_this_obj, 8);
                        let dst = slot_off(sp);
                        sstore(&mut b, u, frame_slot, dst);
                        sstore(&mut b, tag, frame_slot, dst + 8);
                        emit_dup_if_ref(&mut b, f_dup, ptr_ty, p_ctx, frame_slot, dst, tag);
                    }
                    Op::PushConst(cidx) => {
                        let cptr_addr = unsafe { info.cpool.add(cidx as usize) } as i64;
                        let cptr = b.ins().iconst(ptr_ty, cptr_addr);
                        let u = b.ins().load(types::I64, MemFlagsData::trusted(), cptr, 0);
                        let tag = b.ins().load(types::I64, MemFlagsData::trusted(), cptr, 8);
                        let dst = slot_off(sp);
                        sstore(&mut b, u, frame_slot, dst);
                        sstore(&mut b, tag, frame_slot, dst + 8);
                        emit_dup_if_ref(&mut b, f_dup, ptr_ty, p_ctx, frame_slot, dst, tag);
                    }

                    // Stack manipulation
                    Op::Drop => {
                        let top = slot_off(sp - 1);
                        emit_free_and_undef(&mut b, f_free, ptr_ty, p_ctx, frame_slot, top);
                    }
                    Op::Nip => {
                        // a b -> b
                        let s0 = slot_off(sp - 2);
                        let s1 = slot_off(sp - 1);
                        emit_free_slot(&mut b, f_free, ptr_ty, p_ctx, frame_slot, s0);
                        copy_slot(&mut b, frame_slot, s1, s0);
                        store_slot_const(&mut b, frame_slot, s1, 0, TAG_UNDEFINED);
                    }
                    Op::Dup => {
                        let s0 = slot_off(sp - 1);
                        let dst = slot_off(sp);
                        let tag = copy_slot(&mut b, frame_slot, s0, dst);
                        emit_dup_if_ref(&mut b, f_dup, ptr_ty, p_ctx, frame_slot, dst, tag);
                    }
                    Op::Dup1 => {
                        // a b -> a a b
                        let s0 = slot_off(sp - 2);
                        let s1 = slot_off(sp - 1);
                        let s2 = slot_off(sp);
                        copy_slot(&mut b, frame_slot, s1, s2);
                        let tag = copy_slot(&mut b, frame_slot, s0, s1);
                        emit_dup_if_ref(&mut b, f_dup, ptr_ty, p_ctx, frame_slot, s1, tag);
                    }
                    Op::Swap => {
                        let s0 = slot_off(sp - 2);
                        let s1 = slot_off(sp - 1);
                        copy_slot(&mut b, frame_slot, s0, scratch0_off);
                        copy_slot(&mut b, frame_slot, s1, s0);
                        copy_slot(&mut b, frame_slot, scratch0_off, s1);
                    }

                    // Locals & args
                    Op::GetLoc(idx) => {
                        let src = loc_off(idx);
                        let dst = slot_off(sp);
                        let tag = copy_slot(&mut b, frame_slot, src, dst);
                        emit_dup_if_ref(&mut b, f_dup, ptr_ty, p_ctx, frame_slot, dst, tag);
                    }
                    Op::GetLocCheck(idx) => {
                        let src = loc_off(idx);
                        let dst = slot_off(sp);
                        let tag = sload(&mut b, frame_slot, src + 8);
                        let is_uninit = b.ins().icmp_imm_s(IntCC::Equal, tag, TAG_UNINITIALIZED);
                        let tdz_blk = b.create_block();
                        let ok_blk = b.create_block();
                        b.ins().brif(is_uninit, tdz_blk, &[], ok_blk, &[]);
                        b.switch_to_block(tdz_blk);
                        b.ins().call(f_throw_tdz, &[p_ctx]);
                        b.ins().jump(exc_block, &[]);
                        b.switch_to_block(ok_blk);
                        let tag = copy_slot(&mut b, frame_slot, src, dst);
                        emit_dup_if_ref(&mut b, f_dup, ptr_ty, p_ctx, frame_slot, dst, tag);
                    }
                    Op::GetLoc0Loc1 => {
                        let d0 = slot_off(sp);
                        let t0 = copy_slot(&mut b, frame_slot, loc_off(0), d0);
                        emit_dup_if_ref(&mut b, f_dup, ptr_ty, p_ctx, frame_slot, d0, t0);
                        let d1 = slot_off(sp + 1);
                        let t1 = copy_slot(&mut b, frame_slot, loc_off(1), d1);
                        emit_dup_if_ref(&mut b, f_dup, ptr_ty, p_ctx, frame_slot, d1, t1);
                    }
                    Op::PutLoc(idx) => {
                        let src = slot_off(sp - 1);
                        let dst = loc_off(idx);
                        emit_free_slot(&mut b, f_free, ptr_ty, p_ctx, frame_slot, dst);
                        copy_slot(&mut b, frame_slot, src, dst);
                        store_slot_const(&mut b, frame_slot, src, 0, TAG_UNDEFINED);
                    }
                    Op::PutLocCheck(idx) => {
                        let src = slot_off(sp - 1);
                        let dst = loc_off(idx);
                        let tag = sload(&mut b, frame_slot, dst + 8);
                        let is_uninit = b.ins().icmp_imm_s(IntCC::Equal, tag, TAG_UNINITIALIZED);
                        let tdz_blk = b.create_block();
                        let ok_blk = b.create_block();
                        b.ins().brif(is_uninit, tdz_blk, &[], ok_blk, &[]);
                        b.switch_to_block(tdz_blk);
                        b.ins().call(f_throw_tdz, &[p_ctx]);
                        b.ins().jump(exc_block, &[]);
                        b.switch_to_block(ok_blk);
                        emit_free_slot(&mut b, f_free, ptr_ty, p_ctx, frame_slot, dst);
                        copy_slot(&mut b, frame_slot, src, dst);
                        store_slot_const(&mut b, frame_slot, src, 0, TAG_UNDEFINED);
                    }
                    Op::SetLoc(idx) => {
                        let src = slot_off(sp - 1);
                        let dst = loc_off(idx);
                        emit_free_slot(&mut b, f_free, ptr_ty, p_ctx, frame_slot, dst);
                        let tag = copy_slot(&mut b, frame_slot, src, dst);
                        emit_dup_if_ref(&mut b, f_dup, ptr_ty, p_ctx, frame_slot, dst, tag);
                    }
                    Op::SetLocUninitialized(idx) => {
                        let dst = loc_off(idx);
                        emit_free_slot(&mut b, f_free, ptr_ty, p_ctx, frame_slot, dst);
                        store_slot_const(&mut b, frame_slot, dst, 0, TAG_UNINITIALIZED);
                    }
                    Op::GetArg(idx) => {
                        let src = arg_off(idx);
                        let dst = slot_off(sp);
                        let tag = copy_slot(&mut b, frame_slot, src, dst);
                        emit_dup_if_ref(&mut b, f_dup, ptr_ty, p_ctx, frame_slot, dst, tag);
                    }
                    Op::PutArg(idx) => {
                        let src = slot_off(sp - 1);
                        let dst = arg_off(idx);
                        emit_free_slot(&mut b, f_free, ptr_ty, p_ctx, frame_slot, dst);
                        copy_slot(&mut b, frame_slot, src, dst);
                        store_slot_const(&mut b, frame_slot, src, 0, TAG_UNDEFINED);
                    }
                    Op::SetArg(idx) => {
                        let src = slot_off(sp - 1);
                        let dst = arg_off(idx);
                        emit_free_slot(&mut b, f_free, ptr_ty, p_ctx, frame_slot, dst);
                        let tag = copy_slot(&mut b, frame_slot, src, dst);
                        emit_dup_if_ref(&mut b, f_dup, ptr_ty, p_ctx, frame_slot, dst, tag);
                    }

                    // Closure vars
                    Op::GetVarRef(idx) => {
                        let dst = slot_off(sp);
                        let dst_ptr = b.ins().stack_addr(ptr_ty, frame_slot, dst);
                        let idx_val = b.ins().iconst(types::I16, idx as i64);
                        let c = b.ins().call(f_get_closure, &[p_ctx, p_func_obj, idx_val, dst_ptr]);
                        let err = b.inst_results(c)[0];
                        emit_check_exc(&mut b, err, exc_block);
                    }
                    Op::PutVarRef(idx) => {
                        let src = slot_off(sp - 1);
                        let src_ptr = b.ins().stack_addr(ptr_ty, frame_slot, src);
                        let idx_val = b.ins().iconst(types::I16, idx as i64);
                        let c = b.ins().call(f_put_closure, &[p_ctx, p_func_obj, idx_val, src_ptr]);
                        let err = b.inst_results(c)[0];
                        emit_free_and_undef(&mut b, f_free, ptr_ty, p_ctx, frame_slot, src);
                        emit_check_exc(&mut b, err, exc_block);
                    }
                    Op::SetVarRef(idx) => {
                        let src = slot_off(sp - 1);
                        let src_ptr = b.ins().stack_addr(ptr_ty, frame_slot, src);
                        let idx_val = b.ins().iconst(types::I16, idx as i64);
                        let c = b.ins().call(f_put_closure, &[p_ctx, p_func_obj, idx_val, src_ptr]);
                        let err = b.inst_results(c)[0];
                        emit_check_exc(&mut b, err, exc_block);
                    }

                    // Globals
                    Op::GetVar(atom) => {
                        let dst = slot_off(sp);
                        let dst_ptr = b.ins().stack_addr(ptr_ty, frame_slot, dst);
                        let atom_v = b.ins().iconst(types::I32, atom as i64);
                        let c = b.ins().call(f_get_var, &[p_ctx, atom_v, dst_ptr]);
                        let err = b.inst_results(c)[0];
                        emit_check_exc(&mut b, err, exc_block);
                    }
                    Op::GetVarUndef(atom) => {
                        let dst = slot_off(sp);
                        let dst_ptr = b.ins().stack_addr(ptr_ty, frame_slot, dst);
                        let atom_v = b.ins().iconst(types::I32, atom as i64);
                        let c = b.ins().call(f_get_var_undef, &[p_ctx, atom_v, dst_ptr]);
                        let err = b.inst_results(c)[0];
                        emit_check_exc(&mut b, err, exc_block);
                    }
                    Op::PutVar(atom) => {
                        let src = slot_off(sp - 1);
                        let src_ptr = b.ins().stack_addr(ptr_ty, frame_slot, src);
                        let atom_v = b.ins().iconst(types::I32, atom as i64);
                        let c = b.ins().call(f_put_var, &[p_ctx, atom_v, src_ptr]);
                        let err = b.inst_results(c)[0];
                        emit_free_and_undef(&mut b, f_free, ptr_ty, p_ctx, frame_slot, src);
                        emit_check_exc(&mut b, err, exc_block);
                    }

                    // Properties & arrays
                    Op::GetField(atom) => {
                        let s0 = slot_off(sp - 1);
                        let obj_ptr = b.ins().stack_addr(ptr_ty, frame_slot, s0);
                        let out_ptr = b.ins().stack_addr(ptr_ty, frame_slot, scratch0_off);
                        let atom_v = b.ins().iconst(types::I32, atom as i64);
                        let c = b.ins().call(f_get_field, &[p_ctx, obj_ptr, atom_v, out_ptr]);
                        let err = b.inst_results(c)[0];
                        emit_free_slot(&mut b, f_free, ptr_ty, p_ctx, frame_slot, s0);
                        copy_slot(&mut b, frame_slot, scratch0_off, s0);
                        emit_check_exc(&mut b, err, exc_block);
                    }
                    Op::GetField2(atom) => {
                        let s0 = slot_off(sp - 1);
                        let s1 = slot_off(sp);
                        let obj_ptr = b.ins().stack_addr(ptr_ty, frame_slot, s0);
                        let out_ptr = b.ins().stack_addr(ptr_ty, frame_slot, s1);
                        let atom_v = b.ins().iconst(types::I32, atom as i64);
                        let c = b.ins().call(f_get_field, &[p_ctx, obj_ptr, atom_v, out_ptr]);
                        let err = b.inst_results(c)[0];
                        emit_check_exc(&mut b, err, exc_block);
                    }
                    Op::PutField(atom) => {
                        let s_obj = slot_off(sp - 2);
                        let s_val = slot_off(sp - 1);
                        let obj_ptr = b.ins().stack_addr(ptr_ty, frame_slot, s_obj);
                        let val_ptr = b.ins().stack_addr(ptr_ty, frame_slot, s_val);
                        let atom_v = b.ins().iconst(types::I32, atom as i64);
                        let c = b.ins().call(f_put_field, &[p_ctx, obj_ptr, atom_v, val_ptr]);
                        let err = b.inst_results(c)[0];
                        emit_free_and_undef(&mut b, f_free, ptr_ty, p_ctx, frame_slot, s_val);
                        emit_free_and_undef(&mut b, f_free, ptr_ty, p_ctx, frame_slot, s_obj);
                        emit_check_exc(&mut b, err, exc_block);
                    }
                    Op::GetArrayEl => {
                        let s_obj = slot_off(sp - 2);
                        let s_idx = slot_off(sp - 1);
                        let obj_ptr = b.ins().stack_addr(ptr_ty, frame_slot, s_obj);
                        let idx_ptr = b.ins().stack_addr(ptr_ty, frame_slot, s_idx);
                        let out_ptr = b.ins().stack_addr(ptr_ty, frame_slot, scratch0_off);
                        let c = b.ins().call(f_get_array_el, &[p_ctx, obj_ptr, idx_ptr, out_ptr]);
                        let err = b.inst_results(c)[0];
                        emit_free_and_undef(&mut b, f_free, ptr_ty, p_ctx, frame_slot, s_idx);
                        emit_free_slot(&mut b, f_free, ptr_ty, p_ctx, frame_slot, s_obj);
                        copy_slot(&mut b, frame_slot, scratch0_off, s_obj);
                        emit_check_exc(&mut b, err, exc_block);
                    }
                    Op::GetArrayEl2 => {
                        let s_obj = slot_off(sp - 2);
                        let s_idx = slot_off(sp - 1);
                        let obj_ptr = b.ins().stack_addr(ptr_ty, frame_slot, s_obj);
                        let idx_ptr = b.ins().stack_addr(ptr_ty, frame_slot, s_idx);
                        let out_ptr = b.ins().stack_addr(ptr_ty, frame_slot, scratch0_off);
                        let c = b.ins().call(f_get_array_el, &[p_ctx, obj_ptr, idx_ptr, out_ptr]);
                        let err = b.inst_results(c)[0];
                        emit_free_slot(&mut b, f_free, ptr_ty, p_ctx, frame_slot, s_idx);
                        copy_slot(&mut b, frame_slot, scratch0_off, s_idx);
                        emit_check_exc(&mut b, err, exc_block);
                    }
                    Op::PutArrayEl => {
                        let s_obj = slot_off(sp - 3);
                        let s_idx = slot_off(sp - 2);
                        let s_val = slot_off(sp - 1);
                        let obj_ptr = b.ins().stack_addr(ptr_ty, frame_slot, s_obj);
                        let idx_ptr = b.ins().stack_addr(ptr_ty, frame_slot, s_idx);
                        let val_ptr = b.ins().stack_addr(ptr_ty, frame_slot, s_val);
                        let c = b.ins().call(f_put_array_el, &[p_ctx, obj_ptr, idx_ptr, val_ptr]);
                        let err = b.inst_results(c)[0];
                        emit_free_and_undef(&mut b, f_free, ptr_ty, p_ctx, frame_slot, s_val);
                        emit_free_and_undef(&mut b, f_free, ptr_ty, p_ctx, frame_slot, s_idx);
                        emit_free_and_undef(&mut b, f_free, ptr_ty, p_ctx, frame_slot, s_obj);
                        emit_check_exc(&mut b, err, exc_block);
                    }
                    Op::GetLength => {
                        let s0 = slot_off(sp - 1);
                        let obj_ptr = b.ins().stack_addr(ptr_ty, frame_slot, s0);
                        let out_ptr = b.ins().stack_addr(ptr_ty, frame_slot, scratch0_off);
                        let c = b.ins().call(f_get_length, &[p_ctx, obj_ptr, out_ptr]);
                        let err = b.inst_results(c)[0];
                        emit_free_slot(&mut b, f_free, ptr_ty, p_ctx, frame_slot, s0);
                        copy_slot(&mut b, frame_slot, scratch0_off, s0);
                        emit_check_exc(&mut b, err, exc_block);
                    }

                    // In-place local numeric ops
                    Op::IncLoc(idx) => {
                        emit_inc_dec_loc(
                            &mut b,
                            f_un_slow,
                            f_free,
                            ptr_ty,
                            p_ctx,
                            frame_slot,
                            loc_off(idx),
                            scratch0_off,
                            1,
                            sys::QJSJitOp_QJS_OP_inc as i32,
                            exc_block,
                        );
                    }
                    Op::DecLoc(idx) => {
                        emit_inc_dec_loc(
                            &mut b,
                            f_un_slow,
                            f_free,
                            ptr_ty,
                            p_ctx,
                            frame_slot,
                            loc_off(idx),
                            scratch0_off,
                            -1,
                            sys::QJSJitOp_QJS_OP_dec as i32,
                            exc_block,
                        );
                    }
                    Op::AddLoc(idx) => {
                        let s_rhs = slot_off(sp - 1);
                        let s_loc = loc_off(idx);
                        emit_binary_op(
                            &mut b,
                            f_bin_slow,
                            f_free,
                            ptr_ty,
                            p_ctx,
                            frame_slot,
                            s_loc,
                            s_rhs,
                            s_loc,
                            scratch0_off,
                            Op::Add,
                            exc_block,
                        );
                        emit_free_and_undef(&mut b, f_free, ptr_ty, p_ctx, frame_slot, s_rhs);
                    }

                    // Unary ops
                    Op::Neg | Op::Plus | Op::Inc | Op::Dec | Op::Not => {
                        let s0 = slot_off(sp - 1);
                        emit_unary_op(
                            &mut b,
                            f_un_slow,
                            f_free,
                            ptr_ty,
                            p_ctx,
                            frame_slot,
                            s0,
                            scratch0_off,
                            insn.op,
                            exc_block,
                        );
                    }
                    Op::PostInc | Op::PostDec => {
                        // a -> old_num new_num
                        let s0 = slot_off(sp - 1);
                        let s1 = slot_off(sp);
                        let delta = if matches!(insn.op, Op::PostInc) { 1i64 } else { -1i64 };
                        let slow_op = if matches!(insn.op, Op::PostInc) {
                            sys::QJSJitOp_QJS_OP_inc as i32
                        } else {
                            sys::QJSJitOp_QJS_OP_dec as i32
                        };
                        let u = sload(&mut b, frame_slot, s0);
                        let tag = sload(&mut b, frame_slot, s0 + 8);
                        let is_int = b.ins().icmp_imm_s(IntCC::Equal, tag, TAG_INT);
                        let fast_blk = b.create_block();
                        let slow_blk = b.create_block();
                        let done_blk = b.create_block();
                        b.ins().brif(is_int, fast_blk, &[], slow_blk, &[]);

                        b.switch_to_block(fast_blk);
                        let i32_v = b.ins().ireduce(types::I32, u);
                        let i64_v = b.ins().sextend(types::I64, i32_v);
                        let sum = b.ins().iadd_imm_s(i64_v, delta);
                        let sum32 = b.ins().ireduce(types::I32, sum);
                        let check = b.ins().sextend(types::I64, sum32);
                        let fits = b.ins().icmp(IntCC::Equal, sum, check);
                        let fit_blk = b.create_block();
                        let ovf_blk = b.create_block();
                        b.ins().brif(fits, fit_blk, &[], ovf_blk, &[]);

                        b.switch_to_block(fit_blk);
                        let sum_u = b.ins().uextend(types::I64, sum32);
                        let int_tag = b.ins().iconst(types::I64, TAG_INT);
                        sstore(&mut b, sum_u, frame_slot, s1);
                        sstore(&mut b, int_tag, frame_slot, s1 + 8);
                        b.ins().jump(done_blk, &[]);

                        b.switch_to_block(ovf_blk);
                        let f = b.ins().fcvt_from_sint(types::F64, sum);
                        let fbits = b.ins().bitcast(types::I64, MemFlagsData::new(), f);
                        let ftag = b.ins().iconst(types::I64, TAG_FLOAT64);
                        sstore(&mut b, fbits, frame_slot, s1);
                        sstore(&mut b, ftag, frame_slot, s1 + 8);
                        b.ins().jump(done_blk, &[]);

                        b.switch_to_block(slow_blk);
                        let src_ptr = b.ins().stack_addr(ptr_ty, frame_slot, s0);
                        let old_ptr = b.ins().stack_addr(ptr_ty, frame_slot, scratch0_off);
                        let plus_op = b.ins().iconst(types::I32, sys::QJSJitOp_QJS_OP_plus as i64);
                        let c1 = b.ins().call(f_un_slow, &[p_ctx, plus_op, src_ptr, old_ptr]);
                        let err1 = b.inst_results(c1)[0];
                        emit_check_exc(&mut b, err1, exc_block);
                        emit_free_slot(&mut b, f_free, ptr_ty, p_ctx, frame_slot, s0);
                        copy_slot(&mut b, frame_slot, scratch0_off, s0);

                        let new_ptr = b.ins().stack_addr(ptr_ty, frame_slot, s1);
                        let op_v = b.ins().iconst(types::I32, slow_op as i64);
                        let c2 = b.ins().call(f_un_slow, &[p_ctx, op_v, src_ptr, new_ptr]);
                        let err2 = b.inst_results(c2)[0];
                        emit_check_exc(&mut b, err2, exc_block);
                        b.ins().jump(done_blk, &[]);

                        b.switch_to_block(done_blk);
                    }
                    Op::LNot => {
                        let s0 = slot_off(sp - 1);
                        let truthy = emit_to_bool(&mut b, f_to_bool, ptr_ty, p_ctx, frame_slot, s0, exc_block);
                        emit_free_slot(&mut b, f_free, ptr_ty, p_ctx, frame_slot, s0);
                        let is_zero = b.ins().icmp_imm_s(IntCC::Equal, truthy, 0);
                        let res_u = b.ins().uextend(types::I64, is_zero);
                        let tag_bool = b.ins().iconst(types::I64, TAG_BOOL);
                        sstore(&mut b, res_u, frame_slot, s0);
                        sstore(&mut b, tag_bool, frame_slot, s0 + 8);
                    }
                    Op::IsUndefined => {
                        let s0 = slot_off(sp - 1);
                        let tag = sload(&mut b, frame_slot, s0 + 8);
                        emit_free_slot(&mut b, f_free, ptr_ty, p_ctx, frame_slot, s0);
                        let is_u = b.ins().icmp_imm_s(IntCC::Equal, tag, TAG_UNDEFINED);
                        let res_u = b.ins().uextend(types::I64, is_u);
                        let tag_bool = b.ins().iconst(types::I64, TAG_BOOL);
                        sstore(&mut b, res_u, frame_slot, s0);
                        sstore(&mut b, tag_bool, frame_slot, s0 + 8);
                    }
                    Op::IsNull => {
                        let s0 = slot_off(sp - 1);
                        let tag = sload(&mut b, frame_slot, s0 + 8);
                        emit_free_slot(&mut b, f_free, ptr_ty, p_ctx, frame_slot, s0);
                        let is_n = b.ins().icmp_imm_s(IntCC::Equal, tag, TAG_NULL);
                        let res_u = b.ins().uextend(types::I64, is_n);
                        let tag_bool = b.ins().iconst(types::I64, TAG_BOOL);
                        sstore(&mut b, res_u, frame_slot, s0);
                        sstore(&mut b, tag_bool, frame_slot, s0 + 8);
                    }
                    Op::IsUndefinedOrNull => {
                        let s0 = slot_off(sp - 1);
                        let tag = sload(&mut b, frame_slot, s0 + 8);
                        emit_free_slot(&mut b, f_free, ptr_ty, p_ctx, frame_slot, s0);
                        let is_u = b.ins().icmp_imm_s(IntCC::Equal, tag, TAG_UNDEFINED);
                        let is_n = b.ins().icmp_imm_s(IntCC::Equal, tag, TAG_NULL);
                        let both = b.ins().bor(is_u, is_n);
                        let res_u = b.ins().uextend(types::I64, both);
                        let tag_bool = b.ins().iconst(types::I64, TAG_BOOL);
                        sstore(&mut b, res_u, frame_slot, s0);
                        sstore(&mut b, tag_bool, frame_slot, s0 + 8);
                    }

                    // Binary ops
                    Op::Add
                    | Op::Sub
                    | Op::Mul
                    | Op::Div
                    | Op::Mod
                    | Op::Pow
                    | Op::Shl
                    | Op::Sar
                    | Op::Shr
                    | Op::And
                    | Op::Or
                    | Op::Xor
                    | Op::Lt
                    | Op::Lte
                    | Op::Gt
                    | Op::Gte
                    | Op::Eq
                    | Op::Neq
                    | Op::StrictEq
                    | Op::StrictNeq => {
                        let s0 = slot_off(sp - 2);
                        let s1 = slot_off(sp - 1);
                        emit_binary_op(
                            &mut b,
                            f_bin_slow,
                            f_free,
                            ptr_ty,
                            p_ctx,
                            frame_slot,
                            s0,
                            s1,
                            s0,
                            scratch0_off,
                            insn.op,
                            exc_block,
                        );
                        emit_free_and_undef(&mut b, f_free, ptr_ty, p_ctx, frame_slot, s1);
                    }

                    // Control flow
                    Op::Goto(target) => {
                        if target <= insn.pc {
                            let c = b.ins().call(f_poll, &[p_ctx]);
                            let interrupted = b.inst_results(c)[0];
                            emit_check_exc(&mut b, interrupted, exc_block);
                        }
                        let dst_blk = pc_blocks[&target];
                        b.ins().jump(dst_blk, &[]);
                        terminated = true;
                    }
                    Op::IfFalse(target) => {
                        let s0 = slot_off(sp - 1);
                        let truthy = emit_to_bool(&mut b, f_to_bool, ptr_ty, p_ctx, frame_slot, s0, exc_block);
                        emit_free_and_undef(&mut b, f_free, ptr_ty, p_ctx, frame_slot, s0);
                        if target <= insn.pc {
                            let c = b.ins().call(f_poll, &[p_ctx]);
                            let interrupted = b.inst_results(c)[0];
                            emit_check_exc(&mut b, interrupted, exc_block);
                        }
                        let target_blk = pc_blocks[&target];
                        let fall_blk = pc_blocks[&insn.next_pc];
                        b.ins().brif(truthy, fall_blk, &[], target_blk, &[]);
                        terminated = true;
                    }
                    Op::IfTrue(target) => {
                        let s0 = slot_off(sp - 1);
                        let truthy = emit_to_bool(&mut b, f_to_bool, ptr_ty, p_ctx, frame_slot, s0, exc_block);
                        emit_free_and_undef(&mut b, f_free, ptr_ty, p_ctx, frame_slot, s0);
                        if target <= insn.pc {
                            let c = b.ins().call(f_poll, &[p_ctx]);
                            let interrupted = b.inst_results(c)[0];
                            emit_check_exc(&mut b, interrupted, exc_block);
                        }
                        let target_blk = pc_blocks[&target];
                        let fall_blk = pc_blocks[&insn.next_pc];
                        b.ins().brif(truthy, target_blk, &[], fall_blk, &[]);
                        terminated = true;
                    }

                    // Calls
                    Op::Call(argc) => {
                        let argc_u = argc as u32;
                        let func_depth = sp - 1 - argc_u;
                        let func_off = slot_off(func_depth);
                        let argv_off = slot_off(func_depth + 1);

                        // this_obj = undefined in scratch1
                        store_slot_const(&mut b, frame_slot, scratch1_off, 0, TAG_UNDEFINED);
                        let func_ptr = b.ins().stack_addr(ptr_ty, frame_slot, func_off);
                        let this_ptr = b.ins().stack_addr(ptr_ty, frame_slot, scratch1_off);
                        let argv_ptr = b.ins().stack_addr(ptr_ty, frame_slot, argv_off);
                        let out_ptr = b.ins().stack_addr(ptr_ty, frame_slot, scratch0_off);
                        let argc_val = b.ins().iconst(types::I32, argc as i64);

                        let c = b.ins().call(
                            f_call,
                            &[p_ctx, func_ptr, this_ptr, argc_val, argv_ptr, out_ptr],
                        );
                        let err = b.inst_results(c)[0];

                        // Free func and all argument slots.
                        for i in 0..=argc_u {
                            let s = slot_off(func_depth + i);
                            emit_free_and_undef(&mut b, f_free, ptr_ty, p_ctx, frame_slot, s);
                        }
                        emit_check_exc(&mut b, err, exc_block);
                        copy_slot(&mut b, frame_slot, scratch0_off, func_off);
                    }
                    Op::CallMethod(argc) => {
                        let argc_u = argc as u32;
                        let this_depth = sp - 2 - argc_u;
                        let this_off = slot_off(this_depth);
                        let func_off = slot_off(this_depth + 1);
                        let argv_off = slot_off(this_depth + 2);

                        let func_ptr = b.ins().stack_addr(ptr_ty, frame_slot, func_off);
                        let this_ptr = b.ins().stack_addr(ptr_ty, frame_slot, this_off);
                        let argv_ptr = b.ins().stack_addr(ptr_ty, frame_slot, argv_off);
                        let out_ptr = b.ins().stack_addr(ptr_ty, frame_slot, scratch0_off);
                        let argc_val = b.ins().iconst(types::I32, argc as i64);

                        let c = b.ins().call(
                            f_call,
                            &[p_ctx, func_ptr, this_ptr, argc_val, argv_ptr, out_ptr],
                        );
                        let err = b.inst_results(c)[0];

                        for i in 0..(2 + argc_u) {
                            let s = slot_off(this_depth + i);
                            emit_free_and_undef(&mut b, f_free, ptr_ty, p_ctx, frame_slot, s);
                        }
                        emit_check_exc(&mut b, err, exc_block);
                        copy_slot(&mut b, frame_slot, scratch0_off, this_off);
                    }

                    // Returns
                    Op::Return => {
                        let ret_src = slot_off(sp - 1);
                        // Move return value into *ret_out and clear its stack slot so cleanup won't free it.
                        let u = sload(&mut b, frame_slot, ret_src);
                        let tag = sload(&mut b, frame_slot, ret_src + 8);
                        b.ins().store(MemFlagsData::trusted(), u, p_ret_out, 0);
                        b.ins().store(MemFlagsData::trusted(), tag, p_ret_out, 8);
                        store_slot_const(&mut b, frame_slot, ret_src, 0, TAG_UNDEFINED);
                        emit_cleanup_all(&mut b, f_free, ptr_ty, p_ctx, frame_slot, total_slots);
                        let zero = b.ins().iconst(types::I32, 0);
                        b.ins().return_(&[zero]);
                        terminated = true;
                    }
                    Op::ReturnUndef => {
                        store_raw_ptr(&mut b, p_ret_out, 0, TAG_UNDEFINED);
                        emit_cleanup_all(&mut b, f_free, ptr_ty, p_ctx, frame_slot, total_slots);
                        let zero = b.ins().iconst(types::I32, 0);
                        b.ins().return_(&[zero]);
                        terminated = true;
                    }
                }
            }

            if !terminated {
                store_raw_ptr(&mut b, p_ret_out, 0, TAG_UNDEFINED);
                emit_cleanup_all(&mut b, f_free, ptr_ty, p_ctx, frame_slot, total_slots);
                let zero = b.ins().iconst(types::I32, 0);
                b.ins().return_(&[zero]);
            }

            // Build exception epilogue block.
            b.switch_to_block(exc_block);
            emit_cleanup_all(&mut b, f_free, ptr_ty, p_ctx, frame_slot, total_slots);
            store_raw_ptr(&mut b, p_ret_out, 0, TAG_EXCEPTION);
            let zero = b.ins().iconst(types::I32, 0);
            b.ins().return_(&[zero]);

            b.seal_all_blocks();
            b.finalize(self.module.target_config());
        }

        self.module
            .define_function(func_id, &mut ctx)
            .map_err(|e| format!("cranelift define_function: {e}"))?;
        self.module.clear_context(&mut ctx);
        self.module
            .finalize_definitions()
            .map_err(|e| format!("cranelift finalize_definitions: {e}"))?;

        let code_ptr = self.module.get_finalized_function(func_id);
        let compiled: CompiledFn = unsafe { std::mem::transmute(code_ptr) };
        Ok(compiled)
    }
}

fn sload(b: &mut FunctionBuilder, slot: StackSlot, off: i32) -> Value {
    b.ins().stack_load(types::I64, types::I64, slot, off)
}

fn sstore(b: &mut FunctionBuilder, val: Value, slot: StackSlot, off: i32) {
    b.ins().stack_store(types::I64, val, slot, off);
}

fn store_raw_ptr(b: &mut FunctionBuilder, ptr: Value, u: i64, tag: i64) {
    let u_val = b.ins().iconst(types::I64, u);
    let tag_val = b.ins().iconst(types::I64, tag);
    b.ins().store(MemFlagsData::trusted(), u_val, ptr, 0);
    b.ins().store(MemFlagsData::trusted(), tag_val, ptr, 8);
}

fn store_slot_const(b: &mut FunctionBuilder, slot: StackSlot, off: i32, u: i64, tag: i64) {
    let u_val = b.ins().iconst(types::I64, u);
    let tag_val = b.ins().iconst(types::I64, tag);
    sstore(b, u_val, slot, off);
    sstore(b, tag_val, slot, off + 8);
}

fn copy_slot(b: &mut FunctionBuilder, slot: StackSlot, src_off: i32, dst_off: i32) -> Value {
    let u = sload(b, slot, src_off);
    let tag = sload(b, slot, src_off + 8);
    sstore(b, u, slot, dst_off);
    sstore(b, tag, slot, dst_off + 8);
    tag
}

fn emit_dup_if_ref(
    b: &mut FunctionBuilder,
    f_dup: cranelift_codegen::ir::FuncRef,
    ptr_ty: types::Type,
    p_ctx: Value,
    slot: StackSlot,
    off: i32,
    tag: Value,
) {
    let is_ref = b.ins().icmp_imm_s(IntCC::SignedLessThan, tag, 0);
    let dup_blk = b.create_block();
    let cont_blk = b.create_block();
    b.ins().brif(is_ref, dup_blk, &[], cont_blk, &[]);
    b.switch_to_block(dup_blk);
    let addr = b.ins().stack_addr(ptr_ty, slot, off);
    b.ins().call(f_dup, &[p_ctx, addr]);
    b.ins().jump(cont_blk, &[]);
    b.switch_to_block(cont_blk);
}

fn emit_free_slot(
    b: &mut FunctionBuilder,
    f_free: cranelift_codegen::ir::FuncRef,
    ptr_ty: types::Type,
    p_ctx: Value,
    slot: StackSlot,
    off: i32,
) {
    let tag = b.ins().stack_load(types::I64, types::I64, slot, off + 8);
    let is_ref = b.ins().icmp_imm_s(IntCC::SignedLessThan, tag, 0);
    let free_blk = b.create_block();
    let cont_blk = b.create_block();
    b.ins().brif(is_ref, free_blk, &[], cont_blk, &[]);
    b.switch_to_block(free_blk);
    let addr = b.ins().stack_addr(ptr_ty, slot, off);
    b.ins().call(f_free, &[p_ctx, addr]);
    b.ins().jump(cont_blk, &[]);
    b.switch_to_block(cont_blk);
}

fn emit_free_and_undef(
    b: &mut FunctionBuilder,
    f_free: cranelift_codegen::ir::FuncRef,
    ptr_ty: types::Type,
    p_ctx: Value,
    slot: StackSlot,
    off: i32,
) {
    emit_free_slot(b, f_free, ptr_ty, p_ctx, slot, off);
    store_slot_const(b, slot, off, 0, TAG_UNDEFINED);
}

fn emit_cleanup_all(
    b: &mut FunctionBuilder,
    f_free: cranelift_codegen::ir::FuncRef,
    ptr_ty: types::Type,
    p_ctx: Value,
    slot: StackSlot,
    total_slots: u32,
) {
    for i in 0..total_slots {
        emit_free_slot(b, f_free, ptr_ty, p_ctx, slot, (i * JSVAL_SIZE) as i32);
    }
}

fn emit_check_exc(b: &mut FunctionBuilder, err_i32: Value, exc_block: Block) {
    let ok_blk = b.create_block();
    b.ins().brif(err_i32, exc_block, &[], ok_blk, &[]);
    b.switch_to_block(ok_blk);
}

/// Evaluates JS ToBool with an inline fast path for INT/BOOL/NULL/UNDEFINED.
fn emit_to_bool(
    b: &mut FunctionBuilder,
    f_to_bool: cranelift_codegen::ir::FuncRef,
    ptr_ty: types::Type,
    p_ctx: Value,
    slot: StackSlot,
    off: i32,
    exc_block: Block,
) -> Value {
    let u = b.ins().stack_load(types::I64, types::I64, slot, off);
    let tag = b.ins().stack_load(types::I64, types::I64, slot, off + 8);

    // QuickJS fast path: if (uint64_t)tag <= JS_TAG_UNDEFINED, truthiness is (u as i32) != 0!
    // (For NULL and UNDEFINED, u is always 0; for BOOL, u is 0 or 1; for INT, u is i32.)
    let is_fast = b.ins().icmp_imm_s(IntCC::UnsignedLessThanOrEqual, tag, TAG_UNDEFINED);
    let fast_blk = b.create_block();
    let slow_blk = b.create_block();
    let merge_blk = b.create_block();
    b.append_block_param(merge_blk, types::I32);

    b.ins().brif(is_fast, fast_blk, &[], slow_blk, &[]);

    b.switch_to_block(fast_blk);
    let u32_val = b.ins().ireduce(types::I32, u);
    let nz = b.ins().icmp_imm_s(IntCC::NotEqual, u32_val, 0);
    let fast_res = b.ins().uextend(types::I32, nz);
    b.ins().jump(merge_blk, &[fast_res.into()]);

    b.switch_to_block(slow_blk);
    let addr = b.ins().stack_addr(ptr_ty, slot, off);
    let c = b.ins().call(f_to_bool, &[p_ctx, addr]);
    let slow_res = b.inst_results(c)[0];
    let is_err = b.ins().icmp_imm_s(IntCC::SignedLessThan, slow_res, 0);
    let slow_ok = b.create_block();
    b.ins().brif(is_err, exc_block, &[], slow_ok, &[]);
    b.switch_to_block(slow_ok);
    b.ins().jump(merge_blk, &[slow_res.into()]);

    b.switch_to_block(merge_blk);
    b.block_params(merge_blk)[0]
}

#[allow(clippy::too_many_arguments)]
fn emit_inc_dec_loc(
    b: &mut FunctionBuilder,
    f_un_slow: cranelift_codegen::ir::FuncRef,
    f_free: cranelift_codegen::ir::FuncRef,
    ptr_ty: types::Type,
    p_ctx: Value,
    slot: StackSlot,
    loc_off: i32,
    scratch_off: i32,
    delta: i64,
    slow_opcode: i32,
    exc_block: Block,
) {
    let u = b.ins().stack_load(types::I64, types::I64, slot, loc_off);
    let tag = b.ins().stack_load(types::I64, types::I64, slot, loc_off + 8);

    let is_int = b.ins().icmp_imm_s(IntCC::Equal, tag, TAG_INT);
    let fast_blk = b.create_block();
    let slow_blk = b.create_block();
    let done_blk = b.create_block();
    b.ins().brif(is_int, fast_blk, &[], slow_blk, &[]);

    b.switch_to_block(fast_blk);
    let i32_v = b.ins().ireduce(types::I32, u);
    let i64_v = b.ins().sextend(types::I64, i32_v);
    let sum = b.ins().iadd_imm_s(i64_v, delta);
    let sum32 = b.ins().ireduce(types::I32, sum);
    let check = b.ins().sextend(types::I64, sum32);
    let fits = b.ins().icmp(IntCC::Equal, sum, check);
    let fit_blk = b.create_block();
    let ovf_blk = b.create_block();
    b.ins().brif(fits, fit_blk, &[], ovf_blk, &[]);

    b.switch_to_block(fit_blk);
    let sum_u = b.ins().uextend(types::I64, sum32);
    b.ins().stack_store(types::I64, sum_u, slot, loc_off);
    b.ins().jump(done_blk, &[]);

    b.switch_to_block(ovf_blk);
    let f = b.ins().fcvt_from_sint(types::F64, sum);
    let fbits = b.ins().bitcast(types::I64, MemFlagsData::new(), f);
    let ftag = b.ins().iconst(types::I64, TAG_FLOAT64);
    b.ins().stack_store(types::I64, fbits, slot, loc_off);
    b.ins().stack_store(types::I64, ftag, slot, loc_off + 8);
    b.ins().jump(done_blk, &[]);

    b.switch_to_block(slow_blk);
    let src_ptr = b.ins().stack_addr(ptr_ty, slot, loc_off);
    let out_ptr = b.ins().stack_addr(ptr_ty, slot, scratch_off);
    let op_v = b.ins().iconst(types::I32, slow_opcode as i64);
    let c = b.ins().call(f_un_slow, &[p_ctx, op_v, src_ptr, out_ptr]);
    let err = b.inst_results(c)[0];
    emit_check_exc(b, err, exc_block);
    emit_free_slot(b, f_free, ptr_ty, p_ctx, slot, loc_off);
    copy_slot(b, slot, scratch_off, loc_off);
    b.ins().jump(done_blk, &[]);

    b.switch_to_block(done_blk);
}

#[allow(clippy::too_many_arguments)]
fn emit_unary_op(
    b: &mut FunctionBuilder,
    f_un_slow: cranelift_codegen::ir::FuncRef,
    f_free: cranelift_codegen::ir::FuncRef,
    ptr_ty: types::Type,
    p_ctx: Value,
    slot: StackSlot,
    s0: i32,
    scratch_off: i32,
    op: Op,
    exc_block: Block,
) {
    let u = b.ins().stack_load(types::I64, types::I64, slot, s0);
    let tag = b.ins().stack_load(types::I64, types::I64, slot, s0 + 8);
    let slow_opcode = match op {
        Op::Neg => sys::QJSJitOp_QJS_OP_neg as i32,
        Op::Plus => sys::QJSJitOp_QJS_OP_plus as i32,
        Op::Inc => sys::QJSJitOp_QJS_OP_inc as i32,
        Op::Dec => sys::QJSJitOp_QJS_OP_dec as i32,
        Op::Not => sys::QJSJitOp_QJS_OP_not as i32,
        _ => unreachable!(),
    };

    let is_int = b.ins().icmp_imm_s(IntCC::Equal, tag, TAG_INT);
    let fast_blk = b.create_block();
    let slow_blk = b.create_block();
    let done_blk = b.create_block();
    b.ins().brif(is_int, fast_blk, &[], slow_blk, &[]);

    b.switch_to_block(fast_blk);
    let i32_v = b.ins().ireduce(types::I32, u);
    match op {
        Op::Plus => {
            // +int32 is a no-op
            b.ins().jump(done_blk, &[]);
        }
        Op::Not => {
            let res32 = b.ins().bnot(i32_v);
            let res_u = b.ins().uextend(types::I64, res32);
            b.ins().stack_store(types::I64, res_u, slot, s0);
            b.ins().jump(done_blk, &[]);
        }
        Op::Neg => {
            // -0 must become -0.0 (float64) and -INT32_MIN overflows to float64!
            let is_zero = b.ins().icmp_imm_s(IntCC::Equal, i32_v, 0);
            let is_min = b.ins().icmp_imm_s(IntCC::Equal, i32_v, i32::MIN as i64);
            let special = b.ins().bor(is_zero, is_min);
            let normal_blk = b.create_block();
            b.ins().brif(special, slow_blk, &[], normal_blk, &[]);
            b.switch_to_block(normal_blk);
            let neg32 = b.ins().ineg(i32_v);
            let neg_u = b.ins().uextend(types::I64, neg32);
            b.ins().stack_store(types::I64, neg_u, slot, s0);
            b.ins().jump(done_blk, &[]);
        }
        Op::Inc | Op::Dec => {
            let delta = if matches!(op, Op::Inc) { 1 } else { -1 };
            let i64_v = b.ins().sextend(types::I64, i32_v);
            let sum = b.ins().iadd_imm_s(i64_v, delta);
            let sum32 = b.ins().ireduce(types::I32, sum);
            let check = b.ins().sextend(types::I64, sum32);
            let fits = b.ins().icmp(IntCC::Equal, sum, check);
            let fit_blk = b.create_block();
            let ovf_blk = b.create_block();
            b.ins().brif(fits, fit_blk, &[], ovf_blk, &[]);
            b.switch_to_block(fit_blk);
            let sum_u = b.ins().uextend(types::I64, sum32);
            b.ins().stack_store(types::I64, sum_u, slot, s0);
            b.ins().jump(done_blk, &[]);
            b.switch_to_block(ovf_blk);
            let f = b.ins().fcvt_from_sint(types::F64, sum);
            let fbits = b.ins().bitcast(types::I64, MemFlagsData::new(), f);
            let ftag = b.ins().iconst(types::I64, TAG_FLOAT64);
            b.ins().stack_store(types::I64, fbits, slot, s0);
            b.ins().stack_store(types::I64, ftag, slot, s0 + 8);
            b.ins().jump(done_blk, &[]);
        }
        _ => unreachable!(),
    }

    b.switch_to_block(slow_blk);
    let src_ptr = b.ins().stack_addr(ptr_ty, slot, s0);
    let out_ptr = b.ins().stack_addr(ptr_ty, slot, scratch_off);
    let op_v = b.ins().iconst(types::I32, slow_opcode as i64);
    let c = b.ins().call(f_un_slow, &[p_ctx, op_v, src_ptr, out_ptr]);
    let err = b.inst_results(c)[0];
    emit_check_exc(b, err, exc_block);
    emit_free_slot(b, f_free, ptr_ty, p_ctx, slot, s0);
    copy_slot(b, slot, scratch_off, s0);
    b.ins().jump(done_blk, &[]);

    b.switch_to_block(done_blk);
}

#[allow(clippy::too_many_arguments)]
fn emit_binary_op(
    b: &mut FunctionBuilder,
    f_bin_slow: cranelift_codegen::ir::FuncRef,
    f_free: cranelift_codegen::ir::FuncRef,
    ptr_ty: types::Type,
    p_ctx: Value,
    slot: StackSlot,
    lhs_off: i32,
    rhs_off: i32,
    dst_off: i32,
    scratch_off: i32,
    op: Op,
    exc_block: Block,
) {
    let slow_opcode = match op {
        Op::Add => sys::QJSJitOp_QJS_OP_add as i32,
        Op::Sub => sys::QJSJitOp_QJS_OP_sub as i32,
        Op::Mul => sys::QJSJitOp_QJS_OP_mul as i32,
        Op::Div => sys::QJSJitOp_QJS_OP_div as i32,
        Op::Mod => sys::QJSJitOp_QJS_OP_mod as i32,
        Op::Pow => sys::QJSJitOp_QJS_OP_pow as i32,
        Op::Shl => sys::QJSJitOp_QJS_OP_shl as i32,
        Op::Sar => sys::QJSJitOp_QJS_OP_sar as i32,
        Op::Shr => sys::QJSJitOp_QJS_OP_shr as i32,
        Op::And => sys::QJSJitOp_QJS_OP_and as i32,
        Op::Or => sys::QJSJitOp_QJS_OP_or as i32,
        Op::Xor => sys::QJSJitOp_QJS_OP_xor as i32,
        Op::Lt => sys::QJSJitOp_QJS_OP_lt as i32,
        Op::Lte => sys::QJSJitOp_QJS_OP_lte as i32,
        Op::Gt => sys::QJSJitOp_QJS_OP_gt as i32,
        Op::Gte => sys::QJSJitOp_QJS_OP_gte as i32,
        Op::Eq => sys::QJSJitOp_QJS_OP_eq as i32,
        Op::Neq => sys::QJSJitOp_QJS_OP_neq as i32,
        Op::StrictEq => sys::QJSJitOp_QJS_OP_strict_eq as i32,
        Op::StrictNeq => sys::QJSJitOp_QJS_OP_strict_neq as i32,
        _ => unreachable!(),
    };

    let u1 = b.ins().stack_load(types::I64, types::I64, slot, lhs_off);
    let t1 = b.ins().stack_load(types::I64, types::I64, slot, lhs_off + 8);
    let u2 = b.ins().stack_load(types::I64, types::I64, slot, rhs_off);
    let t2 = b.ins().stack_load(types::I64, types::I64, slot, rhs_off + 8);

    let t1_int = b.ins().icmp_imm_s(IntCC::Equal, t1, TAG_INT);
    let t2_int = b.ins().icmp_imm_s(IntCC::Equal, t2, TAG_INT);
    let both_int = b.ins().band(t1_int, t2_int);

    let int_blk = b.create_block();
    let non_int_blk = b.create_block();
    let slow_blk = b.create_block();
    let done_blk = b.create_block();

    b.ins().brif(both_int, int_blk, &[], non_int_blk, &[]);

    // ── Int32 fast path ──
    b.switch_to_block(int_blk);
    let a32 = b.ins().ireduce(types::I32, u1);
    let b32 = b.ins().ireduce(types::I32, u2);
    match op {
        Op::Add | Op::Sub | Op::Mul => {
            let a64 = b.ins().sextend(types::I64, a32);
            let b64 = b.ins().sextend(types::I64, b32);
            let res64 = match op {
                Op::Add => b.ins().iadd(a64, b64),
                Op::Sub => b.ins().isub(a64, b64),
                Op::Mul => {
                    // Note: 0 * negative must be -0.0 (float64) in JS!
                    // Check if res == 0 && (a < 0 || b < 0) -> slow path.
                    b.ins().imul(a64, b64)
                }
                _ => unreachable!(),
            };
            if matches!(op, Op::Mul) {
                let res_zero = b.ins().icmp_imm_s(IntCC::Equal, res64, 0);
                let any_neg = b.ins().bor(a32, b32);
                let is_neg = b.ins().icmp_imm_s(IntCC::SignedLessThan, any_neg, 0);
                let neg_zero = b.ins().band(res_zero, is_neg);
                let mul_ok = b.create_block();
                b.ins().brif(neg_zero, slow_blk, &[], mul_ok, &[]);
                b.switch_to_block(mul_ok);
            }
            let res32 = b.ins().ireduce(types::I32, res64);
            let check = b.ins().sextend(types::I64, res32);
            let fits = b.ins().icmp(IntCC::Equal, res64, check);
            let fit_blk = b.create_block();
            let ovf_blk = b.create_block();
            b.ins().brif(fits, fit_blk, &[], ovf_blk, &[]);

            b.switch_to_block(fit_blk);
            let res_u = b.ins().uextend(types::I64, res32);
            let int_tag = b.ins().iconst(types::I64, TAG_INT);
            b.ins().stack_store(types::I64, res_u, slot, dst_off);
            b.ins().stack_store(types::I64, int_tag, slot, dst_off + 8);
            b.ins().jump(done_blk, &[]);

            b.switch_to_block(ovf_blk);
            let f = b.ins().fcvt_from_sint(types::F64, res64);
            let fbits = b.ins().bitcast(types::I64, MemFlagsData::new(), f);
            let ftag = b.ins().iconst(types::I64, TAG_FLOAT64);
            b.ins().stack_store(types::I64, fbits, slot, dst_off);
            b.ins().stack_store(types::I64, ftag, slot, dst_off + 8);
            b.ins().jump(done_blk, &[]);
        }
        Op::Div => {
            let fa = b.ins().fcvt_from_sint(types::F64, a32);
            let fb = b.ins().fcvt_from_sint(types::F64, b32);
            let fq = b.ins().fdiv(fa, fb);
            let fbits = b.ins().bitcast(types::I64, MemFlagsData::new(), fq);
            let ftag = b.ins().iconst(types::I64, TAG_FLOAT64);
            b.ins().stack_store(types::I64, fbits, slot, dst_off);
            b.ins().stack_store(types::I64, ftag, slot, dst_off + 8);
            b.ins().jump(done_blk, &[]);
        }
        Op::Mod => {
            // Fast path: b32 > 0 && a32 >= 0 avoids -0.0 and INT32_MIN % -1 traps.
            let b_pos = b.ins().icmp_imm_s(IntCC::SignedGreaterThan, b32, 0);
            let a_nonneg = b.ins().icmp_imm_s(IntCC::SignedGreaterThanOrEqual, a32, 0);
            let safe = b.ins().band(b_pos, a_nonneg);
            let safe_blk = b.create_block();
            b.ins().brif(safe, safe_blk, &[], slow_blk, &[]);
            b.switch_to_block(safe_blk);
            let rem = b.ins().srem(a32, b32);
            let rem_u = b.ins().uextend(types::I64, rem);
            let int_tag = b.ins().iconst(types::I64, TAG_INT);
            b.ins().stack_store(types::I64, rem_u, slot, dst_off);
            b.ins().stack_store(types::I64, int_tag, slot, dst_off + 8);
            b.ins().jump(done_blk, &[]);
        }
        Op::Pow => {
            b.ins().jump(slow_blk, &[]);
        }
        Op::And | Op::Or | Op::Xor | Op::Shl | Op::Sar => {
            let res32 = match op {
                Op::And => b.ins().band(a32, b32),
                Op::Or => b.ins().bor(a32, b32),
                Op::Xor => b.ins().bxor(a32, b32),
                Op::Shl => {
                    let sh = b.ins().band_imm_s(b32, 31);
                    b.ins().ishl(a32, sh)
                }
                Op::Sar => {
                    let sh = b.ins().band_imm_s(b32, 31);
                    b.ins().sshr(a32, sh)
                }
                _ => unreachable!(),
            };
            let res_u = b.ins().uextend(types::I64, res32);
            let int_tag = b.ins().iconst(types::I64, TAG_INT);
            b.ins().stack_store(types::I64, res_u, slot, dst_off);
            b.ins().stack_store(types::I64, int_tag, slot, dst_off + 8);
            b.ins().jump(done_blk, &[]);
        }
        Op::Shr => {
            let sh = b.ins().band_imm_s(b32, 31);
            let res32 = b.ins().ushr(a32, sh);
            let is_neg_signed = b.ins().icmp_imm_s(IntCC::SignedLessThan, res32, 0);
            let fits_blk = b.create_block();
            let u32_f64_blk = b.create_block();
            b.ins().brif(is_neg_signed, u32_f64_blk, &[], fits_blk, &[]);

            b.switch_to_block(fits_blk);
            let res_u = b.ins().uextend(types::I64, res32);
            let int_tag = b.ins().iconst(types::I64, TAG_INT);
            b.ins().stack_store(types::I64, res_u, slot, dst_off);
            b.ins().stack_store(types::I64, int_tag, slot, dst_off + 8);
            b.ins().jump(done_blk, &[]);

            b.switch_to_block(u32_f64_blk);
            let f = b.ins().fcvt_from_uint(types::F64, res32);
            let fbits = b.ins().bitcast(types::I64, MemFlagsData::new(), f);
            let ftag = b.ins().iconst(types::I64, TAG_FLOAT64);
            b.ins().stack_store(types::I64, fbits, slot, dst_off);
            b.ins().stack_store(types::I64, ftag, slot, dst_off + 8);
            b.ins().jump(done_blk, &[]);
        }
        Op::Lt
        | Op::Lte
        | Op::Gt
        | Op::Gte
        | Op::Eq
        | Op::Neq
        | Op::StrictEq
        | Op::StrictNeq => {
            let cc = match op {
                Op::Lt => IntCC::SignedLessThan,
                Op::Lte => IntCC::SignedLessThanOrEqual,
                Op::Gt => IntCC::SignedGreaterThan,
                Op::Gte => IntCC::SignedGreaterThanOrEqual,
                Op::Eq | Op::StrictEq => IntCC::Equal,
                Op::Neq | Op::StrictNeq => IntCC::NotEqual,
                _ => unreachable!(),
            };
            let cmp = b.ins().icmp(cc, a32, b32);
            let res_u = b.ins().uextend(types::I64, cmp);
            let tag_bool = b.ins().iconst(types::I64, TAG_BOOL);
            b.ins().stack_store(types::I64, res_u, slot, dst_off);
            b.ins().stack_store(types::I64, tag_bool, slot, dst_off + 8);
            b.ins().jump(done_blk, &[]);
        }
        _ => unreachable!(),
    }

    // ── Float64 fast path (for numeric arithmetic & relational comparisons) ──
    b.switch_to_block(non_int_blk);
    let supports_f64 = matches!(
        op,
        Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Lt | Op::Lte | Op::Gt | Op::Gte
    );
    if supports_f64 {
        let t1_f64 = b.ins().icmp_imm_s(IntCC::Equal, t1, TAG_FLOAT64);
        let t2_f64 = b.ins().icmp_imm_s(IntCC::Equal, t2, TAG_FLOAT64);
        let t1_num = b.ins().bor(t1_int, t1_f64);
        let t2_num = b.ins().bor(t2_int, t2_f64);
        let both_num = b.ins().band(t1_num, t2_num);

        let f64_blk = b.create_block();
        b.ins().brif(both_num, f64_blk, &[], slow_blk, &[]);

        b.switch_to_block(f64_blk);
        let fa = load_as_f64(b, u1, t1_int);
        let fb = load_as_f64(b, u2, t2_int);
        match op {
            Op::Add | Op::Sub | Op::Mul | Op::Div => {
                let fr = match op {
                    Op::Add => b.ins().fadd(fa, fb),
                    Op::Sub => b.ins().fsub(fa, fb),
                    Op::Mul => b.ins().fmul(fa, fb),
                    Op::Div => b.ins().fdiv(fa, fb),
                    _ => unreachable!(),
                };
                let fbits = b.ins().bitcast(types::I64, MemFlagsData::new(), fr);
                let ftag = b.ins().iconst(types::I64, TAG_FLOAT64);
                b.ins().stack_store(types::I64, fbits, slot, dst_off);
                b.ins().stack_store(types::I64, ftag, slot, dst_off + 8);
                b.ins().jump(done_blk, &[]);
            }
            Op::Lt | Op::Lte | Op::Gt | Op::Gte => {
                let cc = match op {
                    Op::Lt => FloatCC::LessThan,
                    Op::Lte => FloatCC::LessThanOrEqual,
                    Op::Gt => FloatCC::GreaterThan,
                    Op::Gte => FloatCC::GreaterThanOrEqual,
                    _ => unreachable!(),
                };
                let cmp = b.ins().fcmp(cc, fa, fb);
                let res_u = b.ins().uextend(types::I64, cmp);
                let tag_bool = b.ins().iconst(types::I64, TAG_BOOL);
                b.ins().stack_store(types::I64, res_u, slot, dst_off);
                b.ins().stack_store(types::I64, tag_bool, slot, dst_off + 8);
                b.ins().jump(done_blk, &[]);
            }
            _ => unreachable!(),
        }
    } else {
        b.ins().jump(slow_blk, &[]);
    }

    // ── Generic QuickJS slow path (strings, objects, BigInt, NaN/Coercion, etc.) ──
    b.switch_to_block(slow_blk);
    let lhs_ptr = b.ins().stack_addr(ptr_ty, slot, lhs_off);
    let rhs_ptr = b.ins().stack_addr(ptr_ty, slot, rhs_off);
    let out_ptr = b.ins().stack_addr(ptr_ty, slot, scratch_off);
    let op_v = b.ins().iconst(types::I32, slow_opcode as i64);
    let c = b
        .ins()
        .call(f_bin_slow, &[p_ctx, op_v, lhs_ptr, rhs_ptr, out_ptr]);
    let err = b.inst_results(c)[0];
    emit_check_exc(b, err, exc_block);
    emit_free_slot(b, f_free, ptr_ty, p_ctx, slot, dst_off);
    copy_slot(b, slot, scratch_off, dst_off);
    b.ins().jump(done_blk, &[]);

    b.switch_to_block(done_blk);
}

fn load_as_f64(b: &mut FunctionBuilder, u: Value, is_int: Value) -> Value {
    let i32_v = b.ins().ireduce(types::I32, u);
    let from_int = b.ins().fcvt_from_sint(types::F64, i32_v);
    let from_f64 = b.ins().bitcast(types::F64, MemFlagsData::new(), u);
    b.ins().select(is_int, from_int, from_f64)
}
