use crate::{ArkJSContext, arkjs};
use rong_core::{
    JSContextImpl, JSErrorFactory, JSRawContext, JSTypeOf, JSValueImpl, RongJSError,
    impl_js_converter,
};
use std::hash::Hash;
use std::ptr;

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

pub struct ArkJSValue {
    value: arkjs::JSVM_Value,
    env: arkjs::JSVM_Env,
    value_type: ArkJSValueType,
    reference: Option<arkjs::JSVM_Ref>, // Track reference for proper cleanup
}

impl PartialEq for ArkJSValue {
    fn eq(&self, other: &Self) -> bool {
        unsafe {
            let lhs = self.resolve_handle();
            let rhs = other.resolve_handle();
            let mut result = false;
            let status = arkjs::OH_JSVM_StrictEquals(self.env, lhs, rhs, &mut result);
            status == arkjs::JSVM_Status_JSVM_OK && result
        }
    }
}

impl Hash for ArkJSValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (self.value as usize).hash(state);
        (self.env as usize).hash(state);
        self.value_type.hash(state);
    }
}

impl ArkJSValue {
    pub(crate) fn new(env: arkjs::JSVM_Env, value: arkjs::JSVM_Value) -> Self {
        Self {
            env,
            value,
            value_type: ArkJSValueType::Other,
            reference: None,
        }
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
                return Self::from_owned_raw(self.env, exception)
                    .protect()
                    .with_exception();
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

    /// Protects the current value from garbage collection by creating a reference
    pub(crate) fn protect(mut self) -> Self {
        unsafe {
            let mut ref_value: arkjs::JSVM_Ref = ptr::null_mut();
            let status = arkjs::OH_JSVM_CreateReference(self.env, self.value, 1, &mut ref_value);
            if status == arkjs::JSVM_Status_JSVM_OK {
                self.reference = Some(ref_value);
            }
        }
        self
    }

    /// Returns a fresh local handle for this value. If the value has a persistent
    /// reference, resolves it via OH_JSVM_GetReferenceValue to get a handle that
    /// is guaranteed valid in the current scope (important for async boundaries).
    /// Falls back to the stored local handle if no reference exists.
    pub(crate) fn resolve_handle(&self) -> arkjs::JSVM_Value {
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

    fn from_borrowed_raw(
        ctx: <Self::Context as JSContextImpl>::RawContext,
        value: Self::RawValue,
    ) -> Self {
        ArkJSValue::new(ctx, value).protect()
    }

    fn from_owned_raw(
        ctx: <Self::Context as JSContextImpl>::RawContext,
        value: Self::RawValue,
    ) -> Self {
        ArkJSValue::new(ctx, value)
    }

    fn into_raw_value(self) -> Self::RawValue {
        let value = self.resolve_handle();
        std::mem::forget(self); // Prevent drop from being called
        value
    }

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
        let env = ctx.to_raw();
        unsafe {
            let mut null_value: arkjs::JSVM_Value = ptr::null_mut();
            arkjs::OH_JSVM_GetNull(env, &mut null_value);
            Self::from_owned_raw(env, null_value)
        }
    }

    fn create_undefined(ctx: &Self::Context) -> Self {
        let env = ctx.to_raw();
        unsafe {
            let mut undefined_value: arkjs::JSVM_Value = ptr::null_mut();
            arkjs::OH_JSVM_GetUndefined(env, &mut undefined_value);
            Self::from_owned_raw(env, undefined_value)
        }
    }

    fn from_json_str(ctx: &Self::Context, str: &str) -> Self {
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
                        ArkJSValue::from_owned_raw(env, exception)
                            .protect()
                            .with_exception()
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
        if let Some(reference) = self.reference {
            // The original has a persistent reference. Resolve it to get a
            // fresh local handle for creating the new reference, but keep
            // self.value as the stored value (CLASS map keys depend on stable
            // handle identity).
            unsafe {
                let mut fresh: arkjs::JSVM_Value = ptr::null_mut();
                let status = arkjs::OH_JSVM_GetReferenceValue(self.env, reference, &mut fresh);
                if status == arkjs::JSVM_Status_JSVM_OK && !fresh.is_null() {
                    let mut ref_value: arkjs::JSVM_Ref = ptr::null_mut();
                    let status = arkjs::OH_JSVM_CreateReference(self.env, fresh, 1, &mut ref_value);
                    if status == arkjs::JSVM_Status_JSVM_OK {
                        return ArkJSValue {
                            env: self.env,
                            value: self.value, // keep original handle identity
                            reference: Some(ref_value),
                            value_type: self.value_type,
                        };
                    }
                }
            }
        }
        // Fallback: no reference or resolve failed
        let mut cloned = ArkJSValue::new(self.env, self.value);
        cloned.value_type = self.value_type;
        cloned.protect()
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
                *result = String::new();
                return 0;
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
