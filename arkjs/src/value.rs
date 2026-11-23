use crate::context::{ArkJSContextInner, context_guard_from_env};
use crate::{ArkJSContext, arkjs};
use rong_core::{
    JSContextImpl, JSErrorFactory, JSRawContext, JSTypeOf, JSValueImpl, RongJSError,
    impl_js_converter,
};
use std::hash::Hash;
use std::ptr;
use std::rc::Rc;

mod array;
mod array_buffer;
mod object;
mod proxy;
mod typed_array;
mod valuetype;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub(crate) enum ArkJSValueType {
    Object,
    Error,
    Exception,
    Other,
}

/// A JSVM value owned by Rust.
///
/// `value` is a handle scoped to the native call that created it: once that
/// call returns, its slot is reused for other values. Every read therefore
/// goes through `reference`, taken at creation while the handle is still valid,
/// or, for a primitive kept by value, makes a fresh handle from `primitive`.
pub struct ArkJSValue {
    value: arkjs::JSVM_Value,
    env: arkjs::JSVM_Env,
    value_type: ArkJSValueType,
    reference: Option<arkjs::JSVM_Ref>,
    primitive: Option<Primitive>,
    // Keeps the env alive until this value has deleted its reference.
    _context_guard: Option<Rc<ArkJSContextInner>>,
}

impl PartialEq for ArkJSValue {
    fn eq(&self, other: &Self) -> bool {
        let _scope = crate::scope::OpScope::enter(self.env);
        unsafe {
            let lhs = self.resolve_handle();
            let rhs = other.resolve_handle();
            let mut result = false;
            let status = arkjs::OH_JSVM_StrictEquals(self.env, lhs, rhs, &mut result);
            status == arkjs::JSVM_Status_JSVM_OK && result
        }
    }
}

// JSVM has no identity hash and handles differ per read, so only the env is
// stable across values that `eq` (StrictEquals) treats as equal.
impl Hash for ArkJSValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (self.env as usize).hash(state);
    }
}

/// A primitive whose value is all there is to keep, so it needs no
/// persistent reference: reads make a fresh handle from it.
#[derive(Clone, Copy)]
enum Primitive {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
}

impl Primitive {
    fn of(env: arkjs::JSVM_Env, value: arkjs::JSVM_Value) -> Option<Self> {
        if value.is_null() {
            return None;
        }
        unsafe {
            let mut kind: arkjs::JSVM_ValueType = arkjs::JSVM_ValueType_JSVM_UNDEFINED;
            if arkjs::OH_JSVM_Typeof(env, value, &mut kind) != arkjs::JSVM_Status_JSVM_OK {
                return None;
            }
            match kind {
                arkjs::JSVM_ValueType_JSVM_UNDEFINED => Some(Self::Undefined),
                arkjs::JSVM_ValueType_JSVM_NULL => Some(Self::Null),
                arkjs::JSVM_ValueType_JSVM_BOOLEAN => {
                    let mut b = false;
                    (arkjs::OH_JSVM_GetValueBool(env, value, &mut b) == arkjs::JSVM_Status_JSVM_OK)
                        .then_some(Self::Bool(b))
                }
                arkjs::JSVM_ValueType_JSVM_NUMBER => {
                    let mut n = 0.0;
                    (arkjs::OH_JSVM_GetValueDouble(env, value, &mut n)
                        == arkjs::JSVM_Status_JSVM_OK)
                        .then_some(Self::Number(n))
                }
                _ => None,
            }
        }
    }

    fn handle(self, env: arkjs::JSVM_Env) -> arkjs::JSVM_Value {
        let mut value: arkjs::JSVM_Value = ptr::null_mut();
        unsafe {
            match self {
                Self::Undefined => arkjs::OH_JSVM_GetUndefined(env, &mut value),
                Self::Null => arkjs::OH_JSVM_GetNull(env, &mut value),
                Self::Bool(b) => arkjs::OH_JSVM_GetBoolean(env, b, &mut value),
                Self::Number(n) => arkjs::OH_JSVM_CreateDouble(env, n, &mut value),
            };
        }
        value
    }
}

impl ArkJSValue {
    pub(crate) fn new(env: arkjs::JSVM_Env, value: arkjs::JSVM_Value) -> Self {
        Self {
            env,
            value,
            value_type: ArkJSValueType::Other,
            reference: None,
            primitive: Primitive::of(env, value),
            _context_guard: context_guard_from_env(env),
        }
        .protect()
    }

    pub(crate) fn with_exception(mut self) -> Self {
        self.value_type = ArkJSValueType::Exception;
        self
    }

    pub(crate) fn with_object(mut self) -> Self {
        self.value_type = ArkJSValueType::Object;
        self
    }

    pub(crate) fn with_error(mut self) -> Self {
        self.value_type = ArkJSValueType::Error;
        self
    }

    pub(crate) fn _is_err(&self) -> bool {
        self.value_type == ArkJSValueType::Error
    }

    pub(crate) fn _is_exception(&self) -> bool {
        self.value_type == ArkJSValueType::Exception
    }

    pub(crate) fn _is_object(&self) -> bool {
        self.value_type == ArkJSValueType::Object
    }

    fn pending_exception_or_type_error(&self, message: &str) -> Self {
        unsafe {
            let mut exception: arkjs::JSVM_Value = ptr::null_mut();
            arkjs::OH_JSVM_GetAndClearLastException(self.env, &mut exception);
            if !exception.is_null() {
                return Self::from_owned_raw(self.env, exception).with_exception();
            }
        }

        let ctx = <ArkJSContext as JSContextImpl>::from_borrowed_raw(self.env);
        ctx.new_error("TypeError", message, None).with_exception()
    }

    pub(crate) fn reflect_set(&self, key: arkjs::JSVM_Value, value: arkjs::JSVM_Value) -> Self {
        unsafe {
            let mut global: arkjs::JSVM_Value = ptr::null_mut();
            let mut reflect: arkjs::JSVM_Value = ptr::null_mut();
            let mut set: arkjs::JSVM_Value = ptr::null_mut();

            let status = arkjs::OH_JSVM_GetGlobal(self.env, &mut global);
            if status != arkjs::JSVM_Status_JSVM_OK {
                return self.pending_exception_or_type_error("Failed to get global object");
            }
            let status = arkjs::OH_JSVM_GetNamedProperty(
                self.env,
                global,
                c"Reflect".as_ptr() as _,
                &mut reflect,
            );
            if status != arkjs::JSVM_Status_JSVM_OK {
                return self.pending_exception_or_type_error("Failed to get Reflect");
            }
            let status =
                arkjs::OH_JSVM_GetNamedProperty(self.env, reflect, c"set".as_ptr() as _, &mut set);
            if status != arkjs::JSVM_Status_JSVM_OK {
                return self.pending_exception_or_type_error("Failed to get Reflect.set");
            }

            let args = [self.resolve_handle(), key, value];
            let mut result: arkjs::JSVM_Value = ptr::null_mut();
            let status = arkjs::OH_JSVM_CallFunction(
                self.env,
                reflect,
                set,
                args.len(),
                args.as_ptr(),
                &mut result,
            );
            if status != arkjs::JSVM_Status_JSVM_OK {
                return self.pending_exception_or_type_error("Failed to set JavaScript property");
            }

            let mut was_set = false;
            let status = arkjs::OH_JSVM_GetValueBool(self.env, result, &mut was_set);
            if status != arkjs::JSVM_Status_JSVM_OK {
                return self
                    .pending_exception_or_type_error("Failed to read JavaScript property result");
            }
            if !was_set {
                return self.pending_exception_or_type_error("Failed to set JavaScript property");
            }

            Self::create_undefined(&<ArkJSContext as JSContextImpl>::from_borrowed_raw(
                self.env,
            ))
        }
    }

    /// Takes the persistent reference this value reads through; a no-op once taken.
    fn protect(mut self) -> Self {
        if self.reference.is_some() || self.primitive.is_some() || self.value.is_null() {
            return self;
        }
        unsafe {
            let mut ref_value: arkjs::JSVM_Ref = ptr::null_mut();
            let status = arkjs::OH_JSVM_CreateReference(self.env, self.value, 1, &mut ref_value);
            if status == arkjs::JSVM_Status_JSVM_OK {
                self.reference = Some(ref_value);
            } else {
                report_refused_reference(status);
            }
        }
        self
    }

    /// Returns a handle valid in the current scope, read through the reference.
    /// Only a value JSVM refused a reference for falls back to its creation
    /// handle, which is valid only inside the native call that created it.
    pub(crate) fn resolve_handle(&self) -> arkjs::JSVM_Value {
        if let Some(primitive) = self.primitive {
            return primitive.handle(self.env);
        }
        if let Some(reference) = self.reference {
            unsafe {
                let mut value: arkjs::JSVM_Value = ptr::null_mut();
                let status = arkjs::OH_JSVM_GetReferenceValue(self.env, reference, &mut value);
                if status == arkjs::JSVM_Status_JSVM_OK && !value.is_null() {
                    return value;
                }
            }
        }
        self.value
    }
}

/// Without a reference a value reads its creation handle, which is reused
/// once its native call returns. JSVM (V8 13.2) references every value type,
/// so say so loudly if some JSVM version does not.
fn report_refused_reference(status: arkjs::JSVM_Status) {
    static REPORTED: std::sync::Once = std::sync::Once::new();
    REPORTED.call_once(|| {
        crate::hilog::warn(&format!(
            "[ArkJS] OH_JSVM_CreateReference refused a value (status {status}); \
             it is readable only inside the native call that created it"
        ));
    });
}

impl Drop for ArkJSValue {
    fn drop(&mut self) {
        // Release the reference if we have one
        if let Some(reference) = self.reference {
            unsafe {
                arkjs::OH_JSVM_DeleteReference(self.env, reference);
            }
        }
    }
}

impl JSRawContext for ArkJSValue {
    type RawContext = arkjs::JSVM_Env;
}

impl JSValueImpl for ArkJSValue {
    type RawValue = arkjs::JSVM_Value;
    type Context = ArkJSContext;

    fn in_handle_scope<R>(
        ctx: <Self::Context as JSContextImpl>::RawContext,
        f: impl FnOnce() -> R,
    ) -> R {
        let _scope = crate::scope::OpScope::enter(ctx);
        f()
    }

    fn from_borrowed_raw(
        ctx: <Self::Context as JSContextImpl>::RawContext,
        value: Self::RawValue,
    ) -> Self {
        ArkJSValue::new(ctx, value)
    }

    // A JSVM handle dies with the native call that made it, so an owned value
    // needs a reference just like a borrowed one.
    fn from_owned_raw(
        ctx: <Self::Context as JSContextImpl>::RawContext,
        value: Self::RawValue,
    ) -> Self {
        ArkJSValue::new(ctx, value)
    }

    fn into_raw_value(self) -> Self::RawValue {
        let value = self.resolve_handle();
        // A resolved local handle remains valid for the active native callback;
        // release our persistent reference instead of leaking it.
        drop(self);
        value
    }

    /// The creation handle; valid only inside the native call that made the
    /// value. Use `raw_value_for_api` to pass the value to JSVM.
    fn as_raw_value(&self) -> &Self::RawValue {
        &self.value
    }

    fn raw_value_for_api(&self) -> Self::RawValue {
        self.resolve_handle()
    }

    fn as_raw_context(&self) -> &<Self::Context as JSContextImpl>::RawContext {
        &self.env
    }

    fn create_null(ctx: &Self::Context) -> Self {
        let _scope = crate::scope::OpScope::enter(ctx.to_raw());
        let env = ctx.to_raw();
        unsafe {
            let mut null_value: arkjs::JSVM_Value = ptr::null_mut();
            arkjs::OH_JSVM_GetNull(env, &mut null_value);
            Self::from_owned_raw(env, null_value)
        }
    }

    fn create_undefined(ctx: &Self::Context) -> Self {
        let _scope = crate::scope::OpScope::enter(ctx.to_raw());
        let env = ctx.to_raw();
        unsafe {
            let mut undefined_value: arkjs::JSVM_Value = ptr::null_mut();
            arkjs::OH_JSVM_GetUndefined(env, &mut undefined_value);
            Self::from_owned_raw(env, undefined_value)
        }
    }

    fn from_json_str(ctx: &Self::Context, str: &str) -> Self {
        let _scope = crate::scope::OpScope::enter(ctx.to_raw());
        let env = ctx.to_raw();
        unsafe {
            let mut json_string: arkjs::JSVM_Value = ptr::null_mut();
            let status = arkjs::OH_JSVM_CreateStringUtf8(
                env,
                str.as_ptr().cast(),
                str.len(),
                &mut json_string,
            );

            if status == arkjs::JSVM_Status_JSVM_OK {
                let mut result: arkjs::JSVM_Value = ptr::null_mut();
                let status = arkjs::OH_JSVM_JsonParse(env, json_string, &mut result);

                if status == arkjs::JSVM_Status_JSVM_OK {
                    Self::from_owned_raw(env, result)
                } else {
                    // JSON parse failed — retrieve and return the pending exception
                    // so the caller can handle it as an error.
                    let mut exception: arkjs::JSVM_Value = ptr::null_mut();
                    arkjs::OH_JSVM_GetAndClearLastException(env, &mut exception);
                    if !exception.is_null() {
                        ArkJSValue::from_owned_raw(env, exception).with_exception()
                    } else {
                        Self::create_undefined(ctx)
                    }
                }
            } else {
                Self::create_undefined(ctx)
            }
        }
    }

    fn create_symbol(ctx: &Self::Context, description: &str) -> Self {
        let _scope = crate::scope::OpScope::enter(ctx.to_raw());
        let env = ctx.to_raw();
        unsafe {
            // First create a string for the description
            let mut desc_string: arkjs::JSVM_Value = ptr::null_mut();
            let status = arkjs::OH_JSVM_CreateStringUtf8(
                env,
                description.as_ptr().cast(),
                description.len(),
                &mut desc_string,
            );

            if status == arkjs::JSVM_Status_JSVM_OK {
                let mut symbol_value: arkjs::JSVM_Value = ptr::null_mut();
                let status = arkjs::OH_JSVM_CreateSymbol(env, desc_string, &mut symbol_value);

                if status == arkjs::JSVM_Status_JSVM_OK {
                    Self::from_owned_raw(env, symbol_value)
                } else {
                    Self::create_undefined(ctx)
                }
            } else {
                Self::create_undefined(ctx)
            }
        }
    }

    fn create_date(ctx: &Self::Context, epoch_ms: f64) -> Self {
        let _scope = crate::scope::OpScope::enter(ctx.to_raw());
        let env = ctx.to_raw();
        unsafe {
            let mut date_value: arkjs::JSVM_Value = ptr::null_mut();
            let status = arkjs::OH_JSVM_CreateDate(env, epoch_ms, &mut date_value);
            if status == arkjs::JSVM_Status_JSVM_OK {
                Self::from_owned_raw(env, date_value).with_object()
            } else {
                Self::create_undefined(ctx)
            }
        }
    }
}

impl Clone for ArkJSValue {
    fn clone(&self) -> Self {
        let _scope = crate::scope::OpScope::enter(self.env);
        if self.primitive.is_some() {
            return ArkJSValue {
                value: self.value,
                env: self.env,
                value_type: self.value_type,
                reference: None,
                primitive: self.primitive,
                _context_guard: self._context_guard.clone(),
            };
        }
        // Each clone owns its own reference to the value the original reads now.
        let mut cloned = ArkJSValue::new(self.env, self.resolve_handle());
        cloned.value_type = self.value_type;
        cloned
    }
}

impl_js_converter!(
    ArkJSValue,
    bool,
    |ctx, value| unsafe {
        let mut bool_value: arkjs::JSVM_Value = ptr::null_mut();
        let status = arkjs::OH_JSVM_GetBoolean(ctx, value, &mut bool_value);
        if status == arkjs::JSVM_Status_JSVM_OK {
            bool_value
        } else {
            ptr::null_mut()
        }
    },
    |env, value, result: *mut bool| unsafe {
        let mut bool_result = false;
        let status = arkjs::OH_JSVM_GetValueBool(env, value, &mut bool_result);
        if status == arkjs::JSVM_Status_JSVM_OK {
            *result = bool_result;
            0
        } else {
            -1
        }
    }
);

impl_js_converter!(
    ArkJSValue,
    i32,
    |ctx, value| unsafe {
        let mut number_value: arkjs::JSVM_Value = ptr::null_mut();
        let status = arkjs::OH_JSVM_CreateInt32(ctx, value, &mut number_value);
        if status == arkjs::JSVM_Status_JSVM_OK {
            number_value
        } else {
            ptr::null_mut()
        }
    },
    |env, value, result: &mut i32| unsafe {
        let mut int_result = 0i32;
        let status = arkjs::OH_JSVM_GetValueInt32(env, value, &mut int_result);
        if status == arkjs::JSVM_Status_JSVM_OK {
            *result = int_result;
            0
        } else {
            -1
        }
    }
);

impl_js_converter!(
    ArkJSValue,
    f64,
    |ctx, value| unsafe {
        let mut number_value: arkjs::JSVM_Value = ptr::null_mut();
        let status = arkjs::OH_JSVM_CreateDouble(ctx, value, &mut number_value);
        if status == arkjs::JSVM_Status_JSVM_OK {
            number_value
        } else {
            ptr::null_mut()
        }
    },
    |env, value, result: &mut f64| unsafe {
        let mut double_result = 0.0f64;
        let status = arkjs::OH_JSVM_GetValueDouble(env, value, &mut double_result);
        if status == arkjs::JSVM_Status_JSVM_OK {
            *result = double_result;
            0
        } else {
            -1
        }
    }
);

impl_js_converter!(
    ArkJSValue,
    &str,
    String,
    |ctx, value: &str| unsafe {
        let mut string_value: arkjs::JSVM_Value = ptr::null_mut();
        let status = arkjs::OH_JSVM_CreateStringUtf8(
            ctx,
            value.as_ptr() as *const std::ffi::c_char,
            value.len(),
            &mut string_value,
        );
        if status == arkjs::JSVM_Status_JSVM_OK {
            string_value
        } else {
            ptr::null_mut()
        }
    },
    |env, value, result: *mut String| unsafe {
        // Coerce to string first so non-string values (numbers, booleans, etc.) work
        let mut string_value: arkjs::JSVM_Value = value;
        let mut value_type: arkjs::JSVM_ValueType = arkjs::JSVM_ValueType_JSVM_UNDEFINED;
        arkjs::OH_JSVM_Typeof(env, value, &mut value_type);
        if value_type != arkjs::JSVM_ValueType_JSVM_STRING {
            let status = arkjs::OH_JSVM_CoerceToString(env, value, &mut string_value);
            if status != arkjs::JSVM_Status_JSVM_OK {
                // `ToString` threw (a Symbol, a throwing `toString`): fail like the
                // other engines, and clear the exception so later JSVM calls work.
                let mut is_pending = false;
                arkjs::OH_JSVM_IsExceptionPending(env, &mut is_pending);
                if is_pending {
                    let mut exception: arkjs::JSVM_Value = ptr::null_mut();
                    arkjs::OH_JSVM_GetAndClearLastException(env, &mut exception);
                }
                return -1;
            }
        }

        let mut length: usize = 0;
        let status =
            arkjs::OH_JSVM_GetValueStringUtf8(env, string_value, ptr::null_mut(), 0, &mut length);

        if status == arkjs::JSVM_Status_JSVM_OK && length > 0 {
            let mut buffer = vec![0u8; length + 1];
            let mut written: usize = 0;
            let status = arkjs::OH_JSVM_GetValueStringUtf8(
                env,
                string_value,
                buffer.as_mut_ptr() as *mut std::ffi::c_char,
                buffer.len(),
                &mut written,
            );

            if status == arkjs::JSVM_Status_JSVM_OK {
                buffer.truncate(written);
                match String::from_utf8(buffer) {
                    Ok(s) => {
                        *result = s;
                        0
                    }
                    Err(_) => -1,
                }
            } else {
                -1
            }
        } else {
            *result = String::new();
            0
        }
    }
);

impl_js_converter!(
    ArkJSValue,
    u32,
    |ctx, value| unsafe {
        let mut number_value: arkjs::JSVM_Value = ptr::null_mut();
        let status = arkjs::OH_JSVM_CreateUint32(ctx, value, &mut number_value);
        if status == arkjs::JSVM_Status_JSVM_OK {
            number_value
        } else {
            ptr::null_mut()
        }
    },
    |env, value, result: &mut u32| unsafe {
        let mut uint_result = 0u32;
        let status = arkjs::OH_JSVM_GetValueUint32(env, value, &mut uint_result);
        if status == arkjs::JSVM_Status_JSVM_OK {
            *result = uint_result;
            0
        } else {
            -1
        }
    }
);

impl_js_converter!(
    ArkJSValue,
    i64,
    |ctx, value| unsafe {
        // For values within JavaScript safe integer range, use regular number
        // JavaScript safe integer range: -(2^53 - 1) to (2^53 - 1)
        const JS_MAX_SAFE_INTEGER: i64 = (1i64 << 53) - 1;
        const JS_MIN_SAFE_INTEGER: i64 = -JS_MAX_SAFE_INTEGER;

        if (JS_MIN_SAFE_INTEGER..=JS_MAX_SAFE_INTEGER).contains(&value) {
            let mut result: arkjs::JSVM_Value = ptr::null_mut();
            let status = arkjs::OH_JSVM_CreateDouble(ctx, value as f64, &mut result);
            if status == arkjs::JSVM_Status_JSVM_OK {
                result
            } else {
                ptr::null_mut()
            }
        } else {
            let mut bigint_value: arkjs::JSVM_Value = ptr::null_mut();
            let status = arkjs::OH_JSVM_CreateBigintInt64(ctx, value, &mut bigint_value);
            if status == arkjs::JSVM_Status_JSVM_OK {
                bigint_value
            } else {
                ptr::null_mut()
            }
        }
    },
    |env, value, result: &mut i64| unsafe {
        let mut int64_result = 0i64;
        let mut lossless = false;
        let status =
            arkjs::OH_JSVM_GetValueBigintInt64(env, value, &mut int64_result, &mut lossless);
        if status == arkjs::JSVM_Status_JSVM_OK {
            *result = int64_result;
            0
        } else {
            // Fallback to number conversion
            let mut double_result = 0.0f64;
            let status = arkjs::OH_JSVM_GetValueDouble(env, value, &mut double_result);
            if status == arkjs::JSVM_Status_JSVM_OK {
                *result = double_result as i64;
                0
            } else {
                -1
            }
        }
    }
);

impl_js_converter!(
    ArkJSValue,
    u64,
    |ctx, value| unsafe {
        // For values within JavaScript safe integer range, use regular number
        // JavaScript safe integer range: 0 to (2^53 - 1) for unsigned
        const JS_MAX_SAFE_INTEGER: u64 = (1u64 << 53) - 1;

        if value <= JS_MAX_SAFE_INTEGER {
            let mut result: arkjs::JSVM_Value = ptr::null_mut();
            let status = arkjs::OH_JSVM_CreateDouble(ctx, value as f64, &mut result);
            if status == arkjs::JSVM_Status_JSVM_OK {
                result
            } else {
                ptr::null_mut()
            }
        } else {
            let mut bigint_value: arkjs::JSVM_Value = ptr::null_mut();
            let status = arkjs::OH_JSVM_CreateBigintUint64(ctx, value, &mut bigint_value);
            if status == arkjs::JSVM_Status_JSVM_OK {
                bigint_value
            } else {
                ptr::null_mut()
            }
        }
    },
    |env, value, result: &mut u64| unsafe {
        let mut uint64_result = 0u64;
        let mut lossless = false;
        let status =
            arkjs::OH_JSVM_GetValueBigintUint64(env, value, &mut uint64_result, &mut lossless);
        if status == arkjs::JSVM_Status_JSVM_OK {
            *result = uint64_result;
            0
        } else {
            // Fallback to number conversion
            let mut double_result = 0.0f64;
            let status = arkjs::OH_JSVM_GetValueDouble(env, value, &mut double_result);
            if status == arkjs::JSVM_Status_JSVM_OK {
                *result = double_result as u64;
                0
            } else {
                -1
            }
        }
    }
);
