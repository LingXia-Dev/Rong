#![doc = "Raw FFI bindings to JavaScriptCore"]
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

/// Watchdog callback for [`JSContextGroupSetExecutionTimeLimit`]: return
/// `true` to terminate the running script, `false` to grant another period.
pub type JSShouldTerminateCallback = ::std::option::Option<
    unsafe extern "C" fn(ctx: JSContextRef, context: *mut ::std::os::raw::c_void) -> bool,
>;

// Execution time limit SPI from JSContextRefPrivate.h. The header is private
// (not shipped in the system framework's public Headers/), but the symbols are
// exported from libJavaScriptCore on both the system and source backends, so
// they are declared by hand instead of via bindgen.
unsafe extern "C" {
    pub fn JSContextGroupSetExecutionTimeLimit(
        group: JSContextGroupRef,
        limit: f64,
        callback: JSShouldTerminateCallback,
        context: *mut ::std::os::raw::c_void,
    );
    pub fn JSContextGroupClearExecutionTimeLimit(group: JSContextGroupRef);
}

// JavaScriptCore's full one-time global initialization (WTF, option parsing,
// Gigacage, the executable allocator, signal handlers, Structure tables). The
// system framework runs this from the dylib's static initializers before
// `main`; a statically linked JSCOnly artifact does not, so the C API would
// otherwise initialize a *subset* lazily and racily on first use across
// threads, corrupting Structure metadata. `JSC::initialize()` guards itself
// with `std::call_once`, and the `Once` here makes the very first call complete
// before any thread creates a VM.
#[cfg(jsc_source)]
mod jsc_init {
    use std::sync::Once;

    unsafe extern "C" {
        fn rong_jsc_initialize();
    }

    static INIT: Once = Once::new();

    /// Run JavaScriptCore's one-time global initialization. Idempotent and
    /// thread-safe; call before creating the first VM/context.
    pub fn ensure_initialized() {
        INIT.call_once(|| unsafe { rong_jsc_initialize() });
    }
}

#[cfg(jsc_source)]
pub use jsc_init::ensure_initialized;

/// FFI declarations for the C++ bytecode bridge (`bytecode_bridge.cpp`).
///
/// The bridge is compiled by `build.rs` via the `cc` crate and linked into
/// the final binary. It exposes these `extern "C"` entry points that wrap
/// JSC's internal C++ bytecode serialization APIs.
///
/// This module is only available for the source/JSCOnly backend
/// (`#[cfg(jsc_source)]`). The bridge is always compiled and linked for that
/// backend by `build.rs` with the real implementation. Source artifacts are
/// validated at build time and must include the private headers required by
/// the bridge, so these `extern "C"` symbols are always defined and never cause
/// a link error.
#[cfg(jsc_source)]
pub mod bytecode_bridge {
    use std::ffi::{c_char, c_int};

    /// Result of a compile-to-bytecode operation.
    ///
    /// On success: `data` is non-null, `size` > 0, `error` is null.
    /// The caller owns `data` and must free it with `rong_jsc_free_bytecode`.
    ///
    /// On failure: `data` may be null, `error` points to a static C string
    /// describing the error.
    #[repr(C)]
    pub struct RongJSCBytecodeResult {
        pub data: *mut u8,
        pub size: usize,
        pub error: *const c_char,
    }

    /// Result of a run-bytecode operation.
    ///
    /// `value` is a JavaScript value in `ctx`. If `is_exception` is non-zero,
    /// the caller must treat it as a thrown exception. `error` is used only for
    /// bridge/internal failures that could not be represented as a JS value.
    #[repr(C)]
    pub struct RongJSCRunBytecodeResult {
        pub value: crate::JSValueRef,
        pub is_exception: c_int,
        pub error: *const c_char,
    }

    unsafe extern "C" {
        /// Whether the real bytecode bridge is linked. This is expected to be
        /// non-zero for source artifacts built through the normal build path.
        pub fn rong_jsc_bytecode_supported() -> c_int;

        /// Compile JavaScript source code to JSC bytecode.
        ///
        /// The returned buffer is a Rong-owned envelope containing the source
        /// bytes needed for JSC cache-key validation plus the serialized JSC
        /// bytecode payload.
        ///
        /// # Safety
        /// - `ctx` must be a valid `JSGlobalContextRef`.
        /// - `source` must point to `source_len` bytes of valid UTF-8.
        /// - `source_url` must be a null-terminated C string.
        pub fn rong_jsc_compile_to_bytecode(
            ctx: *mut crate::OpaqueJSContext,
            source: *const c_char,
            source_len: usize,
            source_url: *const c_char,
        ) -> RongJSCBytecodeResult;

        /// Free a bytecode buffer returned by `rong_jsc_compile_to_bytecode`.
        ///
        /// # Safety
        /// `data` must be a pointer previously returned by
        /// `rong_jsc_compile_to_bytecode` (in the `data` field of a
        /// successful result). Must NOT be called more than once for the
        /// same pointer.
        pub fn rong_jsc_free_bytecode(data: *mut u8);

        /// Free an error string returned by the bytecode bridge.
        ///
        /// # Safety
        /// `error` must be a pointer returned by one of this module's bridge
        /// functions. Passing null is allowed.
        pub fn rong_jsc_free_error(error: *const c_char);

        /// Execute previously compiled JSC bytecode.
        ///
        /// The bytecode must have been produced by
        /// `rong_jsc_compile_to_bytecode`.
        ///
        /// Returns the JS result value on success. On error (version
        /// mismatch, corrupt bytecode, runtime exception), returns a
        /// JS exception value. The caller can check with
        /// `JSValueIsException`.
        ///
        /// # Safety
        /// - `ctx` must be a valid `JSGlobalContextRef`.
        /// - `bytes` must point to `size` bytes of valid bytecode.
        pub fn rong_jsc_run_bytecode(
            ctx: *mut crate::OpaqueJSContext,
            bytes: *const u8,
            size: usize,
        ) -> RongJSCRunBytecodeResult;
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use std::ffi::CString;
    use std::ptr;

    #[test]
    fn test_jscore_raw_binding() {
        unsafe {
            let global_context = JSGlobalContextCreate(ptr::null_mut());

            let js_code = CString::new("Math.sqrt(16)").expect("CString::new failed");

            let js_string = JSStringCreateWithUTF8CString(js_code.as_ptr());

            let mut exception: JSValueRef = ptr::null_mut();

            let result = JSEvaluateScript(
                global_context,
                js_string,
                ptr::null_mut(), // thisObject, use null for global
                ptr::null_mut(), // sourceURL
                1,               // startingLineNumber
                &mut exception,
            );

            if !exception.is_null() {
                let exception_string =
                    JSValueToStringCopy(global_context, exception, ptr::null_mut());
                let exception_cstring = JSStringGetCharactersPtr(exception_string);
                println!("JavaScript exception occurred: {:?}", exception_cstring);
                JSStringRelease(exception_string);
            } else {
                let result_number = JSValueToNumber(global_context, result, ptr::null_mut());
                assert_eq!(result_number, 4.0);
            }

            JSStringRelease(js_string);
            JSGlobalContextRelease(global_context);
        }
    }
}
