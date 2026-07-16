use crate::{QJSRuntime, QJSValue, qjs};
use rong_core::{
    HostError, JSClass, JSContextImpl, JSErrorFactory, JSExceptionThrower, JSRuntimeImpl, JSTypeOf,
    JSValueImpl, RongJSError, Source,
};
use smallvec::SmallVec;
use std::ffi::CString;
use std::mem::MaybeUninit;
use std::rc::Rc;

use crate::runtime::{QJSRuntimeInner, runtime_guard_from_ctx};

fn compile_to_bytecode_failed() -> RongJSError {
    HostError::new(
        rong_core::error::E_COMPILE,
        "Failed to compile JS code to bytecode",
    )
    .into()
}

pub struct QJSContext {
    pub(crate) ctx: *mut qjs::JSContext,
    // Keeps the owning runtime alive while this context exists.
    // This prevents shutdown-time crashes if Rust values outlive the originating context.
    rt_guard: Option<Rc<QJSRuntimeInner>>,
}

impl Drop for QJSContext {
    fn drop(&mut self) {
        unsafe {
            qjs::JS_FreeContext(self.ctx);
        }
    }
}

impl Clone for QJSContext {
    fn clone(&self) -> Self {
        Self {
            ctx: unsafe { qjs::JS_DupContext(self.ctx) },
            rt_guard: self.rt_guard.clone(),
        }
    }
}

impl JSContextImpl for QJSContext {
    type RawContext = *mut qjs::JSContext;
    type Runtime = QJSRuntime;
    type Value = QJSValue;

    fn new(runtime: &Self::Runtime) -> Self {
        let ctx = unsafe { qjs::JS_NewContext(runtime.to_raw()) };
        Self {
            ctx,
            rt_guard: Some(runtime.inner.clone()),
        }
    }

    fn as_raw(&self) -> &Self::RawContext {
        &self.ctx
    }

    fn from_borrowed_raw(ctx: Self::RawContext) -> Self {
        Self::_from_borrowed_raw(ctx)
    }

    fn eval(&self, source: Source) -> Self::Value {
        let options = EvalOptions::default();
        self.eval_raw(&source, options.to_flags())
    }

    fn compile_to_bytecode(&self, source: Source) -> Result<Vec<u8>, RongJSError> {
        let options = EvalOptions {
            bytecode: true,
            ..EvalOptions::default()
        };
        let obj = self.eval_raw(&source, options.to_flags());
        if obj.is_exception() {
            return Err(compile_to_bytecode_failed());
        }

        let mut out_size: usize = 0;
        let buf = unsafe {
            qjs::JS_WriteObject(
                self.ctx,
                &mut out_size,
                *obj.as_raw_value(),
                qjs::JS_WRITE_OBJ_BYTECODE as _,
            )
        };

        if buf.is_null() {
            return Err(compile_to_bytecode_failed());
        }

        let bytecode = unsafe { std::slice::from_raw_parts(buf, out_size) }.to_vec();
        unsafe {
            qjs::QJS_Free(self.ctx, buf.cast());
        }
        Ok(bytecode)
    }

    fn run_bytecode(&self, bytes: &[u8]) -> Self::Value {
        unsafe {
            let obj = qjs::JS_ReadObject(
                self.ctx,
                bytes.as_ptr(),
                bytes.len(),
                qjs::JS_READ_OBJ_BYTECODE as i32,
            );

            if qjs::QJS_IsException(self.ctx, obj) {
                let exception = qjs::JS_GetException(self.ctx);
                return QJSValue::from_owned_raw(self.ctx, exception).with_exception();
            }

            // JS_EvalFunction takes ownership of `obj`.
            let eval_result = qjs::JS_EvalFunction(self.ctx, obj);
            if qjs::QJS_IsException(self.ctx, eval_result) {
                let exception = qjs::JS_GetException(self.ctx);
                QJSValue::from_owned_raw(self.ctx, exception).with_exception()
            } else {
                QJSValue::from_owned_raw(self.ctx, eval_result)
            }
        }
    }

    fn global(&self) -> Self::Value {
        let raw = unsafe { qjs::JS_GetGlobalObject(self.ctx) };
        QJSValue::from_owned_raw(self.ctx, raw)
    }

    fn register_class<JC>(&self) -> Self::Value
    where
        JC: JSClass<QJSValue>,
    {
        let name = CString::new(JC::NAME).unwrap();
        let raw = unsafe {
            qjs::QJS_CreateClass(
                self.ctx,
                name.as_ptr(),
                Some(crate::class::generic_constructor::<JC>),
                Some(crate::class::call::<JC>),
                Some(crate::class::finalizer::<JC>),
                if std::any::type_name::<JC>().contains("RustFunc") {
                    None
                } else {
                    Some(crate::class::gc_mark::<JC>)
                },
            )
        };
        QJSValue::from_owned_raw(self.ctx, raw)
    }

    fn call(&self, function: &Self::Value, this: Self::Value, argv: &[Self::Value]) -> Self::Value {
        // Convert argv to raw JSValues
        let mut args: SmallVec<[qjs::JSValue; 4]> =
            argv.iter().map(|v| *v.as_raw_value()).collect();

        let val = unsafe {
            qjs::JS_Call(
                self.ctx,
                *function.as_raw_value(),
                *this.as_raw_value(),
                args.len() as std::ffi::c_int,
                args.as_mut_ptr(),
            )
        };

        if unsafe { qjs::QJS_IsException(self.ctx, val) } {
            let exception = unsafe { qjs::JS_GetException(self.ctx) };
            QJSValue::from_owned_raw(self.ctx, exception).with_exception()
        } else {
            QJSValue::from_owned_raw(self.ctx, val)
        }
    }

    fn promise(&self) -> (Self::Value, Self::Value, Self::Value) {
        // Create uninitialized array
        let mut resolving_funcs = MaybeUninit::<[qjs::JSValue; 2]>::uninit();

        // Get raw pointer to the array
        let resolving_funcs_ptr = resolving_funcs.as_mut_ptr() as *mut qjs::JSValue;

        // Create promise
        let promise = unsafe { qjs::JS_NewPromiseCapability(self.ctx, resolving_funcs_ptr) };

        // On exception, QuickJS may not have initialized `resolving_funcs`.
        if unsafe { qjs::QJS_IsException(self.ctx, promise) } {
            let undef =
                || QJSValue::from_owned_raw(self.ctx, unsafe { qjs::QJS_NewUndefined(self.ctx) });
            return (undef(), undef(), undef());
        }

        // Safety: JS_NewPromiseCapability initializes the array on success.
        let resolving_funcs = unsafe { resolving_funcs.assume_init() };

        let resolve = QJSValue::from_owned_raw(self.ctx, resolving_funcs[0]);
        let reject = QJSValue::from_owned_raw(self.ctx, resolving_funcs[1]);

        (QJSValue::from_owned_raw(self.ctx, promise), resolve, reject)
    }

    fn context_id(ctx: &Self::RawContext) -> usize {
        *ctx as *const _ as usize
    }
}

impl QJSContext {
    fn _from_borrowed_raw(ctx: *mut qjs::JSContext) -> Self {
        let ctx = unsafe { qjs::JS_DupContext(ctx) };
        Self {
            rt_guard: runtime_guard_from_ctx(ctx),
            ctx,
        }
    }

    pub(crate) fn to_raw(&self) -> *mut qjs::JSContext {
        self.ctx
    }

    /// Converts a raw JSValue to QJSValue, handling exceptions gracefully.
    ///
    /// This function takes a raw JSValue from QuickJS and converts it into a QJSValue.
    /// If the input value represents an exception, it will be extracted and returned
    /// as a QJSValue with the exception flag set. Otherwise, a normal QJSValue will
    /// be returned.
    ///
    /// # Safety
    /// - The input `raw` must be a valid JSValue obtained from QuickJS
    /// - The context (`self`) must be valid and match the context where `raw` was created
    ///
    /// # Returns
    /// - QJSValue containing either the converted value or the exception
    pub(crate) fn to_owned_value(&self, raw: qjs::JSValue) -> QJSValue {
        let ctx = self.to_raw();
        if unsafe { qjs::QJS_IsException(ctx, raw) } {
            let exception = unsafe { qjs::JS_GetException(ctx) };
            QJSValue::from_owned_raw(ctx, exception).with_exception()
        } else {
            QJSValue::from_owned_raw(ctx, raw)
        }
    }
}

// eval option assiciated with JS_EVAL_*
#[derive(Clone, Copy)]
struct EvalOptions {
    global: bool,
    strict: bool,
    promise: bool,
    backtrace_barrier: bool,
    bytecode: bool,
}

impl Default for EvalOptions {
    fn default() -> Self {
        Self {
            global: true,
            strict: true,
            promise: false,
            bytecode: false,
            backtrace_barrier: false,
        }
    }
}

impl EvalOptions {
    fn to_flags(self) -> i32 {
        let mut flags = if self.global {
            qjs::JS_EVAL_TYPE_GLOBAL
        } else {
            qjs::JS_EVAL_TYPE_MODULE
        };

        if self.strict {
            flags |= qjs::JS_EVAL_FLAG_STRICT;
        }

        if self.promise {
            flags |= qjs::JS_EVAL_FLAG_ASYNC;
        }
        if self.backtrace_barrier {
            flags |= qjs::JS_EVAL_FLAG_BACKTRACE_BARRIER;
        }
        if self.bytecode {
            flags |= qjs::JS_EVAL_FLAG_COMPILE_ONLY;
        }
        flags as _
    }
}

impl QJSContext {
    fn eval_raw(&self, source: &Source, flags: i32) -> QJSValue {
        let filename = source.name().unwrap_or("eval");
        let c_filename = CString::new(filename).unwrap_or_else(|_| {
            CString::new(filename.replace('\0', "\u{fffd}"))
                .expect("replacing null bytes must produce a valid C string")
        });
        let code_len = source.code().len();
        let mut code = source.code().to_vec();
        code.push(0);

        unsafe {
            let val = qjs::JS_Eval(
                self.ctx,
                code.as_ptr().cast(),
                code_len,
                c_filename.as_ptr(),
                flags,
            );
            if qjs::QJS_IsException(self.ctx, val) {
                let exception = qjs::JS_GetException(self.ctx);
                QJSValue::from_owned_raw(self.ctx, exception).with_exception()
            } else {
                QJSValue::from_owned_raw(self.ctx, val)
            }
        }
    }
}

impl JSErrorFactory for QJSContext {
    fn new_error(&self, name: &str, message: impl AsRef<str>, code: Option<&str>) -> Self::Value {
        let message = message.as_ref();
        unsafe {
            let global = qjs::JS_GetGlobalObject(self.ctx);

            let ctor_name = if matches!(
                name,
                "Error" | "TypeError" | "RangeError" | "ReferenceError" | "SyntaxError"
            ) {
                name
            } else {
                "Error"
            };

            let ctor_name_c = CString::new(ctor_name).unwrap();
            let ctor = qjs::JS_GetPropertyStr(self.ctx, global, ctor_name_c.as_ptr());

            let mut obj = qjs::QJS_NewUndefined(self.ctx);
            if qjs::JS_IsFunction(self.ctx, ctor) {
                let msg = qjs::JS_NewStringLen(self.ctx, message.as_ptr() as _, message.len() as _);
                let mut args = [msg];
                obj = qjs::JS_CallConstructor(self.ctx, ctor, 1, args.as_mut_ptr());
                qjs::JS_FreeValue(self.ctx, msg);
            }

            if qjs::QJS_IsUndefined(self.ctx, obj) || qjs::QJS_IsException(self.ctx, obj) {
                if !qjs::QJS_IsUndefined(self.ctx, obj) {
                    qjs::JS_FreeValue(self.ctx, obj);
                }
                obj = qjs::JS_NewError(self.ctx);
                let msg = qjs::JS_NewStringLen(self.ctx, message.as_ptr() as _, message.len() as _);
                let _ = qjs::JS_SetPropertyStr(self.ctx, obj, c"message".as_ptr(), msg);
            }

            if name != "Error" {
                let name_val = qjs::JS_NewStringLen(self.ctx, name.as_ptr() as _, name.len() as _);
                let _ = qjs::JS_SetPropertyStr(self.ctx, obj, c"name".as_ptr(), name_val);
            }

            if let Some(code) = code {
                let code_val = qjs::JS_NewStringLen(self.ctx, code.as_ptr() as _, code.len() as _);
                let _ = qjs::JS_DefinePropertyValueStr(
                    self.ctx,
                    obj,
                    c"code".as_ptr(),
                    code_val,
                    (qjs::JS_PROP_WRITABLE | qjs::JS_PROP_CONFIGURABLE) as i32,
                );
            }

            qjs::JS_FreeValue(self.ctx, ctor);
            qjs::JS_FreeValue(self.ctx, global);

            QJSValue::from_owned_raw(self.ctx, obj).with_error()
        }
    }
}

impl JSExceptionThrower for QJSContext {
    fn throw(&self, value: Self::Value) -> Self::Value {
        QJSValue::from_owned_raw(self.ctx, value.into_raw_value()).with_exception()
    }
}
