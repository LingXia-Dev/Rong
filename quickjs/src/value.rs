use crate::runtime::{QJSRuntimeInner, runtime_guard_from_ctx};
use crate::{QJSContext, qjs};
use rong_core::{
    JSContextImpl, JSRawContext, JSTypeOf, JSValueImpl, RongJSError, impl_js_converter,
};
use std::hash::Hash;
use std::rc::Rc;
use std::slice;

mod array;
mod array_buffer;
mod object;
mod proxy;
mod typed_array;
mod valuetype;

pub struct QJSValue {
    value: qjs::JSValue,
    ctx: *mut qjs::JSContext,
    // Keep the owning runtime alive and free values via JS_FreeValueRT.
    // This avoids shutdown-time crashes where a value outlives its originating context.
    rt_guard: Option<Rc<QJSRuntimeInner>>,
    value_type: QJSValueType,
}

unsafe fn dup_value(
    ctx: *mut qjs::JSContext,
    rt_guard: &Option<Rc<QJSRuntimeInner>>,
    value: qjs::JSValue,
) -> qjs::JSValue {
    if let Some(rt) = rt_guard {
        unsafe { qjs::JS_DupValueRT(rt.rt, value) }
    } else if !ctx.is_null() {
        unsafe { qjs::JS_DupValue(ctx, value) }
    } else {
        value
    }
}

unsafe fn free_value(
    ctx: *mut qjs::JSContext,
    rt_guard: &Option<Rc<QJSRuntimeInner>>,
    value: qjs::JSValue,
) {
    if let Some(rt) = rt_guard {
        unsafe { qjs::JS_FreeValueRT(rt.rt, value) }
    } else if !ctx.is_null() {
        unsafe { qjs::JS_FreeValue(ctx, value) }
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub(crate) enum QJSValueType {
    Error,
    Exception,
    Other,
}

impl PartialEq for QJSValue {
    fn eq(&self, other: &Self) -> bool {
        if self.ctx != other.ctx {
            return false;
        }

        // Identity semantics without reading uninitialized union/padding bytes.
        unsafe {
            qjs::QJS_ValueIdentTag(self.value) == qjs::QJS_ValueIdentTag(other.value)
                && qjs::QJS_ValueIdentPayload(self.value) == qjs::QJS_ValueIdentPayload(other.value)
        }
    }
}

impl Hash for QJSValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        unsafe {
            qjs::QJS_ValueIdentTag(self.value).hash(state);
            qjs::QJS_ValueIdentPayload(self.value).hash(state);
        }
        self.ctx.hash(state);
    }
}

impl Clone for QJSValue {
    fn clone(&self) -> Self {
        let value = unsafe { dup_value(self.ctx, &self.rt_guard, self.value) };
        Self {
            value,
            ctx: self.ctx,
            rt_guard: self.rt_guard.clone(),
            value_type: self.value_type,
        }
    }
}

impl Drop for QJSValue {
    fn drop(&mut self) {
        unsafe {
            // Finalizer callback: ctx (and rt_guard) can be NULL/None. In that case, do nothing.
            free_value(self.ctx, &self.rt_guard, self.value);
        }
    }
}

impl JSRawContext for QJSValue {
    type RawContext = *mut qjs::JSContext;
}

impl QJSValue {
    fn _from_raw(ctx: *mut qjs::JSContext, value: qjs::JSValue) -> Self {
        // In callback context, generally, all JS variables are from JS engine, in order to make Rust lifetime
        // and ownship works, these variables should be increased referece count first, and then Rust side can
        // drop QJSValue safely
        let rt_guard = runtime_guard_from_ctx(ctx);
        let value = unsafe { dup_value(ctx, &rt_guard, value) };

        Self {
            value,
            ctx,
            rt_guard,
            value_type: QJSValueType::Other,
        }
    }

    pub(crate) fn new(ctx: *mut qjs::JSContext, value: qjs::JSValue) -> Self {
        let rt_guard = runtime_guard_from_ctx(ctx);
        Self {
            value,
            ctx,
            rt_guard,
            value_type: QJSValueType::Other,
        }
    }

    // In callback context, generally, all JS variables are from JS engine, in order to make Rust lifetime
    // and ownship works, these variables should be increased referece count first, and then Rust side can
    // drop QJSValue safely
    pub(crate) fn protect(mut self) -> Self {
        self.value = unsafe { dup_value(self.ctx, &self.rt_guard, self.value) };
        self
    }

    pub(crate) fn with_exception(mut self) -> Self {
        self.value_type = QJSValueType::Exception;
        self
    }

    pub(crate) fn with_error(mut self) -> Self {
        self.value_type = QJSValueType::Error;
        self
    }

    pub(crate) fn _is_err(&self) -> bool {
        self.value_type == QJSValueType::Error
    }

    pub(crate) fn _is_exception(&self) -> bool {
        self.value_type == QJSValueType::Exception
    }
}

impl JSValueImpl for QJSValue {
    type RawValue = qjs::JSValue;
    type Context = QJSContext;

    fn from_borrowed_raw(
        ctx: <Self::Context as JSContextImpl>::RawContext,
        value: Self::RawValue,
    ) -> Self {
        QJSValue::new(ctx, value).protect()
    }

    fn from_owned_raw(
        ctx: <Self::Context as JSContextImpl>::RawContext,
        value: Self::RawValue,
    ) -> Self {
        QJSValue::new(ctx, value)
    }

    fn into_raw_value(self) -> Self::RawValue {
        let value = self.value;
        std::mem::forget(self); // forbiden triggering drop
        value
    }

    fn as_raw_value(&self) -> &Self::RawValue {
        &self.value
    }

    fn as_raw_context(&self) -> &<Self::Context as JSContextImpl>::RawContext {
        &self.ctx
    }

    fn create_null(ctx: &Self::Context) -> Self {
        let ctx = ctx.to_raw();
        let raw = unsafe { qjs::QJS_NewNull(ctx) };
        Self::from_owned_raw(ctx, raw)
    }

    fn create_undefined(ctx: &Self::Context) -> Self {
        let ctx = ctx.to_raw();
        let raw = unsafe { qjs::QJS_NewUndefined(ctx) };
        Self::from_owned_raw(ctx, raw)
    }

    fn from_json_str(ctx: &Self::Context, str: &str) -> Self {
        // Parse JSON string into JS value
        let mut json = str.as_bytes().to_vec();
        json.push(0);
        let raw = unsafe {
            qjs::JS_ParseJSON(
                ctx.to_raw(),
                json.as_ptr().cast(),
                str.len(),
                c"JSON".as_ptr(),
            )
        };
        ctx.to_owned_value(raw)
    }

    fn create_symbol(ctx: &Self::Context, description: &str) -> Self {
        let ctx_ptr = ctx.to_raw();
        let raw = unsafe {
            let global = qjs::JS_GetGlobalObject(ctx_ptr);
            let symbol = qjs::JS_GetPropertyStr(ctx_ptr, global, c"Symbol".as_ptr());
            let description =
                qjs::JS_NewStringLen(ctx_ptr, description.as_ptr().cast(), description.len());
            let mut args = [description];
            let value = qjs::JS_Call(
                ctx_ptr,
                symbol,
                qjs::QJS_NewUndefined(ctx_ptr),
                1,
                args.as_mut_ptr(),
            );
            qjs::JS_FreeValue(ctx_ptr, description);
            qjs::JS_FreeValue(ctx_ptr, symbol);
            qjs::JS_FreeValue(ctx_ptr, global);
            value
        };
        ctx.to_owned_value(raw)
    }

    fn create_date(ctx: &Self::Context, epoch_ms: f64) -> Self {
        let raw = unsafe { qjs::JS_NewDate(ctx.to_raw(), epoch_ms) };
        ctx.to_owned_value(raw)
    }
}

impl QJSValue {
    // it's for debug only
    #[allow(dead_code)]
    pub fn get_ref_count(value: qjs::JSValue) -> i32 {
        unsafe { qjs::QJS_GetRefCount(value) }
    }
}

impl_js_converter!(
    QJSValue,
    bool,
    |ctx, value| qjs::QJS_NewBool(ctx, value),
    |ctx, value, result: *mut bool| {
        if qjs::QJS_IsBool(ctx, value) {
            let status = qjs::JS_ToBool(ctx, value);
            *result = status != 0;
            0
        } else {
            -1
        }
    }
);

impl_js_converter!(
    QJSValue,
    i32,
    |ctx, value| { qjs::QJS_NewInt32(ctx, value) },
    |ctx, value, result| {
        // not number
        if !qjs::QJS_IsNumber(ctx, value) {
            return -1;
        }

        qjs::JS_ToInt32(ctx, result, value)
    }
);

impl_js_converter!(
    QJSValue,
    u32,
    |ctx, value| { qjs::QJS_NewUint32(ctx, value) },
    |ctx, value, result| {
        // not number
        if !qjs::QJS_IsNumber(ctx, value) {
            return -1;
        }

        qjs::QJS_ToUint32(ctx, result, value)
    }
);

impl_js_converter!(
    QJSValue,
    i64,
    |ctx, value| {
        // For values within JavaScript safe integer range, use regular number
        // JavaScript safe integer range: -(2^53 - 1) to (2^53 - 1)
        const JS_MAX_SAFE_INTEGER: i64 = (1i64 << 53) - 1;
        const JS_MIN_SAFE_INTEGER: i64 = -JS_MAX_SAFE_INTEGER;

        if (JS_MIN_SAFE_INTEGER..=JS_MAX_SAFE_INTEGER).contains(&value) {
            qjs::QJS_NewFloat64(ctx, value as f64)
        } else {
            qjs::JS_NewBigInt64(ctx, value)
        }
    },
    |ctx, value, result: &mut i64| {
        if qjs::QJS_IsBigInt(ctx, value) {
            // It's a BigInt, extract as i64
            qjs::JS_ToBigInt64(ctx, result, value)
        } else if qjs::QJS_IsNumber(ctx, value) {
            // It's a regular number, convert to f64 first then to i64
            let mut temp: f64 = 0.0;
            let ret = qjs::JS_ToFloat64(ctx, &mut temp, value);
            *result = temp as i64;
            ret
        } else {
            // Not a number or BigInt
            -1
        }
    }
);

impl_js_converter!(
    QJSValue,
    u64,
    |ctx, value| {
        // For values within JavaScript safe integer range, use regular number
        // JavaScript safe integer range: 0 to (2^53 - 1) for unsigned
        const JS_MAX_SAFE_INTEGER: u64 = (1u64 << 53) - 1;

        if value <= JS_MAX_SAFE_INTEGER {
            qjs::QJS_NewFloat64(ctx, value as f64)
        } else {
            qjs::JS_NewBigUint64(ctx, value)
        }
    },
    |ctx, value, result| {
        if qjs::QJS_IsBigInt(ctx, value) {
            // It's a BigInt, extract as u64
            qjs::JS_ToBigUint64(ctx, result, value)
        } else if qjs::QJS_IsNumber(ctx, value) {
            // It's a regular number, convert to f64 first then to u64
            let mut temp: f64 = 0.0;
            let ret = qjs::JS_ToFloat64(ctx, &mut temp, value);
            if temp >= 0.0 {
                *result = temp as u64;
                ret
            } else {
                -1 // Negative numbers can't be converted to u64
            }
        } else {
            // Not a number or BigInt
            -1
        }
    }
);

impl_js_converter!(
    QJSValue,
    f64,
    |ctx, value| qjs::QJS_NewFloat64(ctx, value),
    |ctx, value, result| {
        // not number
        if !qjs::QJS_IsNumber(ctx, value) {
            return -1;
        }
        qjs::JS_ToFloat64(ctx, result, value)
    }
);

impl_js_converter!(
    QJSValue,
    &str,
    String,
    |ctx, value: &str| {
        let len = value.len();
        qjs::JS_NewStringLen(ctx, value.as_ptr() as _, len as _)
    },
    |ctx, value, result: *mut String| {
        // This intentionally skips QJS_IsString check to allow any value that
        // the JS engine can convert to a string, providing more flexible type coercion
        let mut len: usize = 0;
        let ptr = qjs::JS_ToCStringLen2(ctx, &mut len as _, value, false);
        if ptr.is_null() {
            return -1;
        }

        // Use from_raw_parts to get the complete string including null characters
        let slice = slice::from_raw_parts(ptr as *const u8, len);
        *result = String::from_utf8_lossy(slice).into_owned();

        qjs::JS_FreeCString(ctx, ptr);
        0
    }
);
