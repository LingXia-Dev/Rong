use crate::runtime::ArkJSRuntimeInner;
use crate::{ArkJSRuntime, ArkJSValue, arkjs};
use rong_core::{
    HostError, JSClass, JSContextImpl, JSErrorFactory, JSExceptionThrower, JSRuntimeImpl,
    JSValueImpl, PromiseHandlerRegistration, RongJSError,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::ptr;
use std::rc::{Rc, Weak};

fn compile_to_bytecode_failed() -> RongJSError {
    HostError::new(
        rong_core::error::E_COMPILE,
        "Failed to compile JS code to bytecode",
    )
    .into()
}

thread_local! {
    static CONTEXT_REGISTRY: RefCell<HashMap<usize, Weak<ArkJSContextInner>>> =
        RefCell::new(HashMap::new());
}

pub(crate) struct ArkJSContextInner {
    raw: arkjs::JSVM_Env,
    env_scope: arkjs::JSVM_EnvScope,
    handle_scope: arkjs::JSVM_HandleScope,
    runtime: Rc<ArkJSRuntimeInner>,
}

impl Drop for ArkJSContextInner {
    fn drop(&mut self) {
        if self.raw.is_null() {
            return;
        }

        CONTEXT_REGISTRY.with(|registry| {
            registry.borrow_mut().remove(&(self.raw as usize));
        });

        unsafe {
            crate::class::cleanup_class_cache(self.raw);
            crate::runtime::clear_unhandled_rejections(self.raw);
            if !self.handle_scope.is_null() {
                arkjs::OH_JSVM_CloseHandleScope(self.raw, self.handle_scope);
            }
            if !self.env_scope.is_null() {
                arkjs::OH_JSVM_CloseEnvScope(self.raw, self.env_scope);
            }
            arkjs::OH_JSVM_DestroyEnv(self.raw);
        }
    }
}

pub(crate) fn context_guard_from_env(env: arkjs::JSVM_Env) -> Option<Rc<ArkJSContextInner>> {
    if env.is_null() {
        return None;
    }
    CONTEXT_REGISTRY.with(|registry| {
        registry
            .borrow()
            .get(&(env as usize))
            .and_then(Weak::upgrade)
    })
}

pub struct ArkJSContext {
    raw: arkjs::JSVM_Env,
    vm: arkjs::JSVM_VM,
    context_guard: Option<Rc<ArkJSContextInner>>,
}

impl JSContextImpl for ArkJSContext {
    type RawContext = arkjs::JSVM_Env;
    type Runtime = ArkJSRuntime;
    type Value = ArkJSValue;

    fn new(runtime: &Self::Runtime) -> Self {
        let mut env: arkjs::JSVM_Env = ptr::null_mut();
        let mut env_scope: arkjs::JSVM_EnvScope = ptr::null_mut();
        let mut handle_scope: arkjs::JSVM_HandleScope = ptr::null_mut();

        unsafe {
            let status = arkjs::OH_JSVM_CreateEnv(runtime.to_raw(), 0, ptr::null(), &mut env);
            if status != arkjs::JSVM_Status_JSVM_OK {
                panic!("Failed to create Ark JS environment: {:?}", status);
            }

            let status = arkjs::OH_JSVM_OpenEnvScope(env, &mut env_scope);
            if status != arkjs::JSVM_Status_JSVM_OK {
                arkjs::OH_JSVM_DestroyEnv(env);
                panic!("Failed to open env scope: {:?}", status);
            }

            let status = arkjs::OH_JSVM_OpenHandleScope(env, &mut handle_scope);
            if status != arkjs::JSVM_Status_JSVM_OK {
                arkjs::OH_JSVM_CloseEnvScope(env, env_scope);
                arkjs::OH_JSVM_DestroyEnv(env);
                panic!("Failed to open handle scope: {:?}", status);
            }

            // Store the VM pointer on the env so from_borrowed_raw can recover it
            let vm = runtime.to_raw();
            arkjs::OH_JSVM_SetInstanceData(env, vm as *mut std::ffi::c_void, None, ptr::null_mut());

            let inner = Rc::new(ArkJSContextInner {
                raw: env,
                env_scope,
                handle_scope,
                runtime: runtime.inner.clone(),
            });
            CONTEXT_REGISTRY.with(|registry| {
                registry
                    .borrow_mut()
                    .insert(env as usize, Rc::downgrade(&inner));
            });

            Self {
                raw: env,
                vm,
                context_guard: Some(inner),
            }
        }
    }

    fn as_raw(&self) -> &Self::RawContext {
        &self.raw
    }

    fn context_id(ctx: &Self::RawContext) -> usize {
        *ctx as *const _ as usize
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn from_borrowed_raw(ctx: Self::RawContext) -> Self {
        let context_guard = context_guard_from_env(ctx);
        let vm = context_guard.as_ref().map_or_else(
            || unsafe {
                // Support externally-created environments that are not in the
                // registry by recovering the VM pointer from instance data.
                let mut data: *mut std::ffi::c_void = ptr::null_mut();
                arkjs::OH_JSVM_GetInstanceData(ctx, &mut data);
                data as arkjs::JSVM_VM
            },
            |guard| guard.runtime.raw,
        );
        Self {
            raw: ctx,
            vm,
            context_guard,
        }
    }

    fn eval(&self, source: rong_core::Source) -> Self::Value {
        let code = source.code();
        let resource_name = source.name().unwrap_or("eval");
        let resource_name = CString::new(resource_name).unwrap_or_else(|_| {
            CString::new(resource_name.replace('\0', "\u{fffd}"))
                .expect("replacing null bytes must produce a valid C string")
        });

        unsafe {
            // Clear any pending exception so JSVM API calls succeed
            let mut is_pending = false;
            arkjs::OH_JSVM_IsExceptionPending(self.raw, &mut is_pending);
            if is_pending {
                let mut stale: arkjs::JSVM_Value = ptr::null_mut();
                arkjs::OH_JSVM_GetAndClearLastException(self.raw, &mut stale);
            }

            let mut result: arkjs::JSVM_Value = ptr::null_mut();
            let mut script_value: arkjs::JSVM_Value = ptr::null_mut();
            let status = arkjs::OH_JSVM_CreateStringUtf8(
                self.raw,
                code.as_ptr().cast(),
                code.len(),
                &mut script_value,
            );

            if status != arkjs::JSVM_Status_JSVM_OK {
                let mut exception: arkjs::JSVM_Value = ptr::null_mut();
                arkjs::OH_JSVM_GetAndClearLastException(self.raw, &mut exception);
                if !exception.is_null() {
                    return ArkJSValue::from_owned_raw(self.raw, exception)
                        .protect()
                        .with_exception();
                }
                return Self::Value::create_undefined(self);
            }

            let mut script: arkjs::JSVM_Script = ptr::null_mut();
            let mut cache_rejected = false;
            let mut origin = arkjs::JSVM_ScriptOrigin {
                sourceMapUrl: ptr::null(),
                resourceName: resource_name.as_ptr(),
                resourceLineOffset: 0,
                resourceColumnOffset: 0,
            };
            let status = arkjs::OH_JSVM_CompileScriptWithOrigin(
                self.raw,
                script_value,
                ptr::null(),
                0,
                true,
                &mut cache_rejected,
                &mut origin,
                &mut script,
            );

            if status != arkjs::JSVM_Status_JSVM_OK {
                let mut exception: arkjs::JSVM_Value = ptr::null_mut();
                arkjs::OH_JSVM_GetAndClearLastException(self.raw, &mut exception);
                if !exception.is_null() {
                    return ArkJSValue::from_owned_raw(self.raw, exception)
                        .protect()
                        .with_exception();
                }
                return Self::Value::create_undefined(self);
            }

            let status = arkjs::OH_JSVM_RunScript(self.raw, script, &mut result);

            // Drain microtasks (promise callbacks) after script execution
            let _ = arkjs::OH_JSVM_PerformMicrotaskCheckpoint(self.vm);

            if status != arkjs::JSVM_Status_JSVM_OK {
                let mut exception: arkjs::JSVM_Value = ptr::null_mut();
                arkjs::OH_JSVM_GetAndClearLastException(self.raw, &mut exception);
                if !exception.is_null() {
                    return ArkJSValue::from_owned_raw(self.raw, exception)
                        .protect()
                        .with_exception();
                }
                return Self::Value::create_undefined(self);
            }

            ArkJSValue::from_owned_raw(self.raw, result)
        }
    }

    fn global(&self) -> Self::Value {
        unsafe {
            let mut global: arkjs::JSVM_Value = ptr::null_mut();
            let status = arkjs::OH_JSVM_GetGlobal(self.raw, &mut global);

            if status != arkjs::JSVM_Status_JSVM_OK {
                panic!("Failed to get global object: {:?}", status);
            }

            ArkJSValue::from_owned_raw(self.raw, global)
        }
    }

    fn register_class<JC>(&self) -> Self::Value
    where
        JC: JSClass<Self::Value>,
    {
        crate::class::register_class_internal::<JC>(self, JC::NAME)
    }

    fn call(&self, function: &Self::Value, this: Self::Value, argv: &[Self::Value]) -> Self::Value {
        unsafe {
            let mut result: arkjs::JSVM_Value = ptr::null_mut();

            // Clear any pending exception left over from a previous operation
            // (e.g. IntoJSValue for Result::Err calls throw_js_exception which
            // sets a pending exception via OH_JSVM_Throw — if the Rust side catches
            // the error and wants to call reject(), the pending exception would
            // cause OH_JSVM_CallFunction to fail immediately).
            let mut is_pending = false;
            arkjs::OH_JSVM_IsExceptionPending(self.raw, &mut is_pending);
            if is_pending {
                let mut stale: arkjs::JSVM_Value = ptr::null_mut();
                arkjs::OH_JSVM_GetAndClearLastException(self.raw, &mut stale);
            }

            // Resolve handles from references if needed to get fresh local handles.
            // Protected values may have stale local handles after async boundaries.
            let func_handle = function.resolve_handle();
            let this_handle = this.resolve_handle();
            let args: Vec<arkjs::JSVM_Value> = argv.iter().map(|v| v.resolve_handle()).collect();

            let status = arkjs::OH_JSVM_CallFunction(
                self.raw,
                this_handle,
                func_handle,
                args.len(),
                args.as_ptr(),
                &mut result,
            );

            // Drain microtasks after function calls
            let _ = arkjs::OH_JSVM_PerformMicrotaskCheckpoint(self.vm);

            if status != arkjs::JSVM_Status_JSVM_OK {
                let mut exception: arkjs::JSVM_Value = ptr::null_mut();
                arkjs::OH_JSVM_GetAndClearLastException(self.raw, &mut exception);
                return ArkJSValue::from_owned_raw(self.raw, exception)
                    .protect()
                    .with_exception();
            }

            ArkJSValue::from_owned_raw(self.raw, result)
        }
    }

    fn promise(&self) -> (Self::Value, Self::Value, Self::Value) {
        unsafe {
            let mut deferred: arkjs::JSVM_Deferred = ptr::null_mut();
            let mut promise: arkjs::JSVM_Value = ptr::null_mut();

            let status = arkjs::OH_JSVM_CreatePromise(self.raw, &mut deferred, &mut promise);

            if status != arkjs::JSVM_Status_JSVM_OK {
                let undefined = Self::Value::create_undefined(self);
                return (undefined.clone(), undefined.clone(), undefined);
            }

            let resolve_func = create_deferred_function(self, deferred, true);
            let reject_func = create_deferred_function(self, deferred, false);

            (
                ArkJSValue::from_owned_raw(self.raw, promise).protect(),
                resolve_func,
                reject_func,
            )
        }
    }

    fn register_promise_handlers(
        &self,
        promise: &Self::Value,
        on_fulfilled: &Self::Value,
        on_rejected: &Self::Value,
    ) -> PromiseHandlerRegistration {
        unsafe {
            let promise_handle = promise.resolve_handle();

            // ArkJS does not fire .then() / PromiseRegisterHandler callbacks
            // for already-settled promises via PerformMicrotaskCheckpoint.
            // Work around: the promise_reject_handler captures rejection values
            // when promises are rejected without handlers.  Check if we have a
            // captured rejection for this promise and deliver it directly.
            if let Some(rejection_value) =
                crate::runtime::take_unhandled_rejection(self.raw, promise_handle)
            {
                // Directly call the on_rejected callback with the captured value
                let rejected_handle = on_rejected.resolve_handle();
                let this = ArkJSValue::create_undefined(self);
                let this_handle = this.resolve_handle();
                let argv = [rejection_value];
                let mut result: arkjs::JSVM_Value = ptr::null_mut();
                let status = arkjs::OH_JSVM_CallFunction(
                    self.raw,
                    this_handle,
                    rejected_handle,
                    1,
                    argv.as_ptr(),
                    &mut result,
                );
                if status == arkjs::JSVM_Status_JSVM_OK {
                    return PromiseHandlerRegistration::NativeOnly;
                }

                let mut stale: arkjs::JSVM_Value = ptr::null_mut();
                let _ = arkjs::OH_JSVM_GetAndClearLastException(self.raw, &mut stale);
                return PromiseHandlerRegistration::JavaScriptOnly;
            }

            // For pending promises: register handlers via native API.
            // The callbacks will fire when the promise settles later, but
            // already-settled promises may still need a JS `.then()` backup.
            let fulfilled_handle = on_fulfilled.resolve_handle();
            let rejected_handle = on_rejected.resolve_handle();
            let mut result: arkjs::JSVM_Value = ptr::null_mut();
            let status = arkjs::OH_JSVM_PromiseRegisterHandler(
                self.raw,
                promise_handle,
                fulfilled_handle,
                rejected_handle,
                &mut result,
            );
            if status != arkjs::JSVM_Status_JSVM_OK {
                let mut stale: arkjs::JSVM_Value = ptr::null_mut();
                let _ = arkjs::OH_JSVM_GetAndClearLastException(self.raw, &mut stale);
                return PromiseHandlerRegistration::JavaScriptOnly;
            }
            PromiseHandlerRegistration::NativeWithJavaScriptFallbackIfPending
        }
    }

    fn compile_to_bytecode(&self, source: rong_core::Source) -> Result<Vec<u8>, RongJSError> {
        let code = source.code();

        unsafe {
            let code_cstr = CString::new(code).unwrap();
            let mut script_value: arkjs::JSVM_Value = ptr::null_mut();
            let status = arkjs::OH_JSVM_CreateStringUtf8(
                self.raw,
                code_cstr.as_ptr(),
                code.len(),
                &mut script_value,
            );
            if status != arkjs::JSVM_Status_JSVM_OK {
                return Err(compile_to_bytecode_failed());
            }

            let mut script: arkjs::JSVM_Script = ptr::null_mut();
            let mut cache_rejected = false;
            let status = arkjs::OH_JSVM_CompileScript(
                self.raw,
                script_value,
                ptr::null(),
                0,
                true,
                &mut cache_rejected,
                &mut script,
            );
            if status != arkjs::JSVM_Status_JSVM_OK {
                return Err(compile_to_bytecode_failed());
            }

            // Generate code cache from compiled script
            let mut cache_data: *const u8 = ptr::null();
            let mut cache_len: usize = 0;
            let status =
                arkjs::OH_JSVM_CreateCodeCache(self.raw, script, &mut cache_data, &mut cache_len);
            if status != arkjs::JSVM_Status_JSVM_OK || cache_data.is_null() {
                return Err(compile_to_bytecode_failed());
            }

            // Pack: [source_len: 4 bytes LE][source bytes][cache bytes]
            let source_bytes = code;
            let source_len = source_bytes.len() as u32;
            let mut result = Vec::with_capacity(4 + source_bytes.len() + cache_len);
            result.extend_from_slice(&source_len.to_le_bytes());
            result.extend_from_slice(source_bytes);
            result.extend_from_slice(std::slice::from_raw_parts(cache_data, cache_len));

            Ok(result)
        }
    }

    fn run_bytecode(&self, bytes: &[u8]) -> Self::Value {
        if bytes.len() < 4 {
            return ArkJSValue::create_undefined(self);
        }

        // Unpack: [source_len: 4 bytes LE][source bytes][cache bytes]
        let source_len = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        if bytes.len() < 4 + source_len {
            return ArkJSValue::create_undefined(self);
        }
        let source_bytes = &bytes[4..4 + source_len];
        let cache_bytes = &bytes[4 + source_len..];

        unsafe {
            let code_cstr = match CString::new(source_bytes) {
                Ok(s) => s,
                Err(_) => return ArkJSValue::create_undefined(self),
            };
            let mut script_value: arkjs::JSVM_Value = ptr::null_mut();
            let status = arkjs::OH_JSVM_CreateStringUtf8(
                self.raw,
                code_cstr.as_ptr(),
                source_bytes.len(),
                &mut script_value,
            );
            if status != arkjs::JSVM_Status_JSVM_OK {
                return ArkJSValue::create_undefined(self);
            }

            let mut script: arkjs::JSVM_Script = ptr::null_mut();
            let mut cache_rejected = false;
            let status = arkjs::OH_JSVM_CompileScript(
                self.raw,
                script_value,
                cache_bytes.as_ptr(),
                cache_bytes.len(),
                true,
                &mut cache_rejected,
                &mut script,
            );
            if status != arkjs::JSVM_Status_JSVM_OK {
                return ArkJSValue::create_undefined(self);
            }

            let mut result: arkjs::JSVM_Value = ptr::null_mut();
            let status = arkjs::OH_JSVM_RunScript(self.raw, script, &mut result);

            let _ = arkjs::OH_JSVM_PerformMicrotaskCheckpoint(self.vm);

            if status != arkjs::JSVM_Status_JSVM_OK {
                let mut exception: arkjs::JSVM_Value = ptr::null_mut();
                arkjs::OH_JSVM_GetAndClearLastException(self.raw, &mut exception);
                if !exception.is_null() {
                    return ArkJSValue::from_owned_raw(self.raw, exception)
                        .protect()
                        .with_exception();
                }
                return ArkJSValue::create_undefined(self);
            }

            ArkJSValue::from_owned_raw(self.raw, result)
        }
    }
}

impl ArkJSContext {
    pub(crate) fn to_raw(&self) -> arkjs::JSVM_Env {
        self.raw
    }
}

impl Clone for ArkJSContext {
    fn clone(&self) -> Self {
        Self {
            raw: self.raw,
            vm: self.vm,
            context_guard: self.context_guard.clone(),
        }
    }
}

impl JSErrorFactory for ArkJSContext {
    fn new_error(&self, name: &str, message: impl AsRef<str>, code: Option<&str>) -> Self::Value {
        unsafe {
            // Clear any pending exception so JSVM API calls succeed
            // (e.g. consecutive throw_*() calls in tests).
            let mut is_pending = false;
            arkjs::OH_JSVM_IsExceptionPending(self.raw, &mut is_pending);
            if is_pending {
                let mut stale: arkjs::JSVM_Value = ptr::null_mut();
                arkjs::OH_JSVM_GetAndClearLastException(self.raw, &mut stale);
            }

            let mut error: arkjs::JSVM_Value = ptr::null_mut();
            let message_cstr = CString::new(message.as_ref()).unwrap();

            let mut message_value: arkjs::JSVM_Value = ptr::null_mut();
            let _ = arkjs::OH_JSVM_CreateStringUtf8(
                self.raw,
                message_cstr.as_ptr(),
                message.as_ref().len(),
                &mut message_value,
            );

            let status = match name {
                "TypeError" => arkjs::OH_JSVM_CreateTypeError(
                    self.raw,
                    ptr::null_mut(),
                    message_value,
                    &mut error,
                ),
                "RangeError" => arkjs::OH_JSVM_CreateRangeError(
                    self.raw,
                    ptr::null_mut(),
                    message_value,
                    &mut error,
                ),
                "SyntaxError" => arkjs::OH_JSVM_CreateSyntaxError(
                    self.raw,
                    ptr::null_mut(),
                    message_value,
                    &mut error,
                ),
                // JSVM API lacks dedicated constructors for ReferenceError, URIError, etc.
                // Create them via JS eval to get the correct constructor and prototype chain.
                "ReferenceError" | "URIError" | "EvalError" => {
                    // Store message on global temporarily, eval `new <ErrorType>(msg)`, then clean up
                    let mut global: arkjs::JSVM_Value = ptr::null_mut();
                    arkjs::OH_JSVM_GetGlobal(self.raw, &mut global);
                    let _ = arkjs::OH_JSVM_SetNamedProperty(
                        self.raw,
                        global,
                        c"__rong_err_msg__".as_ptr() as _,
                        message_value,
                    );
                    let script = CString::new(format!("new {name}(__rong_err_msg__)")).unwrap();
                    let mut script_value: arkjs::JSVM_Value = ptr::null_mut();
                    arkjs::OH_JSVM_CreateStringUtf8(
                        self.raw,
                        script.as_ptr(),
                        script.as_bytes().len(),
                        &mut script_value,
                    );
                    let mut compiled: arkjs::JSVM_Script = ptr::null_mut();
                    let mut s = arkjs::OH_JSVM_CompileScript(
                        self.raw,
                        script_value,
                        ptr::null(),
                        0,
                        false,
                        ptr::null_mut(),
                        &mut compiled,
                    );
                    if s == arkjs::JSVM_Status_JSVM_OK {
                        let mut result: arkjs::JSVM_Value = ptr::null_mut();
                        s = arkjs::OH_JSVM_RunScript(self.raw, compiled, &mut result);
                        if s == arkjs::JSVM_Status_JSVM_OK {
                            error = result;
                        }
                    }
                    // Clean up temp global
                    let mut undefined: arkjs::JSVM_Value = ptr::null_mut();
                    arkjs::OH_JSVM_GetUndefined(self.raw, &mut undefined);
                    let _ = arkjs::OH_JSVM_SetNamedProperty(
                        self.raw,
                        global,
                        c"__rong_err_msg__".as_ptr() as _,
                        undefined,
                    );
                    s
                }
                _ => {
                    let s = arkjs::OH_JSVM_CreateError(
                        self.raw,
                        ptr::null_mut(),
                        message_value,
                        &mut error,
                    );
                    if s == arkjs::JSVM_Status_JSVM_OK && name != "Error" {
                        let name_cstr = CString::new(name).unwrap();
                        let mut name_value: arkjs::JSVM_Value = ptr::null_mut();
                        let _ = arkjs::OH_JSVM_CreateStringUtf8(
                            self.raw,
                            name_cstr.as_ptr(),
                            name.len(),
                            &mut name_value,
                        );
                        let _ = arkjs::OH_JSVM_SetNamedProperty(
                            self.raw,
                            error,
                            c"name".as_ptr() as _,
                            name_value,
                        );
                    }
                    s
                }
            };

            if status != arkjs::JSVM_Status_JSVM_OK {
                return Self::Value::create_undefined(self);
            }

            if let Some(code) = code {
                let code_cstr = CString::new(code).unwrap();
                let mut code_value: arkjs::JSVM_Value = ptr::null_mut();
                let _ = arkjs::OH_JSVM_CreateStringUtf8(
                    self.raw,
                    code_cstr.as_ptr(),
                    code.len(),
                    &mut code_value,
                );
                let _ = arkjs::OH_JSVM_SetNamedProperty(
                    self.raw,
                    error,
                    c"code".as_ptr() as _,
                    code_value,
                );
            }

            ArkJSValue::from_owned_raw(self.raw, error).with_error()
        }
    }
}

impl JSExceptionThrower for ArkJSContext {
    fn throw(&self, value: Self::Value) -> Self::Value {
        unsafe {
            arkjs::OH_JSVM_Throw(self.raw, value.raw_value_for_api());
        }
        value.with_exception()
    }
}

/// Creates a resolve or reject function wrapping a JSVM_Deferred.
fn create_deferred_function(
    ctx: &ArkJSContext,
    deferred: arkjs::JSVM_Deferred,
    is_resolve: bool,
) -> ArkJSValue {
    unsafe {
        let mut func: arkjs::JSVM_Value = ptr::null_mut();
        let state = Box::into_raw(Box::new(DeferredCallbackState {
            callback: arkjs::JSVM_CallbackStruct {
                callback: Some(deferred_callback),
                data: ptr::null_mut(),
            },
            deferred,
            is_resolve,
        }));

        let name = if is_resolve { c"resolve" } else { c"reject" };
        let status = arkjs::OH_JSVM_CreateFunction(
            ctx.to_raw(),
            name.as_ptr() as _,
            name.to_bytes().len(),
            &mut (*state).callback as *mut _,
            &mut func,
        );

        if status == arkjs::JSVM_Status_JSVM_OK {
            (*state).callback.data = state as *mut std::ffi::c_void;
            let mut wrapper_obj: arkjs::JSVM_Value = ptr::null_mut();
            if arkjs::OH_JSVM_CreateObject(ctx.to_raw(), &mut wrapper_obj)
                == arkjs::JSVM_Status_JSVM_OK
            {
                let wrapper_data = Box::into_raw(Box::new(DeferredWrapperData { state }));
                let wrap_status = arkjs::OH_JSVM_Wrap(
                    ctx.to_raw(),
                    wrapper_obj,
                    wrapper_data as *mut std::ffi::c_void,
                    Some(deferred_wrapper_finalizer),
                    ptr::null_mut(),
                    ptr::null_mut(),
                );
                if wrap_status == arkjs::JSVM_Status_JSVM_OK {
                    let status = crate::class::define_hidden_value_property(
                        ctx.to_raw(),
                        func,
                        c"__rong_deferred",
                        wrapper_obj,
                    );
                    if status == arkjs::JSVM_Status_JSVM_OK {
                        return ArkJSValue::from_owned_raw(ctx.to_raw(), func).protect();
                    }

                    let mut stale: arkjs::JSVM_Value = ptr::null_mut();
                    let _ = arkjs::OH_JSVM_GetAndClearLastException(ctx.to_raw(), &mut stale);
                    return ArkJSValue::create_undefined(ctx);
                }
                let _ = Box::from_raw(wrapper_data);
            }
            let _ = Box::from_raw(state);
            ArkJSValue::create_undefined(ctx)
        } else {
            let _ = Box::from_raw(state);
            ArkJSValue::create_undefined(ctx)
        }
    }
}

struct DeferredCallbackState {
    callback: arkjs::JSVM_CallbackStruct,
    deferred: arkjs::JSVM_Deferred,
    is_resolve: bool,
}

struct DeferredWrapperData {
    state: *mut DeferredCallbackState,
}

unsafe extern "C" fn deferred_wrapper_finalizer(
    _env: arkjs::JSVM_Env,
    finalize_data: *mut std::ffi::c_void,
    _finalize_hint: *mut std::ffi::c_void,
) {
    if finalize_data.is_null() {
        return;
    }

    let wrapper = unsafe { Box::from_raw(finalize_data as *mut DeferredWrapperData) };
    if !wrapper.state.is_null() {
        let _ = unsafe { Box::from_raw(wrapper.state) };
    }
}

unsafe extern "C" fn deferred_callback(
    env: arkjs::JSVM_Env,
    info: arkjs::JSVM_CallbackInfo,
) -> arkjs::JSVM_Value {
    unsafe {
        let mut argc: usize = 1;
        let mut argv: [arkjs::JSVM_Value; 1] = [ptr::null_mut()];
        let mut this_arg: arkjs::JSVM_Value = ptr::null_mut();
        let mut data: *mut std::ffi::c_void = ptr::null_mut();

        let status = arkjs::OH_JSVM_GetCbInfo(
            env,
            info,
            &mut argc,
            argv.as_mut_ptr(),
            &mut this_arg,
            &mut data,
        );

        if status == arkjs::JSVM_Status_JSVM_OK && !data.is_null() {
            let state = &*(data as *mut DeferredCallbackState);
            let value = if argc > 0 { argv[0] } else { ptr::null_mut() };

            if state.is_resolve {
                arkjs::OH_JSVM_ResolveDeferred(env, state.deferred, value);
            } else {
                arkjs::OH_JSVM_RejectDeferred(env, state.deferred, value);
            }
        }

        let mut undefined: arkjs::JSVM_Value = ptr::null_mut();
        arkjs::OH_JSVM_GetUndefined(env, &mut undefined);
        undefined
    }
}
