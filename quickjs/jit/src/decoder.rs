//! QuickJS-NG bytecode decoder and CFG/stack-depth validator for the baseline JIT.

use rong_quickjs_sys as sys;
use std::collections::{BTreeSet, HashMap, VecDeque};

/// Reason why a `JSFunctionBytecode` was not compiled by the baseline JIT.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectReason {
    NotNormalFunction,
    HasCapturedLocals,
    BytecodeTooLarge,
    EmptyBytecode,
    TruncatedBytecode,
    InvalidBranchTarget,
    StackMismatch,
    StackOverflow,
    UnsupportedOpcode(u8),
}

/// Decoded QuickJS instruction normalized into a compact enum for Cranelift lowering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Nop,
    // Constants & literals
    PushI32(i32),
    PushConst(u32),
    PushUndefined,
    PushNull,
    PushBool(bool),
    PushThis,

    // Stack manipulation
    Drop,
    Nip,
    Dup,
    Dup1,
    Swap,

    // Locals & arguments
    GetLoc(u16),
    GetLocCheck(u16),
    GetLoc0Loc1,
    PutLoc(u16),
    PutLocCheck(u16),
    SetLoc(u16),
    SetLocUninitialized(u16),
    GetArg(u16),
    PutArg(u16),
    SetArg(u16),

    // Closure variables (reading/writing outer captured variables)
    GetVarRef(u16),
    PutVarRef(u16),
    SetVarRef(u16),

    // Global variables
    GetVar(u32),
    GetVarUndef(u32),
    PutVar(u32),

    // Object / array properties
    GetField(u32),
    GetField2(u32),
    PutField(u32),
    GetArrayEl,
    GetArrayEl2,
    PutArrayEl,
    GetLength,

    // In-place local numeric ops
    IncLoc(u16),
    DecLoc(u16),
    AddLoc(u16),

    // Unary ops
    Neg,
    Plus,
    Inc,
    Dec,
    PostInc,
    PostDec,
    Not,
    LNot,
    IsUndefined,
    IsNull,
    IsUndefinedOrNull,

    // Binary ops
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Shl,
    Sar,
    Shr,
    And,
    Or,
    Xor,
    Lt,
    Lte,
    Gt,
    Gte,
    Eq,
    Neq,
    StrictEq,
    StrictNeq,

    // Control flow
    Goto(usize),
    IfFalse(usize),
    IfTrue(usize),

    // Calls & returns
    Call(u16),
    CallMethod(u16),
    Return,
    ReturnUndef,
}

#[derive(Debug, Clone, Copy)]
pub struct DecodedInsn {
    pub pc: usize,
    pub next_pc: usize,
    pub stack_before: u16,
    pub op: Op,
}

#[derive(Debug)]
pub struct DecodedFunction {
    pub insns: Vec<DecodedInsn>,
    pub block_starts: BTreeSet<usize>,
    pub arg_count: u16,
    pub var_count: u16,
    pub stack_size: u16,
}

fn read_u8(buf: &[u8], pos: usize) -> Result<u8, RejectReason> {
    buf.get(pos).copied().ok_or(RejectReason::TruncatedBytecode)
}

fn read_i8(buf: &[u8], pos: usize) -> Result<i8, RejectReason> {
    Ok(read_u8(buf, pos)? as i8)
}

fn read_u16(buf: &[u8], pos: usize) -> Result<u16, RejectReason> {
    let bytes: [u8; 2] = buf
        .get(pos..pos + 2)
        .ok_or(RejectReason::TruncatedBytecode)?
        .try_into()
        .unwrap();
    Ok(u16::from_le_bytes(bytes))
}

fn read_i16(buf: &[u8], pos: usize) -> Result<i16, RejectReason> {
    Ok(read_u16(buf, pos)? as i16)
}

fn read_u32(buf: &[u8], pos: usize) -> Result<u32, RejectReason> {
    let bytes: [u8; 4] = buf
        .get(pos..pos + 4)
        .ok_or(RejectReason::TruncatedBytecode)?
        .try_into()
        .unwrap();
    Ok(u32::from_le_bytes(bytes))
}

fn read_i32(buf: &[u8], pos: usize) -> Result<i32, RejectReason> {
    Ok(read_u32(buf, pos)? as i32)
}

fn branch_target(insn_pc: usize, diff: isize, len: usize) -> Result<usize, RejectReason> {
    let target = (insn_pc as isize) + 1 + diff;
    if target < 0 || (target as usize) >= len {
        return Err(RejectReason::InvalidBranchTarget);
    }
    Ok(target as usize)
}

impl Op {
    /// Returns `(pop_count, push_count)` for operand stack depth verification.
    pub fn stack_effect(self) -> (u16, u16) {
        match self {
            Op::Nop | Op::SetLocUninitialized(_) | Op::IncLoc(_) | Op::DecLoc(_) | Op::Goto(_) => {
                (0, 0)
            }
            Op::PushI32(_)
            | Op::PushConst(_)
            | Op::PushUndefined
            | Op::PushNull
            | Op::PushBool(_)
            | Op::PushThis
            | Op::GetLoc(_)
            | Op::GetLocCheck(_)
            | Op::GetArg(_)
            | Op::GetVarRef(_)
            | Op::GetVar(_)
            | Op::GetVarUndef(_) => (0, 1),
            Op::GetLoc0Loc1 => (0, 2),
            Op::Drop
            | Op::PutLoc(_)
            | Op::PutLocCheck(_)
            | Op::PutArg(_)
            | Op::PutVarRef(_)
            | Op::PutVar(_)
            | Op::AddLoc(_)
            | Op::IfFalse(_)
            | Op::IfTrue(_)
            | Op::Return => (1, 0),
            Op::ReturnUndef => (0, 0),
            Op::Dup | Op::PostInc | Op::PostDec => (1, 2),
            Op::Dup1 => (2, 3),
            Op::Nip => (2, 1),
            Op::Swap => (2, 2),
            Op::SetLoc(_)
            | Op::SetArg(_)
            | Op::SetVarRef(_)
            | Op::GetField(_)
            | Op::GetLength
            | Op::Neg
            | Op::Plus
            | Op::Inc
            | Op::Dec
            | Op::Not
            | Op::LNot
            | Op::IsUndefined
            | Op::IsNull
            | Op::IsUndefinedOrNull => (1, 1),
            Op::GetField2(_) => (1, 2),
            Op::PutField(_) => (2, 0),
            Op::GetArrayEl => (2, 1),
            Op::GetArrayEl2 => (2, 2),
            Op::PutArrayEl => (3, 0),
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
            | Op::StrictNeq => (2, 1),
            Op::Call(argc) => (1 + argc, 1),
            Op::CallMethod(argc) => (2 + argc, 1),
        }
    }
}

/// Decodes and validates a QuickJS function's bytecode buffer.
pub fn decode_bytecode(
    info: &sys::QJSJitBytecodeInfo,
    max_bytecode_len: usize,
) -> Result<DecodedFunction, RejectReason> {
    if info.func_kind != 0 {
        return Err(RejectReason::NotNormalFunction);
    }
    if info.var_ref_count != 0 {
        return Err(RejectReason::HasCapturedLocals);
    }
    let len = info.len as usize;
    if len == 0 || info.buf.is_null() {
        return Err(RejectReason::EmptyBytecode);
    }
    if len > max_bytecode_len {
        return Err(RejectReason::BytecodeTooLarge);
    }

    let buf = unsafe { std::slice::from_raw_parts(info.buf, len) };

    let mut insns = Vec::new();
    let mut pc_to_idx = HashMap::new();
    let mut block_starts = BTreeSet::new();
    block_starts.insert(0usize);

    let mut pc = 0usize;
    while pc < len {
        let raw = buf[pc];
        let op_enum = raw as u32;
        let (op, size) = match op_enum {
            x if x == sys::QJSJitOp_QJS_OP_nop as u32 => (Op::Nop, 1),

            // Push literals
            x if x == sys::QJSJitOp_QJS_OP_push_i32 as u32 => {
                (Op::PushI32(read_i32(buf, pc + 1)?), 5)
            }
            x if x == sys::QJSJitOp_QJS_OP_push_i8 as u32 => {
                (Op::PushI32(read_i8(buf, pc + 1)? as i32), 2)
            }
            x if x == sys::QJSJitOp_QJS_OP_push_i16 as u32 => {
                (Op::PushI32(read_i16(buf, pc + 1)? as i32), 3)
            }
            x if x == sys::QJSJitOp_QJS_OP_push_minus1 as u32 => (Op::PushI32(-1), 1),
            x if x == sys::QJSJitOp_QJS_OP_push_0 as u32 => (Op::PushI32(0), 1),
            x if x == sys::QJSJitOp_QJS_OP_push_1 as u32 => (Op::PushI32(1), 1),
            x if x == sys::QJSJitOp_QJS_OP_push_2 as u32 => (Op::PushI32(2), 1),
            x if x == sys::QJSJitOp_QJS_OP_push_3 as u32 => (Op::PushI32(3), 1),
            x if x == sys::QJSJitOp_QJS_OP_push_4 as u32 => (Op::PushI32(4), 1),
            x if x == sys::QJSJitOp_QJS_OP_push_5 as u32 => (Op::PushI32(5), 1),
            x if x == sys::QJSJitOp_QJS_OP_push_6 as u32 => (Op::PushI32(6), 1),
            x if x == sys::QJSJitOp_QJS_OP_push_7 as u32 => (Op::PushI32(7), 1),
            x if x == sys::QJSJitOp_QJS_OP_push_const as u32 => {
                (Op::PushConst(read_u32(buf, pc + 1)?), 5)
            }
            x if x == sys::QJSJitOp_QJS_OP_push_const8 as u32 => {
                (Op::PushConst(read_u8(buf, pc + 1)? as u32), 2)
            }
            x if x == sys::QJSJitOp_QJS_OP_undefined as u32 => (Op::PushUndefined, 1),
            x if x == sys::QJSJitOp_QJS_OP_null as u32 => (Op::PushNull, 1),
            x if x == sys::QJSJitOp_QJS_OP_push_false as u32 => (Op::PushBool(false), 1),
            x if x == sys::QJSJitOp_QJS_OP_push_true as u32 => (Op::PushBool(true), 1),
            x if x == sys::QJSJitOp_QJS_OP_push_this as u32 => (Op::PushThis, 1),

            // Stack ops
            x if x == sys::QJSJitOp_QJS_OP_drop as u32 => (Op::Drop, 1),
            x if x == sys::QJSJitOp_QJS_OP_nip as u32 => (Op::Nip, 1),
            x if x == sys::QJSJitOp_QJS_OP_dup as u32 => (Op::Dup, 1),
            x if x == sys::QJSJitOp_QJS_OP_dup1 as u32 => (Op::Dup1, 1),
            x if x == sys::QJSJitOp_QJS_OP_swap as u32 => (Op::Swap, 1),

            // Locals
            x if x == sys::QJSJitOp_QJS_OP_get_loc as u32 => {
                (Op::GetLoc(read_u16(buf, pc + 1)?), 3)
            }
            x if x == sys::QJSJitOp_QJS_OP_get_loc8 as u32 => {
                (Op::GetLoc(read_u8(buf, pc + 1)? as u16), 2)
            }
            x if x == sys::QJSJitOp_QJS_OP_get_loc0_loc1 as u32 => (Op::GetLoc0Loc1, 1),
            x if x == sys::QJSJitOp_QJS_OP_get_loc0 as u32 => (Op::GetLoc(0), 1),
            x if x == sys::QJSJitOp_QJS_OP_get_loc1 as u32 => (Op::GetLoc(1), 1),
            x if x == sys::QJSJitOp_QJS_OP_get_loc2 as u32 => (Op::GetLoc(2), 1),
            x if x == sys::QJSJitOp_QJS_OP_get_loc3 as u32 => (Op::GetLoc(3), 1),
            x if x == sys::QJSJitOp_QJS_OP_put_loc as u32 => {
                (Op::PutLoc(read_u16(buf, pc + 1)?), 3)
            }
            x if x == sys::QJSJitOp_QJS_OP_put_loc8 as u32 => {
                (Op::PutLoc(read_u8(buf, pc + 1)? as u16), 2)
            }
            x if x == sys::QJSJitOp_QJS_OP_put_loc0 as u32 => (Op::PutLoc(0), 1),
            x if x == sys::QJSJitOp_QJS_OP_put_loc1 as u32 => (Op::PutLoc(1), 1),
            x if x == sys::QJSJitOp_QJS_OP_put_loc2 as u32 => (Op::PutLoc(2), 1),
            x if x == sys::QJSJitOp_QJS_OP_put_loc3 as u32 => (Op::PutLoc(3), 1),
            x if x == sys::QJSJitOp_QJS_OP_set_loc as u32 => {
                (Op::SetLoc(read_u16(buf, pc + 1)?), 3)
            }
            x if x == sys::QJSJitOp_QJS_OP_set_loc8 as u32 => {
                (Op::SetLoc(read_u8(buf, pc + 1)? as u16), 2)
            }
            x if x == sys::QJSJitOp_QJS_OP_set_loc0 as u32 => (Op::SetLoc(0), 1),
            x if x == sys::QJSJitOp_QJS_OP_set_loc1 as u32 => (Op::SetLoc(1), 1),
            x if x == sys::QJSJitOp_QJS_OP_set_loc2 as u32 => (Op::SetLoc(2), 1),
            x if x == sys::QJSJitOp_QJS_OP_set_loc3 as u32 => (Op::SetLoc(3), 1),
            x if x == sys::QJSJitOp_QJS_OP_set_loc_uninitialized as u32 => {
                (Op::SetLocUninitialized(read_u16(buf, pc + 1)?), 3)
            }
            x if x == sys::QJSJitOp_QJS_OP_get_loc_check as u32 => {
                (Op::GetLocCheck(read_u16(buf, pc + 1)?), 3)
            }
            x if x == sys::QJSJitOp_QJS_OP_put_loc_check as u32 => {
                (Op::PutLocCheck(read_u16(buf, pc + 1)?), 3)
            }
            // Safe no-op because `decode_bytecode` requires `var_ref_count == 0`.
            x if x == sys::QJSJitOp_QJS_OP_close_loc as u32 => (Op::Nop, 3),

            // Arguments
            x if x == sys::QJSJitOp_QJS_OP_get_arg as u32 => {
                (Op::GetArg(read_u16(buf, pc + 1)?), 3)
            }
            x if x == sys::QJSJitOp_QJS_OP_get_arg0 as u32 => (Op::GetArg(0), 1),
            x if x == sys::QJSJitOp_QJS_OP_get_arg1 as u32 => (Op::GetArg(1), 1),
            x if x == sys::QJSJitOp_QJS_OP_get_arg2 as u32 => (Op::GetArg(2), 1),
            x if x == sys::QJSJitOp_QJS_OP_get_arg3 as u32 => (Op::GetArg(3), 1),
            x if x == sys::QJSJitOp_QJS_OP_put_arg as u32 => {
                (Op::PutArg(read_u16(buf, pc + 1)?), 3)
            }
            x if x == sys::QJSJitOp_QJS_OP_put_arg0 as u32 => (Op::PutArg(0), 1),
            x if x == sys::QJSJitOp_QJS_OP_put_arg1 as u32 => (Op::PutArg(1), 1),
            x if x == sys::QJSJitOp_QJS_OP_put_arg2 as u32 => (Op::PutArg(2), 1),
            x if x == sys::QJSJitOp_QJS_OP_put_arg3 as u32 => (Op::PutArg(3), 1),
            x if x == sys::QJSJitOp_QJS_OP_set_arg as u32 => {
                (Op::SetArg(read_u16(buf, pc + 1)?), 3)
            }
            x if x == sys::QJSJitOp_QJS_OP_set_arg0 as u32 => (Op::SetArg(0), 1),
            x if x == sys::QJSJitOp_QJS_OP_set_arg1 as u32 => (Op::SetArg(1), 1),
            x if x == sys::QJSJitOp_QJS_OP_set_arg2 as u32 => (Op::SetArg(2), 1),
            x if x == sys::QJSJitOp_QJS_OP_set_arg3 as u32 => (Op::SetArg(3), 1),

            // Closure vars
            x if x == sys::QJSJitOp_QJS_OP_get_var_ref as u32
                || x == sys::QJSJitOp_QJS_OP_get_var_ref_check as u32 =>
            {
                (Op::GetVarRef(read_u16(buf, pc + 1)?), 3)
            }
            x if x == sys::QJSJitOp_QJS_OP_get_var_ref0 as u32 => (Op::GetVarRef(0), 1),
            x if x == sys::QJSJitOp_QJS_OP_get_var_ref1 as u32 => (Op::GetVarRef(1), 1),
            x if x == sys::QJSJitOp_QJS_OP_get_var_ref2 as u32 => (Op::GetVarRef(2), 1),
            x if x == sys::QJSJitOp_QJS_OP_get_var_ref3 as u32 => (Op::GetVarRef(3), 1),
            x if x == sys::QJSJitOp_QJS_OP_put_var_ref as u32 => {
                (Op::PutVarRef(read_u16(buf, pc + 1)?), 3)
            }
            x if x == sys::QJSJitOp_QJS_OP_put_var_ref0 as u32 => (Op::PutVarRef(0), 1),
            x if x == sys::QJSJitOp_QJS_OP_put_var_ref1 as u32 => (Op::PutVarRef(1), 1),
            x if x == sys::QJSJitOp_QJS_OP_put_var_ref2 as u32 => (Op::PutVarRef(2), 1),
            x if x == sys::QJSJitOp_QJS_OP_put_var_ref3 as u32 => (Op::PutVarRef(3), 1),
            x if x == sys::QJSJitOp_QJS_OP_set_var_ref as u32 => {
                (Op::SetVarRef(read_u16(buf, pc + 1)?), 3)
            }
            x if x == sys::QJSJitOp_QJS_OP_set_var_ref0 as u32 => (Op::SetVarRef(0), 1),
            x if x == sys::QJSJitOp_QJS_OP_set_var_ref1 as u32 => (Op::SetVarRef(1), 1),
            x if x == sys::QJSJitOp_QJS_OP_set_var_ref2 as u32 => (Op::SetVarRef(2), 1),
            x if x == sys::QJSJitOp_QJS_OP_set_var_ref3 as u32 => (Op::SetVarRef(3), 1),

            // Globals
            x if x == sys::QJSJitOp_QJS_OP_get_var as u32 => {
                (Op::GetVar(read_u32(buf, pc + 1)?), 5)
            }
            x if x == sys::QJSJitOp_QJS_OP_get_var_undef as u32 => {
                (Op::GetVarUndef(read_u32(buf, pc + 1)?), 5)
            }
            x if x == sys::QJSJitOp_QJS_OP_put_var as u32 => {
                (Op::PutVar(read_u32(buf, pc + 1)?), 5)
            }

            // Properties & arrays
            x if x == sys::QJSJitOp_QJS_OP_get_field as u32 => {
                (Op::GetField(read_u32(buf, pc + 1)?), 5)
            }
            x if x == sys::QJSJitOp_QJS_OP_get_field2 as u32 => {
                (Op::GetField2(read_u32(buf, pc + 1)?), 5)
            }
            x if x == sys::QJSJitOp_QJS_OP_put_field as u32 => {
                (Op::PutField(read_u32(buf, pc + 1)?), 5)
            }
            x if x == sys::QJSJitOp_QJS_OP_get_array_el as u32 => (Op::GetArrayEl, 1),
            x if x == sys::QJSJitOp_QJS_OP_get_array_el2 as u32 => (Op::GetArrayEl2, 1),
            x if x == sys::QJSJitOp_QJS_OP_put_array_el as u32 => (Op::PutArrayEl, 1),
            x if x == sys::QJSJitOp_QJS_OP_get_length as u32 => (Op::GetLength, 1),

            // In-place local ops
            x if x == sys::QJSJitOp_QJS_OP_inc_loc as u32 => {
                (Op::IncLoc(read_u8(buf, pc + 1)? as u16), 2)
            }
            x if x == sys::QJSJitOp_QJS_OP_dec_loc as u32 => {
                (Op::DecLoc(read_u8(buf, pc + 1)? as u16), 2)
            }
            x if x == sys::QJSJitOp_QJS_OP_add_loc as u32 => {
                (Op::AddLoc(read_u8(buf, pc + 1)? as u16), 2)
            }

            // Unary ops
            x if x == sys::QJSJitOp_QJS_OP_neg as u32 => (Op::Neg, 1),
            x if x == sys::QJSJitOp_QJS_OP_plus as u32 => (Op::Plus, 1),
            x if x == sys::QJSJitOp_QJS_OP_inc as u32 => (Op::Inc, 1),
            x if x == sys::QJSJitOp_QJS_OP_dec as u32 => (Op::Dec, 1),
            x if x == sys::QJSJitOp_QJS_OP_post_inc as u32 => (Op::PostInc, 1),
            x if x == sys::QJSJitOp_QJS_OP_post_dec as u32 => (Op::PostDec, 1),
            x if x == sys::QJSJitOp_QJS_OP_not as u32 => (Op::Not, 1),
            x if x == sys::QJSJitOp_QJS_OP_lnot as u32 => (Op::LNot, 1),
            x if x == sys::QJSJitOp_QJS_OP_is_undefined as u32 => (Op::IsUndefined, 1),
            x if x == sys::QJSJitOp_QJS_OP_is_null as u32 => (Op::IsNull, 1),
            x if x == sys::QJSJitOp_QJS_OP_is_undefined_or_null as u32 => {
                (Op::IsUndefinedOrNull, 1)
            }

            // Binary ops
            x if x == sys::QJSJitOp_QJS_OP_add as u32 => (Op::Add, 1),
            x if x == sys::QJSJitOp_QJS_OP_sub as u32 => (Op::Sub, 1),
            x if x == sys::QJSJitOp_QJS_OP_mul as u32 => (Op::Mul, 1),
            x if x == sys::QJSJitOp_QJS_OP_div as u32 => (Op::Div, 1),
            x if x == sys::QJSJitOp_QJS_OP_mod as u32 => (Op::Mod, 1),
            x if x == sys::QJSJitOp_QJS_OP_pow as u32 => (Op::Pow, 1),
            x if x == sys::QJSJitOp_QJS_OP_shl as u32 => (Op::Shl, 1),
            x if x == sys::QJSJitOp_QJS_OP_sar as u32 => (Op::Sar, 1),
            x if x == sys::QJSJitOp_QJS_OP_shr as u32 => (Op::Shr, 1),
            x if x == sys::QJSJitOp_QJS_OP_and as u32 => (Op::And, 1),
            x if x == sys::QJSJitOp_QJS_OP_or as u32 => (Op::Or, 1),
            x if x == sys::QJSJitOp_QJS_OP_xor as u32 => (Op::Xor, 1),
            x if x == sys::QJSJitOp_QJS_OP_lt as u32 => (Op::Lt, 1),
            x if x == sys::QJSJitOp_QJS_OP_lte as u32 => (Op::Lte, 1),
            x if x == sys::QJSJitOp_QJS_OP_gt as u32 => (Op::Gt, 1),
            x if x == sys::QJSJitOp_QJS_OP_gte as u32 => (Op::Gte, 1),
            x if x == sys::QJSJitOp_QJS_OP_eq as u32 => (Op::Eq, 1),
            x if x == sys::QJSJitOp_QJS_OP_neq as u32 => (Op::Neq, 1),
            x if x == sys::QJSJitOp_QJS_OP_strict_eq as u32 => (Op::StrictEq, 1),
            x if x == sys::QJSJitOp_QJS_OP_strict_neq as u32 => (Op::StrictNeq, 1),

            // Control flow
            x if x == sys::QJSJitOp_QJS_OP_goto as u32 => {
                let target = branch_target(pc, read_i32(buf, pc + 1)? as isize, len)?;
                (Op::Goto(target), 5)
            }
            x if x == sys::QJSJitOp_QJS_OP_goto16 as u32 => {
                let target = branch_target(pc, read_i16(buf, pc + 1)? as isize, len)?;
                (Op::Goto(target), 3)
            }
            x if x == sys::QJSJitOp_QJS_OP_goto8 as u32 => {
                let target = branch_target(pc, read_i8(buf, pc + 1)? as isize, len)?;
                (Op::Goto(target), 2)
            }
            x if x == sys::QJSJitOp_QJS_OP_if_false as u32 => {
                let target = branch_target(pc, read_i32(buf, pc + 1)? as isize, len)?;
                (Op::IfFalse(target), 5)
            }
            x if x == sys::QJSJitOp_QJS_OP_if_false8 as u32 => {
                let target = branch_target(pc, read_i8(buf, pc + 1)? as isize, len)?;
                (Op::IfFalse(target), 2)
            }
            x if x == sys::QJSJitOp_QJS_OP_if_true as u32 => {
                let target = branch_target(pc, read_i32(buf, pc + 1)? as isize, len)?;
                (Op::IfTrue(target), 5)
            }
            x if x == sys::QJSJitOp_QJS_OP_if_true8 as u32 => {
                let target = branch_target(pc, read_i8(buf, pc + 1)? as isize, len)?;
                (Op::IfTrue(target), 2)
            }

            // Calls & returns
            x if x == sys::QJSJitOp_QJS_OP_call as u32 => (Op::Call(read_u16(buf, pc + 1)?), 3),
            x if x == sys::QJSJitOp_QJS_OP_call0 as u32 => (Op::Call(0), 1),
            x if x == sys::QJSJitOp_QJS_OP_call1 as u32 => (Op::Call(1), 1),
            x if x == sys::QJSJitOp_QJS_OP_call2 as u32 => (Op::Call(2), 1),
            x if x == sys::QJSJitOp_QJS_OP_call3 as u32 => (Op::Call(3), 1),
            x if x == sys::QJSJitOp_QJS_OP_call_method as u32 => {
                (Op::CallMethod(read_u16(buf, pc + 1)?), 3)
            }
            x if x == sys::QJSJitOp_QJS_OP_return as u32 => (Op::Return, 1),
            x if x == sys::QJSJitOp_QJS_OP_return_undef as u32 => (Op::ReturnUndef, 1),

            _ => return Err(RejectReason::UnsupportedOpcode(raw)),
        };

        let next_pc = pc + size;
        if next_pc > len {
            return Err(RejectReason::TruncatedBytecode);
        }

        let idx = insns.len();
        pc_to_idx.insert(pc, idx);

        match op {
            Op::Goto(t) => {
                block_starts.insert(t);
                if next_pc < len {
                    block_starts.insert(next_pc);
                }
            }
            Op::IfFalse(t) | Op::IfTrue(t) => {
                block_starts.insert(t);
                if next_pc < len {
                    block_starts.insert(next_pc);
                }
            }
            Op::Return | Op::ReturnUndef if next_pc < len => {
                block_starts.insert(next_pc);
            }
            _ => {}
        }

        insns.push(DecodedInsn {
            pc,
            next_pc,
            stack_before: 0,
            op,
        });
        pc = next_pc;
    }

    // Verify all branch targets land on valid instruction boundaries and compute
    // deterministic stack depths via BFS from PC 0.
    let mut depths: Vec<Option<u16>> = vec![None; insns.len()];
    let mut queue = VecDeque::new();
    depths[0] = Some(0);
    queue.push_back(0usize);

    while let Some(idx) = queue.pop_front() {
        let depth = depths[idx].unwrap();
        let insn = insns[idx];
        let (pop, push) = insn.op.stack_effect();
        if depth < pop {
            return Err(RejectReason::StackMismatch);
        }
        let after = depth - pop + push;
        if after > info.stack_size {
            return Err(RejectReason::StackOverflow);
        }

        let mut visit = |target_pc: usize| -> Result<(), RejectReason> {
            let &target_idx = pc_to_idx
                .get(&target_pc)
                .ok_or(RejectReason::InvalidBranchTarget)?;
            match depths[target_idx] {
                None => {
                    depths[target_idx] = Some(after);
                    queue.push_back(target_idx);
                }
                Some(prev) if prev != after => return Err(RejectReason::StackMismatch),
                Some(_) => {}
            }
            Ok(())
        };

        match insn.op {
            Op::Goto(t) => visit(t)?,
            Op::IfFalse(t) | Op::IfTrue(t) => {
                visit(t)?;
                if insn.next_pc < len {
                    visit(insn.next_pc)?;
                }
            }
            Op::Return | Op::ReturnUndef => {}
            _ => {
                if insn.next_pc < len {
                    visit(insn.next_pc)?;
                }
            }
        }
    }

    // Record computed stack depths; keep only reachable instructions or mark them.
    for (i, insn) in insns.iter_mut().enumerate() {
        if let Some(d) = depths[i] {
            insn.stack_before = d;
        } else {
            // Unreachable dead bytecode: replace with Nop.
            insn.op = Op::Nop;
        }
    }

    Ok(DecodedFunction {
        insns,
        block_starts,
        arg_count: info.arg_count,
        var_count: info.var_count,
        stack_size: info.stack_size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_info(buf: &[u8]) -> sys::QJSJitBytecodeInfo {
        sys::QJSJitBytecodeInfo {
            buf: buf.as_ptr(),
            len: buf.len() as u32,
            arg_count: 1,
            var_count: 1,
            stack_size: 4,
            var_ref_count: 0,
            closure_var_count: 0,
            is_strict_mode: 1,
            func_kind: 0,
            cpool_count: 0,
            cpool: std::ptr::null(),
            realm: std::ptr::null_mut(),
        }
    }

    #[test]
    fn rejects_non_normal_and_captured_and_empty_and_oversized() {
        let code = [sys::QJSJitOp_QJS_OP_return_undef as u8];
        let mut info = make_info(&code);
        info.func_kind = 1;
        assert_eq!(
            decode_bytecode(&info, 1024).unwrap_err(),
            RejectReason::NotNormalFunction
        );

        let mut info = make_info(&code);
        info.var_ref_count = 1;
        assert_eq!(
            decode_bytecode(&info, 1024).unwrap_err(),
            RejectReason::HasCapturedLocals
        );

        let mut info = make_info(&[]);
        info.buf = std::ptr::null();
        assert_eq!(
            decode_bytecode(&info, 1024).unwrap_err(),
            RejectReason::EmptyBytecode
        );

        let info = make_info(&code);
        assert_eq!(
            decode_bytecode(&info, 0).unwrap_err(),
            RejectReason::BytecodeTooLarge
        );
    }

    #[test]
    fn rejects_truncated_invalid_branch_stack_mismatch_and_unsupported() {
        // push_i32 needs 4 immediate bytes, only 2 provided
        let trunc = [sys::QJSJitOp_QJS_OP_push_i32 as u8, 1, 2];
        assert_eq!(
            decode_bytecode(&make_info(&trunc), 1024).unwrap_err(),
            RejectReason::TruncatedBytecode
        );

        // goto8 jumping out of bounds (+50)
        let bad_branch = [sys::QJSJitOp_QJS_OP_goto8 as u8, 50];
        assert_eq!(
            decode_bytecode(&make_info(&bad_branch), 1024).unwrap_err(),
            RejectReason::InvalidBranchTarget
        );

        // drop on empty stack -> StackMismatch
        let underflow = [
            sys::QJSJitOp_QJS_OP_drop as u8,
            sys::QJSJitOp_QJS_OP_return_undef as u8,
        ];
        assert_eq!(
            decode_bytecode(&make_info(&underflow), 1024).unwrap_err(),
            RejectReason::StackMismatch
        );

        // Exceeds info.stack_size (4)
        let overflow = [
            sys::QJSJitOp_QJS_OP_push_0 as u8,
            sys::QJSJitOp_QJS_OP_push_0 as u8,
            sys::QJSJitOp_QJS_OP_push_0 as u8,
            sys::QJSJitOp_QJS_OP_push_0 as u8,
            sys::QJSJitOp_QJS_OP_push_0 as u8,
            sys::QJSJitOp_QJS_OP_return_undef as u8,
        ];
        assert_eq!(
            decode_bytecode(&make_info(&overflow), 1024).unwrap_err(),
            RejectReason::StackOverflow
        );

        // Unsupported opcode (OP_catch)
        let unsupported = [sys::QJSJitOp_QJS_OP_catch as u8, 0, 0, 0, 0];
        assert_eq!(
            decode_bytecode(&make_info(&unsupported), 1024).unwrap_err(),
            RejectReason::UnsupportedOpcode(sys::QJSJitOp_QJS_OP_catch as u8)
        );
    }
}
