use crate::{JSCRuntime, JSCValue, jsc};
use rong_core::{
    JSClass, JSContextImpl, JSErrorFactory, JSExceptionThrower, JSRuntimeImpl, JSValueImpl,
    RongJSError,
};
use smallvec::SmallVec;
use std::ffi::CString;
use std::ptr;

/// Bytecode is unavailable through JavaScriptCore's public C API, so every
/// bytecode entry point reports the same not-supported message.
const BYTECODE_UNSUPPORTED_MSG: &str = "Bytecode is not supported on JavaScriptCore";

pub struct JSCContext {
    raw: *mut jsc::OpaqueJSContext,
}

impl JSContextImpl for JSCContext {
    type RawContext = *mut jsc::OpaqueJSContext;
    type Runtime = JSCRuntime;
    type Value = JSCValue;

    fn new(runtime: &Self::Runtime) -> Self {
        // Each JSCRuntime owns an independent context group (one JSC VM), so the
        // "one thread, one independent runtime" model holds: a VM is only ever
        // touched by its owning thread, and N runtimes run on N threads
        // concurrently. The source backend additionally needs JSC's one-time
        // global init forced before the first VM is created (the system
        // framework gets this from the dylib's static initializers).
        #[cfg(jsc_source)]
        jsc::ensure_initialized();
        let ctx = Self {
            raw: unsafe { jsc::JSGlobalContextCreateInGroup(runtime.to_raw(), ptr::null_mut()) },
        };
        let _ = crate::value::proxy::prime_proxy_helper(&ctx);
        ctx
    }

    fn as_raw(&self) -> &Self::RawContext {
        &self.raw
    }

    fn context_id(ctx: &Self::RawContext) -> usize {
        *ctx as *const _ as usize
    }

    fn from_borrowed_raw(ctx: Self::RawContext) -> Self {
        Self::_from_borrowed_raw(ctx)
    }

    fn eval(&self, source: rong_core::Source) -> Self::Value {
        let filename = source.name().unwrap_or("eval");
        // Keep behavior consistent with the QuickJS backend, which evaluates in
        // strict mode (it sets JS_EVAL_FLAG_STRICT by default). JavaScriptCore's
        // JSEvaluateScript has no strict-mode flag, so we enforce it by
        // prepending a "use strict" directive prologue.
        let mut code_bytes = b"\"use strict\";\n".to_vec();
        code_bytes.extend_from_slice(source.code());
        let code: Vec<u16> = String::from_utf8_lossy(&code_bytes)
            .encode_utf16()
            .collect();
        let filename: Vec<u16> = filename.encode_utf16().collect();

        unsafe {
            let js_str = jsc::JSStringCreateWithCharacters(code.as_ptr(), code.len());
            let js_filename = jsc::JSStringCreateWithCharacters(filename.as_ptr(), filename.len());

            let mut exception: jsc::JSValueRef = ptr::null_mut();
            let result = jsc::JSEvaluateScript(
                self.raw,
                js_str,
                jsc::JSContextGetGlobalObject(self.raw),
                js_filename,
                1,
                &mut exception,
            );

            jsc::JSStringRelease(js_str);
            jsc::JSStringRelease(js_filename);

            if !exception.is_null() {
                return JSCValue::from_owned_raw(self.raw, exception).with_exception();
            }
            JSCValue::from_owned_raw(self.raw, result)
        }
    }

    fn global(&self) -> Self::Value {
        unsafe {
            let global_obj = jsc::JSContextGetGlobalObject(self.raw);
            JSCValue::from_owned_obj(self.raw, global_obj)
        }
    }

    /// Registers a JavaScript class in the context.
    fn register_class<JC>(&self) -> Self::Value
    where
        JC: JSClass<Self::Value>,
    {
        crate::class::register_class_internal::<JC>(self, JC::NAME)
    }

    /// Calls a JavaScript function with the specified `this` value and arguments.
    fn call(&self, function: &Self::Value, this: Self::Value, argv: &[Self::Value]) -> Self::Value {
        unsafe {
            let mut exception: jsc::JSValueRef = std::ptr::null_mut();

            let function_obj = function.as_obj();
            let this_value: jsc::JSValueRef = (*this.as_raw_value()).cast();

            // Convert argv to raw JSValues
            let args: SmallVec<[jsc::JSValueRef; 4]> = argv
                .iter()
                .map(|v| {
                    let raw = *v.as_raw_value();
                    raw.cast()
                })
                .collect();

            // Fast path: object `this` can be passed directly.
            // For non-object `this` (undefined/null/primitive), we must use
            // Function.prototype.call so JavaScriptCore applies correct this-binding rules.
            let result = if jsc::JSValueIsObject(self.raw, this_value) {
                let this_obj = jsc::JSValueToObject(self.raw, this_value, &mut exception);
                if !exception.is_null() {
                    return JSCValue::from_owned_raw(self.raw, exception).with_exception();
                }
                jsc::JSObjectCallAsFunction(
                    self.raw,
                    function_obj,
                    this_obj,
                    args.len(),
                    args.as_ptr(),
                    &mut exception,
                )
            } else {
                // Use Function.prototype.call instead of function.call to avoid user overrides.
                let global_obj = jsc::JSContextGetGlobalObject(self.raw);

                let function_key = jsc::JSStringCreateWithUTF8CString(c"Function".as_ptr());
                let function_value =
                    jsc::JSObjectGetProperty(self.raw, global_obj, function_key, &mut exception);
                jsc::JSStringRelease(function_key);
                if !exception.is_null() {
                    return JSCValue::from_owned_raw(self.raw, exception).with_exception();
                }

                let function_ctor = jsc::JSValueToObject(self.raw, function_value, &mut exception);
                if !exception.is_null() {
                    return JSCValue::from_owned_raw(self.raw, exception).with_exception();
                }

                let prototype_key = jsc::JSStringCreateWithUTF8CString(c"prototype".as_ptr());
                let prototype_value = jsc::JSObjectGetProperty(
                    self.raw,
                    function_ctor,
                    prototype_key,
                    &mut exception,
                );
                jsc::JSStringRelease(prototype_key);
                if !exception.is_null() {
                    return JSCValue::from_owned_raw(self.raw, exception).with_exception();
                }

                let prototype_obj = jsc::JSValueToObject(self.raw, prototype_value, &mut exception);
                if !exception.is_null() {
                    return JSCValue::from_owned_raw(self.raw, exception).with_exception();
                }

                let call_key = jsc::JSStringCreateWithUTF8CString(c"call".as_ptr());
                let call_value =
                    jsc::JSObjectGetProperty(self.raw, prototype_obj, call_key, &mut exception);
                jsc::JSStringRelease(call_key);
                if !exception.is_null() {
                    return JSCValue::from_owned_raw(self.raw, exception).with_exception();
                }

                let call_obj = jsc::JSValueToObject(self.raw, call_value, &mut exception);
                if !exception.is_null() {
                    return JSCValue::from_owned_raw(self.raw, exception).with_exception();
                }

                let mut call_args: SmallVec<[jsc::JSValueRef; 5]> =
                    SmallVec::with_capacity(args.len() + 1);
                call_args.push(this_value);
                call_args.extend(args.iter().copied());

                jsc::JSObjectCallAsFunction(
                    self.raw,
                    call_obj,
                    function_obj,
                    call_args.len(),
                    call_args.as_ptr(),
                    &mut exception,
                )
            };

            if !exception.is_null() {
                return JSCValue::from_owned_raw(self.raw, exception).with_exception();
            }

            JSCValue::from_owned_raw(self.raw, result)
        }
    }

    fn promise(&self) -> (Self::Value, Self::Value, Self::Value) {
        unsafe {
            let mut resolve_fn: jsc::JSObjectRef = std::ptr::null_mut();
            let mut reject_fn: jsc::JSObjectRef = std::ptr::null_mut();
            let mut exception: jsc::JSValueRef = std::ptr::null_mut();

            let promise = jsc::JSObjectMakeDeferredPromise(
                self.raw,
                &mut resolve_fn,
                &mut reject_fn,
                &mut exception,
            );

            if !exception.is_null() {
                let undefined = jsc::JSValueMakeUndefined(self.raw);
                return (
                    JSCValue::from_owned_raw(self.raw, undefined),
                    JSCValue::from_owned_raw(self.raw, undefined),
                    JSCValue::from_owned_raw(self.raw, undefined),
                );
            }

            (
                JSCValue::from_borrowed_obj(self.raw, promise),
                JSCValue::from_borrowed_obj(self.raw, resolve_fn),
                JSCValue::from_borrowed_obj(self.raw, reject_fn),
            )
        }
    }

    #[cfg(jsc_source)]
    fn compile_to_bytecode(&self, source: rong_core::Source) -> Result<Vec<u8>, RongJSError> {
        use rong_core::HostError;
        use std::ffi::CString;

        // Build-time validation requires source artifacts to provide the real
        // bytecode bridge. Keep this guard as a defensive fallback for manually
        // linked binaries.
        if unsafe { jsc::bytecode_bridge::rong_jsc_bytecode_supported() } == 0 {
            return Err(HostError::new(
                rong_core::error::E_NOT_SUPPORTED,
                BYTECODE_UNSUPPORTED_MSG,
            )
            .with_data(rong_core::err_data!({ feature: "bytecode" }))
            .into());
        }

        let code = source.code();
        let filename = source.name().unwrap_or("eval");

        // Match the `eval` method: prepend a "use strict" directive prologue
        // so the compiled bytecode matches what a normal eval would produce.
        let mut code_with_strict = b"\"use strict\";\n".to_vec();
        code_with_strict.extend_from_slice(code);

        let filename_cstr = CString::new(filename).map_err(|_| {
            HostError::new(
                rong_core::error::E_INVALID_ARG,
                "Filename contains null byte",
            )
        })?;

        unsafe {
            let result = jsc::bytecode_bridge::rong_jsc_compile_to_bytecode(
                self.raw,
                code_with_strict.as_ptr().cast(),
                code_with_strict.len(),
                filename_cstr.as_ptr(),
            );

            // Handle explicit error message from the bridge.
            if !result.error.is_null() {
                let msg = std::ffi::CStr::from_ptr(result.error)
                    .to_string_lossy()
                    .into_owned();
                if !result.data.is_null() {
                    jsc::bytecode_bridge::rong_jsc_free_bytecode(result.data);
                }
                jsc::bytecode_bridge::rong_jsc_free_error(result.error);
                return Err(HostError::new(rong_core::error::E_COMPILE, msg).into());
            }

            // The bridge should never return NULL data without an error message,
            // but guard against it anyway.
            if result.data.is_null() || result.size == 0 {
                return Err(HostError::new(
                    rong_core::error::E_COMPILE,
                    "Bytecode compilation produced no output",
                )
                .into());
            }

            let bytecode = std::slice::from_raw_parts(result.data, result.size).to_vec();
            jsc::bytecode_bridge::rong_jsc_free_bytecode(result.data);
            Ok(bytecode)
        }
    }

    #[cfg(jsc_source)]
    fn run_bytecode(&self, bytes: &[u8]) -> Self::Value {
        // Defensive fallback for manually linked binaries without the bridge.
        if unsafe { jsc::bytecode_bridge::rong_jsc_bytecode_supported() } == 0 {
            return self
                .new_error(
                    "Error",
                    BYTECODE_UNSUPPORTED_MSG,
                    Some(rong_core::error::E_NOT_SUPPORTED),
                )
                .with_exception();
        }
        unsafe {
            let result =
                jsc::bytecode_bridge::rong_jsc_run_bytecode(self.raw, bytes.as_ptr(), bytes.len());

            if !result.error.is_null() {
                let msg = std::ffi::CStr::from_ptr(result.error)
                    .to_string_lossy()
                    .into_owned();
                jsc::bytecode_bridge::rong_jsc_free_error(result.error);
                return self
                    .new_error("Error", msg, Some(rong_core::error::E_COMPILE))
                    .with_exception();
            }

            if result.value.is_null() {
                return self
                    .new_error(
                        "Error",
                        "Bytecode execution failed (null result)",
                        Some(rong_core::error::E_COMPILE),
                    )
                    .with_exception();
            }

            let value = JSCValue::from_owned_raw(self.raw, result.value);
            if result.is_exception != 0 {
                value.with_exception()
            } else {
                value
            }
        }
    }

    #[cfg(not(jsc_source))]
    fn compile_to_bytecode(&self, _source: rong_core::Source) -> Result<Vec<u8>, RongJSError> {
        Err(
            rong_core::HostError::new(rong_core::error::E_NOT_SUPPORTED, BYTECODE_UNSUPPORTED_MSG)
                .with_data(rong_core::err_data!({ feature: "bytecode" }))
                .into(),
        )
    }

    #[cfg(not(jsc_source))]
    fn run_bytecode(&self, _bytes: &[u8]) -> Self::Value {
        // JavaScriptCore's public C API exposes no bytecode (de)serialization, so
        // bytecode is unsupported on this backend — mirror `compile_to_bytecode`
        // by returning a thrown not-supported error instead of panicking.
        self.new_error(
            "Error",
            BYTECODE_UNSUPPORTED_MSG,
            Some(rong_core::error::E_NOT_SUPPORTED),
        )
        .with_exception()
    }
}

impl JSCContext {
    pub(crate) fn eval_direct(&self, source: &[u8]) -> JSCValue {
        let code = match CString::new(source) {
            Ok(code) => code,
            Err(_) => {
                return self
                    .new_error(
                        "SyntaxError",
                        "Source contains an interior null byte",
                        Some(rong_core::error::E_COMPILE),
                    )
                    .with_exception();
            }
        };
        unsafe {
            let js_str = jsc::JSStringCreateWithUTF8CString(code.as_ptr());
            let mut exception: jsc::JSValueRef = ptr::null_mut();
            let result = jsc::JSEvaluateScript(
                self.raw,
                js_str,
                ptr::null_mut(),
                ptr::null_mut(),
                1,
                &mut exception,
            );
            jsc::JSStringRelease(js_str);

            if !exception.is_null() {
                JSCValue::from_owned_raw(self.raw, exception).with_exception()
            } else {
                JSCValue::from_owned_raw(self.raw, result)
            }
        }
    }

    fn _from_borrowed_raw(ctx: *mut jsc::OpaqueJSContext) -> Self {
        let raw = unsafe { jsc::JSGlobalContextRetain(ctx) };
        Self { raw }
    }

    pub(crate) fn to_raw(&self) -> *mut jsc::OpaqueJSContext {
        self.raw
    }
}

impl Drop for JSCContext {
    fn drop(&mut self) {
        unsafe {
            jsc::JSGlobalContextRelease(self.raw);
        }
    }
}

impl Clone for JSCContext {
    fn clone(&self) -> Self {
        unsafe {
            // Retains a global JavaScript execution context.
            jsc::JSGlobalContextRetain(self.raw);
            Self { raw: self.raw }
        }
    }
}

impl JSErrorFactory for JSCContext {
    fn new_error(&self, name: &str, message: impl AsRef<str>, code: Option<&str>) -> Self::Value {
        self.new_error_with_name_internal(name, message.as_ref(), code)
    }
}

impl JSExceptionThrower for JSCContext {
    fn throw(&self, value: Self::Value) -> Self::Value {
        value.with_exception()
    }
}

impl JSCContext {
    pub(crate) fn new_error_with_name_internal(
        &self,
        error_name: &str,
        message: &str,
        code: Option<&str>,
    ) -> JSCValue {
        let message_cstr = CString::new(message).unwrap();
        let error_name_cstr = CString::new(error_name).unwrap();

        unsafe {
            let message_str = jsc::JSStringCreateWithUTF8CString(message_cstr.as_ptr());
            let error_name_str = jsc::JSStringCreateWithUTF8CString(error_name_cstr.as_ptr());

            let mut exception: jsc::JSValueRef = ptr::null_mut();
            let global = jsc::JSContextGetGlobalObject(self.raw);
            let ctor_value =
                jsc::JSObjectGetProperty(self.raw, global, error_name_str, &mut exception);

            let mut error: jsc::JSObjectRef = ptr::null_mut();
            if exception.is_null()
                && !ctor_value.is_null()
                && jsc::JSValueIsObject(self.raw, ctor_value)
            {
                let ctor = jsc::JSValueToObject(self.raw, ctor_value, &mut exception);
                if exception.is_null() && !ctor.is_null() {
                    let message_value = jsc::JSValueMakeString(self.raw, message_str);
                    let args = [message_value];
                    let obj = jsc::JSObjectCallAsConstructor(
                        self.raw,
                        ctor,
                        1,
                        args.as_ptr(),
                        &mut exception,
                    );
                    if exception.is_null() && !obj.is_null() {
                        let proto_key = jsc::JSStringCreateWithUTF8CString(c"prototype".as_ptr());
                        let proto_value =
                            jsc::JSObjectGetProperty(self.raw, ctor, proto_key, &mut exception);
                        if exception.is_null() && !proto_value.is_null() {
                            jsc::JSObjectSetPrototype(self.raw, obj, proto_value);
                        }
                        jsc::JSStringRelease(proto_key);

                        let name_key = jsc::JSStringCreateWithUTF8CString(c"name".as_ptr());
                        let name_value = jsc::JSValueMakeString(self.raw, error_name_str);
                        jsc::JSObjectSetProperty(
                            self.raw,
                            obj,
                            name_key,
                            name_value,
                            jsc::attr(jsc::kJSPropertyAttributeDontEnum),
                            ptr::null_mut(),
                        );
                        jsc::JSStringRelease(name_key);

                        error = obj;
                    }
                }
            }

            if error.is_null() {
                let message_value = jsc::JSValueMakeString(self.raw, message_str);
                let args = [message_value];
                let obj = jsc::JSObjectMakeError(self.raw, 1, args.as_ptr(), ptr::null_mut());

                let name_key = jsc::JSStringCreateWithUTF8CString(c"name".as_ptr());
                let name_value = jsc::JSValueMakeString(self.raw, error_name_str);
                jsc::JSObjectSetProperty(
                    self.raw,
                    obj,
                    name_key,
                    name_value,
                    jsc::attr(jsc::kJSPropertyAttributeDontEnum),
                    ptr::null_mut(),
                );
                jsc::JSStringRelease(name_key);

                error = obj;
            }

            if let Some(code) = code {
                let code_cstr = CString::new(code).unwrap();
                let code_key = jsc::JSStringCreateWithUTF8CString(c"code".as_ptr());
                let code_value_str = jsc::JSStringCreateWithUTF8CString(code_cstr.as_ptr());
                let code_value = jsc::JSValueMakeString(self.raw, code_value_str);
                jsc::JSObjectSetProperty(
                    self.raw,
                    error,
                    code_key,
                    code_value,
                    jsc::attr(jsc::kJSPropertyAttributeDontEnum),
                    ptr::null_mut(),
                );
                jsc::JSStringRelease(code_key);
                jsc::JSStringRelease(code_value_str);
            }

            jsc::JSStringRelease(message_str);
            jsc::JSStringRelease(error_name_str);

            JSCValue::from_owned_obj(self.raw, error).with_error()
        }
    }
}
