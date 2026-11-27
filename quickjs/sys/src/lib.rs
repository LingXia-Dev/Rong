#![doc = "Raw FFI bindings to QuickJS-NG"]
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/quickjs.bindings.rs"));

#[cfg(test)]
mod tests {

    use super::*;
    use std::ffi::CString;

    #[test]
    fn run_script() {
        let js = "63";
        let mut number: i64 = 0;

        unsafe {
            let rt = JS_NewRuntime();
            let ctx = JS_NewContext(rt);
            let c_string = CString::new(js).unwrap();
            let jsvalue = JS_Eval(
                ctx,
                c_string.as_ptr(),
                js.len() as _,
                c"eval".as_ptr(),
                0 as _,
            );
            JS_ToInt64(ctx, &mut number, jsvalue);
            JS_FreeValue(ctx, jsvalue);
            JS_FreeContext(ctx);
            JS_FreeRuntime(rt);
        }
        assert!(number == 63);
    }

    #[cfg(feature = "jit")]
    #[test]
    fn jit_hooks_intercept_and_cleanup() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct HookState {
            calls: AtomicUsize,
            freed_bytecodes: AtomicUsize,
            freed_runtime: AtomicUsize,
        }

        unsafe extern "C" fn on_call(
            _caller_ctx: *mut JSContext,
            opaque: *mut std::ffi::c_void,
            bc: *mut std::ffi::c_void,
            _func_obj: *const JSValue,
            _this_obj: *const JSValue,
            _argc: std::ffi::c_int,
            _argv: *const JSValue,
            _flags: std::ffi::c_int,
            _ret_out: *mut JSValue,
        ) -> std::ffi::c_int {
            let state = unsafe { &*(opaque as *const HookState) };
            state.calls.fetch_add(1, Ordering::Relaxed);
            let count = unsafe { QJS_JitIncCallCount(bc) };
            if count == 1 {
                unsafe { QJS_JitSetBytecodeData(bc, 0x1234usize as *mut std::ffi::c_void) };
            }
            // Return 0 to fall back to the interpreter.
            0
        }

        unsafe extern "C" fn on_free_bc(
            opaque: *mut std::ffi::c_void,
            _bc: *mut std::ffi::c_void,
            data: *mut std::ffi::c_void,
        ) {
            assert_eq!(data as usize, 0x1234);
            let state = unsafe { &*(opaque as *const HookState) };
            state.freed_bytecodes.fetch_add(1, Ordering::Relaxed);
        }

        unsafe extern "C" fn on_free_rt(opaque: *mut std::ffi::c_void) {
            let state = unsafe { &*(opaque as *const HookState) };
            state.freed_runtime.fetch_add(1, Ordering::Relaxed);
        }

        let state = Box::new(HookState {
            calls: AtomicUsize::new(0),
            freed_bytecodes: AtomicUsize::new(0),
            freed_runtime: AtomicUsize::new(0),
        });
        let state_ptr = &*state as *const HookState as *mut std::ffi::c_void;

        unsafe {
            let rt = JS_NewRuntime();
            QJS_SetJitHooks(
                rt,
                state_ptr,
                Some(on_call),
                Some(on_free_bc),
                Some(on_free_rt),
            );
            assert_eq!(QJS_GetJitOpaque(rt), state_ptr);

            let ctx = JS_NewContext(rt);
            let js = "function add(a, b) { return a + b; } add(20, 22);";
            let c_string = CString::new(js).unwrap();
            let val = JS_Eval(ctx, c_string.as_ptr(), js.len(), c"eval".as_ptr(), 0);
            let mut res: i32 = 0;
            JS_ToInt32(ctx, &mut res, val);
            assert_eq!(res, 42);
            JS_FreeValue(ctx, val);
            JS_FreeContext(ctx);
            JS_FreeRuntime(rt);
        }

        assert!(state.calls.load(Ordering::Relaxed) >= 1);
        assert!(state.freed_bytecodes.load(Ordering::Relaxed) >= 1);
        assert_eq!(state.freed_runtime.load(Ordering::Relaxed), 1);
    }
}
