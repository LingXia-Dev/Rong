use crate::{JSCContext, jsc};
use rong_core::{
    JSContextImpl, JSRawContext, JSTypeOf, JSValueImpl, RongJSError, impl_js_converter,
};
use std::ffi::CString;
use std::hash::Hash;
use std::ptr;
#[cfg(target_os = "macos")]
use std::sync::OnceLock;

#[cfg(target_os = "macos")]
unsafe fn dlsym(name: &'static [u8]) -> *mut std::ffi::c_void {
    debug_assert!(name.last() == Some(&0));
    unsafe extern "C" {
        fn dlsym(
            handle: *mut std::ffi::c_void,
            symbol: *const std::ffi::c_char,
        ) -> *mut std::ffi::c_void;
    }
    // `RTLD_DEFAULT` is `((void *) -2)` on macOS.
    let handle = (-2isize) as *mut std::ffi::c_void;
    unsafe { dlsym(handle, name.as_ptr().cast()) }
}

#[cfg(target_os = "macos")]
unsafe fn dlsym_fn<T: Copy>(name: &'static [u8]) -> Option<T> {
    let sym = unsafe { dlsym(name) };
    if sym.is_null() {
        None
    } else {
        Some(unsafe { std::mem::transmute_copy::<*mut std::ffi::c_void, T>(&sym) })
    }
}

#[cfg(target_os = "macos")]
type JSIsBigIntFn = unsafe extern "C" fn(*mut jsc::OpaqueJSContext, jsc::JSValueRef) -> bool;
#[cfg(target_os = "macos")]
type JSBigIntCreateI64Fn =
    unsafe extern "C" fn(*mut jsc::OpaqueJSContext, i64, *mut jsc::JSValueRef) -> jsc::JSValueRef;
#[cfg(target_os = "macos")]
type JSBigIntCreateU64Fn =
    unsafe extern "C" fn(*mut jsc::OpaqueJSContext, u64, *mut jsc::JSValueRef) -> jsc::JSValueRef;
#[cfg(target_os = "macos")]
type JSToI64Fn =
    unsafe extern "C" fn(*mut jsc::OpaqueJSContext, jsc::JSValueRef, *mut jsc::JSValueRef) -> i64;
#[cfg(target_os = "macos")]
type JSToU64Fn =
    unsafe extern "C" fn(*mut jsc::OpaqueJSContext, jsc::JSValueRef, *mut jsc::JSValueRef) -> u64;

#[cfg(target_os = "macos")]
fn jsvalue_is_bigint_sym() -> Option<JSIsBigIntFn> {
    static SYM: OnceLock<Option<JSIsBigIntFn>> = OnceLock::new();
    *SYM.get_or_init(|| unsafe { dlsym_fn(b"JSValueIsBigInt\0") })
}

#[cfg(target_os = "macos")]
fn jsbigint_create_i64_sym() -> Option<JSBigIntCreateI64Fn> {
    static SYM: OnceLock<Option<JSBigIntCreateI64Fn>> = OnceLock::new();
    *SYM.get_or_init(|| unsafe { dlsym_fn(b"JSBigIntCreateWithInt64\0") })
}

#[cfg(target_os = "macos")]
fn jsbigint_create_u64_sym() -> Option<JSBigIntCreateU64Fn> {
    static SYM: OnceLock<Option<JSBigIntCreateU64Fn>> = OnceLock::new();
    *SYM.get_or_init(|| unsafe { dlsym_fn(b"JSBigIntCreateWithUInt64\0") })
}

#[cfg(target_os = "macos")]
fn jsvalue_to_i64_sym() -> Option<JSToI64Fn> {
    static SYM: OnceLock<Option<JSToI64Fn>> = OnceLock::new();
    *SYM.get_or_init(|| unsafe { dlsym_fn(b"JSValueToInt64\0") })
}

#[cfg(target_os = "macos")]
fn jsvalue_to_u64_sym() -> Option<JSToU64Fn> {
    static SYM: OnceLock<Option<JSToU64Fn>> = OnceLock::new();
    *SYM.get_or_init(|| unsafe { dlsym_fn(b"JSValueToUInt64\0") })
}

/// Create a BigInt from a decimal string by evaluating JS (works on older macOS).
unsafe fn create_bigint_from_str(ctx: *const jsc::OpaqueJSContext, value: &str) -> jsc::JSValueRef {
    let ctx = ctx as *mut jsc::OpaqueJSContext;
    let code = format!("BigInt(\"{}\")", value);
    let c_code = CString::new(code).unwrap();
    let js_str = unsafe { jsc::JSStringCreateWithUTF8CString(c_code.as_ptr()) };
    let mut exception: jsc::JSValueRef = ptr::null_mut();
    let result = unsafe {
        jsc::JSEvaluateScript(
            ctx,
            js_str,
            ptr::null_mut(),
            ptr::null_mut(),
            1,
            &mut exception,
        )
    };
    unsafe { jsc::JSStringRelease(js_str) };
    if exception.is_null() {
        result
    } else {
        unsafe { jsc::JSValueMakeUndefined(ctx) }
    }
}

/// Check if a value is BigInt. Uses native BigInt APIs when available; falls back to JS typeof.
pub(crate) unsafe fn jsvalue_is_bigint(
    ctx: *const jsc::OpaqueJSContext,
    value: jsc::JSValueRef,
) -> bool {
    let ctx = ctx as *mut jsc::OpaqueJSContext;
    #[cfg(target_os = "ios")]
    {
        return unsafe { jsc::JSValueIsBigInt(ctx, value) };
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(f) = jsvalue_is_bigint_sym() {
            return unsafe { f(ctx, value) };
        }
    }

    #[cfg(not(target_os = "ios"))]
    {
        // Use JS typeof to check if it's a bigint (works everywhere).
        let code = c"(function(v) { return typeof v === 'bigint'; })";
        let js_str = unsafe { jsc::JSStringCreateWithUTF8CString(code.as_ptr()) };
        let mut exception: jsc::JSValueRef = ptr::null_mut();
        let func = unsafe {
            jsc::JSEvaluateScript(
                ctx,
                js_str,
                ptr::null_mut(),
                ptr::null_mut(),
                1,
                &mut exception,
            )
        };
        unsafe { jsc::JSStringRelease(js_str) };

        if exception.is_null() && unsafe { jsc::JSValueIsObject(ctx, func) } {
            let func_obj = unsafe { jsc::JSValueToObject(ctx, func, ptr::null_mut()) };
            let args = [value];
            let result = unsafe {
                jsc::JSObjectCallAsFunction(
                    ctx,
                    func_obj,
                    ptr::null_mut(),
                    1,
                    args.as_ptr(),
                    &mut exception,
                )
            };
            if exception.is_null() {
                return unsafe { jsc::JSValueToBoolean(ctx, result) };
            }
        }
    }

    // Default fallback or if something failed
    false
}

/// Convert BigInt to string using JS code (works everywhere).
unsafe fn bigint_to_string(
    ctx: *const jsc::OpaqueJSContext,
    value: jsc::JSValueRef,
) -> Option<String> {
    let ctx = ctx as *mut jsc::OpaqueJSContext;
    let code = c"(function(v) { return v.toString(); })";
    let js_str = unsafe { jsc::JSStringCreateWithUTF8CString(code.as_ptr()) };
    let mut exception: jsc::JSValueRef = ptr::null_mut();
    let func = unsafe {
        jsc::JSEvaluateScript(
            ctx,
            js_str,
            ptr::null_mut(),
            ptr::null_mut(),
            1,
            &mut exception,
        )
    };
    unsafe { jsc::JSStringRelease(js_str) };

    if exception.is_null() && unsafe { jsc::JSValueIsObject(ctx, func) } {
        let func_obj = unsafe { jsc::JSValueToObject(ctx, func, ptr::null_mut()) };
        let args = [value];
        let result = unsafe {
            jsc::JSObjectCallAsFunction(
                ctx,
                func_obj,
                ptr::null_mut(),
                1,
                args.as_ptr(),
                &mut exception,
            )
        };
        if exception.is_null() && unsafe { jsc::JSValueIsString(ctx, result) } {
            let js_string = unsafe { jsc::JSValueToStringCopy(ctx, result, ptr::null_mut()) };
            let len = unsafe { jsc::JSStringGetMaximumUTF8CStringSize(js_string) };
            let mut buffer = vec![0u8; len];
            unsafe {
                jsc::JSStringGetUTF8CString(js_string, buffer.as_mut_ptr() as *mut i8, len);
            }
            unsafe { jsc::JSStringRelease(js_string) };
            let cstr = unsafe { std::ffi::CStr::from_ptr(buffer.as_ptr() as *const i8) };
            return cstr.to_str().ok().map(|s| s.to_string());
        }
    }
    None
}

unsafe fn bigint_from_i64(ctx: *const jsc::OpaqueJSContext, value: i64) -> jsc::JSValueRef {
    let ctx_mut = ctx as *mut jsc::OpaqueJSContext;
    #[cfg(target_os = "ios")]
    {
        return unsafe { jsc::JSBigIntCreateWithInt64(ctx_mut, value, ptr::null_mut()) };
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(f) = jsbigint_create_i64_sym() {
            return unsafe { f(ctx_mut, value, ptr::null_mut()) };
        }
    }

    #[cfg(not(target_os = "ios"))]
    unsafe {
        create_bigint_from_str(ctx, &value.to_string())
    }

    #[cfg(target_os = "ios")]
    {
        // This path should be unreachable on iOS due to the specific API check above,
        // but if we fall through, we return undefined or similar to avoid UB?
        // Actually, the iOS block above returns, so this is logically unreachable.
        // But to satisfy the compiler:
        unreachable!("iOS should have returned early")
    }
}

unsafe fn bigint_from_u64(ctx: *const jsc::OpaqueJSContext, value: u64) -> jsc::JSValueRef {
    let ctx_mut = ctx as *mut jsc::OpaqueJSContext;
    #[cfg(target_os = "ios")]
    {
        return unsafe { jsc::JSBigIntCreateWithUInt64(ctx_mut, value, ptr::null_mut()) };
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(f) = jsbigint_create_u64_sym() {
            return unsafe { f(ctx_mut, value, ptr::null_mut()) };
        }
    }

    #[cfg(not(target_os = "ios"))]
    unsafe {
        create_bigint_from_str(ctx, &value.to_string())
    }

    #[cfg(target_os = "ios")]
    unreachable!("iOS should have returned early")
}

unsafe fn bigint_to_i64(ctx: *const jsc::OpaqueJSContext, value: jsc::JSValueRef) -> Option<i64> {
    let ctx_mut = ctx as *mut jsc::OpaqueJSContext;
    #[cfg(target_os = "ios")]
    {
        let mut exception: jsc::JSValueRef = ptr::null_mut();
        let v = unsafe { jsc::JSValueToInt64(ctx_mut, value, &mut exception) };
        return if exception.is_null() { Some(v) } else { None };
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(f) = jsvalue_to_i64_sym() {
            let mut exception: jsc::JSValueRef = ptr::null_mut();
            let v = unsafe { f(ctx_mut, value, &mut exception) };
            return if exception.is_null() { Some(v) } else { None };
        }
    }

    #[cfg(not(target_os = "ios"))]
    unsafe {
        bigint_to_string(ctx, value)?.parse::<i64>().ok()
    }

    #[cfg(target_os = "ios")]
    {
        None
    }
}

unsafe fn bigint_to_u64(ctx: *const jsc::OpaqueJSContext, value: jsc::JSValueRef) -> Option<u64> {
    let ctx_mut = ctx as *mut jsc::OpaqueJSContext;
    #[cfg(target_os = "ios")]
    {
        let mut exception: jsc::JSValueRef = ptr::null_mut();
        let v = unsafe { jsc::JSValueToUInt64(ctx_mut, value, &mut exception) };
        return if exception.is_null() { Some(v) } else { None };
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(f) = jsvalue_to_u64_sym() {
            let mut exception: jsc::JSValueRef = ptr::null_mut();
            let v = unsafe { f(ctx_mut, value, &mut exception) };
            return if exception.is_null() { Some(v) } else { None };
        }
    }

    #[cfg(not(target_os = "ios"))]
    unsafe {
        bigint_to_string(ctx, value)?.parse::<u64>().ok()
    }

    #[cfg(target_os = "ios")]
    {
        None
    }
}

mod array;
mod array_buffer;
mod object;
mod typed_array;
mod valuetype;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub(crate) enum JSCValueType {
    Object,
    Error,
    Exception,
    Other,
}

pub struct JSCValue {
    value: *const jsc::OpaqueJSValue,
    ctx: *mut jsc::OpaqueJSContext,
    value_type: JSCValueType,
    protected: bool,
}

impl PartialEq for JSCValue {
    fn eq(&self, other: &Self) -> bool {
        // Identity semantics: same context pointer + same JSValueRef pointer.
        self.ctx == other.ctx && self.value == other.value
    }
}

impl Hash for JSCValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.value.hash(state);
        self.ctx.hash(state);
    }
}

impl JSCValue {
    pub(crate) fn new(ctx: *mut jsc::OpaqueJSContext, value: *const jsc::OpaqueJSValue) -> Self {
        Self {
            ctx,
            value,
            value_type: JSCValueType::Other,
            protected: false,
        }
    }

    pub(crate) fn as_obj(&self) -> jsc::JSObjectRef {
        if self.value_type == JSCValueType::Object {
            self.value as jsc::JSObjectRef
        } else {
            let mut exception: jsc::JSValueRef = std::ptr::null_mut();
            unsafe { jsc::JSValueToObject(self.ctx, self.value, &mut exception) }
        }
    }

    pub(crate) fn as_value(&self) -> jsc::JSValueRef {
        self.value
    }

    /// Protects the current value from garbage collection.
    pub(crate) fn protect(mut self) -> Self {
        if self.protected || self.ctx.is_null() || self.value.is_null() {
            return self;
        }
        unsafe {
            jsc::JSValueProtect(self.ctx, self.value);
        }
        self.protected = true;
        self
    }

    pub(crate) fn with_exception(mut self) -> Self {
        self.value_type = JSCValueType::Exception;
        self
    }

    pub(crate) fn with_object(mut self) -> Self {
        self.value_type = JSCValueType::Object;
        self
    }

    pub(crate) fn with_error(mut self) -> Self {
        self.value_type = JSCValueType::Error;
        self
    }

    pub(crate) fn _is_err(&self) -> bool {
        self.value_type == JSCValueType::Error
    }

    pub(crate) fn _is_exception(&self) -> bool {
        self.value_type == JSCValueType::Exception
    }

    pub(crate) fn _is_object(&self) -> bool {
        self.value_type == JSCValueType::Object
    }

    pub(crate) fn from_borrowed_obj(ctx: *mut jsc::OpaqueJSContext, obj: jsc::JSObjectRef) -> Self {
        JSCValue::new(ctx, obj).with_object().protect()
    }

    pub(crate) fn from_owned_obj(ctx: *mut jsc::OpaqueJSContext, obj: jsc::JSObjectRef) -> Self {
        JSCValue::new(ctx, obj).with_object().protect()
    }
}

impl Drop for JSCValue {
    fn drop(&mut self) {
        // Finalizer callback, ctx is set to NULL
        if self.protected && !self.ctx.is_null() {
            unsafe {
                jsc::JSValueUnprotect(self.ctx, self.value);
            }
        }
    }
}

impl JSRawContext for JSCValue {
    type RawContext = *mut jsc::OpaqueJSContext;
}

impl JSValueImpl for JSCValue {
    type RawValue = *const jsc::OpaqueJSValue;
    type Context = JSCContext;

    fn from_borrowed_raw(
        ctx: <Self::Context as JSContextImpl>::RawContext,
        value: Self::RawValue,
    ) -> Self {
        JSCValue::new(ctx, value).protect()
    }

    fn from_owned_raw(
        ctx: <Self::Context as JSContextImpl>::RawContext,
        value: Self::RawValue,
    ) -> Self {
        JSCValue::new(ctx, value).protect()
    }

    fn into_raw_value(self) -> Self::RawValue {
        let mut this = self;

        // `into_raw_value` transfers ownership to the engine.
        // If we protected this value, balance it first so we don't leak protections.
        if this.protected && !this.ctx.is_null() && !this.value.is_null() {
            unsafe {
                jsc::JSValueUnprotect(this.ctx, this.value);
            }
            this.protected = false;
        }

        let value = this.value;
        std::mem::forget(this);
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
        let raw = unsafe { jsc::JSValueMakeNull(ctx) };
        Self::from_owned_raw(ctx, raw)
    }

    fn create_undefined(ctx: &Self::Context) -> Self {
        let ctx = ctx.to_raw();
        let raw = unsafe { jsc::JSValueMakeUndefined(ctx) };
        Self::from_owned_raw(ctx, raw)
    }

    fn from_json_str(ctx: &Self::Context, str: &str) -> Self {
        let ctx = ctx.to_raw();
        let c_str = CString::new(str).unwrap();
        let raw = unsafe {
            let js_string = jsc::JSStringCreateWithUTF8CString(c_str.as_ptr());
            let raw = jsc::JSValueMakeFromJSONString(ctx, js_string);
            jsc::JSStringRelease(js_string);
            raw
        };
        Self::from_owned_raw(ctx, raw)
    }

    fn create_symbol(ctx: &Self::Context, description: &str) -> Self {
        let ctx = ctx.to_raw();
        let c_str = CString::new(description).unwrap();
        let raw = unsafe {
            let js_string = jsc::JSStringCreateWithUTF8CString(c_str.as_ptr());
            let symbol = jsc::JSValueMakeSymbol(ctx, js_string);
            jsc::JSStringRelease(js_string);
            symbol
        };
        Self::from_owned_raw(ctx, raw)
    }

    fn create_date(ctx: &Self::Context, epoch_ms: f64) -> Self {
        let ctx = ctx.to_raw();
        let raw = unsafe {
            let args = [jsc::JSValueMakeNumber(ctx, epoch_ms)];
            let mut exception: jsc::JSValueRef = std::ptr::null_mut();
            let date = jsc::JSObjectMakeDate(ctx, 1, args.as_ptr(), &mut exception);
            if !exception.is_null() {
                // Handle exception - for now just return undefined
                jsc::JSValueMakeUndefined(ctx)
            } else {
                date
            }
        };
        Self::from_owned_raw(ctx, raw)
    }
}

impl_js_converter!(
    JSCValue,
    bool,
    |ctx, value| unsafe { jsc::JSValueMakeBoolean(ctx, value) },
    |ctx, value, result: *mut bool| unsafe {
        if jsc::JSValueIsBoolean(ctx, value) {
            *result = jsc::JSValueToBoolean(ctx, value);
            0
        } else {
            -1
        }
    }
);

// Note: In JavaScriptCore, all numbers are internally represented as f64 (double precision floating point).
// This means there may be precision loss when converting between integer types and JavaScript numbers.
impl_js_converter!(
    JSCValue,
    i32,
    |ctx, value| unsafe { jsc::JSValueMakeNumber(ctx, value as f64) },
    |ctx, value, result: &mut i32| unsafe {
        if !jsc::JSValueIsNumber(ctx, value) {
            return -1;
        }

        let mut exception: jsc::JSValueRef = std::ptr::null_mut();
        let n = jsc::JSValueToNumber(ctx, value, &mut exception);
        if !exception.is_null() {
            return -1;
        }
        *result = n as i32;
        0
    }
);

impl_js_converter!(
    JSCValue,
    f64,
    |ctx, value| unsafe { jsc::JSValueMakeNumber(ctx, value) },
    |ctx, value, result: &mut f64| unsafe {
        if !jsc::JSValueIsNumber(ctx, value) {
            return -1;
        }

        let mut exception: jsc::JSValueRef = std::ptr::null_mut();
        *result = jsc::JSValueToNumber(ctx, value, &mut exception);
        if exception.is_null() { 0 } else { -1 }
    }
);

impl_js_converter!(
    JSCValue,
    &str,
    String,
    |ctx, value: &str| unsafe {
        let cstr = CString::new(value).unwrap();
        let js_str = jsc::JSStringCreateWithUTF8CString(cstr.as_ptr());
        let result = jsc::JSValueMakeString(ctx, js_str);
        jsc::JSStringRelease(js_str);
        result
    },
    |ctx, value, result: *mut String| unsafe {
        // This intentionally skips JSValueIsString check to allow any value that
        // the JS engine can convert to a string, providing more flexible type coercion
        let js_str = jsc::JSValueToStringCopy(ctx, value, std::ptr::null_mut());
        if js_str.is_null() {
            return -1;
        }

        // Get required buffer size (including null terminator)
        let max_size = jsc::JSStringGetMaximumUTF8CStringSize(js_str);
        let mut buffer: Vec<u8> = Vec::with_capacity(max_size);
        let actual_size = jsc::JSStringGetUTF8CString(
            js_str,
            buffer.as_mut_ptr().cast::<::std::os::raw::c_char>(),
            max_size,
        );
        buffer.set_len(actual_size - 1);
        jsc::JSStringRelease(js_str);

        match String::from_utf8(buffer) {
            Ok(s) => {
                *result = s.to_string();
                0
            }
            Err(_) => -1,
        }
    }
);

impl_js_converter!(
    JSCValue,
    u32,
    |ctx, value| unsafe { jsc::JSValueMakeNumber(ctx, value as f64) },
    |ctx, value, result: &mut u32| unsafe {
        if !jsc::JSValueIsNumber(ctx, value) {
            return -1;
        }

        let mut exception: jsc::JSValueRef = std::ptr::null_mut();
        let n = jsc::JSValueToNumber(ctx, value, &mut exception);
        if !exception.is_null() {
            return -1;
        }
        *result = n as u32;
        0
    }
);

impl_js_converter!(
    JSCValue,
    i64,
    |ctx, value: i64| unsafe {
        // For values that fit in JavaScript's safe integer range, use regular number
        // JavaScript safe integer range: -(2^53 - 1) to (2^53 - 1)
        const JS_MAX_SAFE_INTEGER: i64 = (1i64 << 53) - 1;
        const JS_MIN_SAFE_INTEGER: i64 = -JS_MAX_SAFE_INTEGER;

        if (JS_MIN_SAFE_INTEGER..=JS_MAX_SAFE_INTEGER).contains(&value) {
            jsc::JSValueMakeNumber(ctx, value as f64)
        } else {
            bigint_from_i64(ctx, value)
        }
    },
    |ctx, value, result: &mut i64| unsafe {
        if jsvalue_is_bigint(ctx, value) {
            if let Some(v) = bigint_to_i64(ctx, value) {
                *result = v;
                0
            } else {
                -1
            }
        } else if jsc::JSValueIsNumber(ctx, value) {
            // It's a regular number, convert to double first then to i64
            let mut exception: jsc::JSValueRef = std::ptr::null_mut();
            let num = jsc::JSValueToNumber(ctx, value, &mut exception);
            if exception.is_null() {
                *result = num as i64;
                0
            } else {
                -1
            }
        } else {
            // Not a number or BigInt
            -1
        }
    }
);

impl_js_converter!(
    JSCValue,
    u64,
    |ctx, value: u64| unsafe {
        // For values that fit in JavaScript's safe integer range, use regular number
        // JavaScript safe integer range: 0 to (2^53 - 1) for unsigned
        const JS_MAX_SAFE_INTEGER: u64 = (1u64 << 53) - 1;

        if value <= JS_MAX_SAFE_INTEGER {
            jsc::JSValueMakeNumber(ctx, value as f64)
        } else {
            bigint_from_u64(ctx, value)
        }
    },
    |ctx, value, result: &mut u64| unsafe {
        if jsvalue_is_bigint(ctx, value) {
            if let Some(v) = bigint_to_u64(ctx, value) {
                *result = v;
                0
            } else {
                -1
            }
        } else if jsc::JSValueIsNumber(ctx, value) {
            // It's a regular number, convert to double first then to u64
            let mut exception: jsc::JSValueRef = std::ptr::null_mut();
            let num = jsc::JSValueToNumber(ctx, value, &mut exception);
            if exception.is_null() && num >= 0.0 {
                *result = num as u64;
                0
            } else {
                -1
            }
        } else {
            // Not a number or BigInt
            -1
        }
    }
);

impl Clone for JSCValue {
    fn clone(&self) -> Self {
        let mut cloned = JSCValue::new(self.ctx, self.value);
        cloned.value_type = self.value_type;
        cloned.protect()
    }
}
