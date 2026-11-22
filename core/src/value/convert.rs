use super::{JSTypeOf, JSValue, JSValueImpl};
use crate::{HostError, JSContext, JSResult, RongJSError};

/// The conversion between Rust primitive types, which implement the `JSCompatible`
/// marker trait, and `JSValue` is facilitated using the standard `TryInto` and
/// `From` traits.
///
/// The `impl_js_converter` macro simplifies the implementation of these conversions.
///
/// Since `TryInto` and `From` are implemented for local types like `QJSValue` in
/// the `quickjs` crate, the orphan rule does not apply here.
///
/// We introduce `FromJSValue` and `IntoJSValue` traits for application-level use
/// cases:
/// - `FromJSValue` is particularly useful for returning generic types from operations
///   such as `eval` and `JSObject.get`.
/// - `IntoJSValue` allows `JSObject.set` to accept a wider range of types that can be converted into `JSValue`.
///
/// help trait contains conversion trait bound
/// it help simplify trait boud for upper caller
pub trait JSValueConversion:
    JSValueImpl
    + for<'a> From<(&'a Self::Context, bool)>
    + for<'a> From<(&'a Self::Context, i32)>
    + for<'a> From<(&'a Self::Context, u32)>
    + for<'a> From<(&'a Self::Context, i64)>
    + for<'a> From<(&'a Self::Context, u64)>
    + for<'a> From<(&'a Self::Context, f64)>
    + for<'a> From<(&'a Self::Context, &'a str)>
    + TryInto<bool, Error = RongJSError>
    + TryInto<i32, Error = RongJSError>
    + TryInto<u32, Error = RongJSError>
    + TryInto<i64, Error = RongJSError>
    + TryInto<u64, Error = RongJSError>
    + TryInto<f64, Error = RongJSError>
    + TryInto<String, Error = RongJSError>
{
}

// Automatically implement types that satisfy trait bound
impl<T> JSValueConversion for T where
    T: JSValueImpl
        + for<'a> From<(&'a T::Context, bool)>
        + for<'a> From<(&'a T::Context, i32)>
        + for<'a> From<(&'a T::Context, u32)>
        + for<'a> From<(&'a T::Context, i64)>
        + for<'a> From<(&'a T::Context, u64)>
        + for<'a> From<(&'a T::Context, f64)>
        + for<'a> From<(&'a T::Context, &'a str)>
        + TryInto<bool, Error = RongJSError>
        + TryInto<i32, Error = RongJSError>
        + TryInto<u32, Error = RongJSError>
        + TryInto<i64, Error = RongJSError>
        + TryInto<u64, Error = RongJSError>
        + TryInto<f64, Error = RongJSError>
        + TryInto<String, Error = RongJSError>
{
}

/// Marker trait for types that are compatible with JavaScript values
pub trait JSCompatible: Sized {}

impl JSCompatible for i32 {}
impl JSCompatible for u32 {}
impl JSCompatible for i64 {}
impl JSCompatible for u64 {}
impl JSCompatible for f64 {}
impl JSCompatible for bool {}

/// The trait that supports extract type from JSValue
/// Why from_js_value don't use V as input type ? Because it needs to
/// return JSValue, JSObject.
pub trait FromJSValue<V>: Sized
where
    V: JSValueImpl,
{
    fn from_js_value(ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self>;
}

/// What a refused value was, for "…, got <what>": its type, or the number
/// itself when an integer was expected.
fn describe_got<V: JSTypeOf>(value: &JSValue<V>) -> String {
    value.type_of().to_string()
}

/// Accepts only a JS boolean.
impl<V> FromJSValue<V> for bool
where
    V: JSValueImpl + JSTypeOf + TryInto<bool, Error = RongJSError>,
{
    fn from_js_value(_ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        if !value.is_boolean() {
            return Err(HostError::type_mismatch("a boolean", describe_got(&value)).into());
        }
        value.into_value().try_into()
    }
}

/// Accepts only a JS number (including `NaN` and the infinities).
impl<V> FromJSValue<V> for f64
where
    V: JSValueImpl + JSTypeOf + TryInto<f64, Error = RongJSError>,
{
    fn from_js_value(_ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        if !value.is_number() {
            return Err(HostError::type_mismatch("a number", describe_got(&value)).into());
        }
        value.into_value().try_into()
    }
}

/// An integral JS number within `min..=max`; a fraction, `NaN` or an infinity
/// is a type mismatch, an integral number outside the range a range mismatch.
fn integral_number<V>(value: JSValue<V>, min: f64, max: f64, range: &'static str) -> JSResult<f64>
where
    V: JSValueImpl + JSTypeOf + TryInto<f64, Error = RongJSError>,
{
    if !value.is_number() {
        return Err(HostError::type_mismatch("an integer", describe_got(&value)).into());
    }
    let n: f64 = value.into_value().try_into()?;
    if !n.is_finite() || n.fract() != 0.0 {
        return Err(HostError::type_mismatch("an integer", format_number(n)).into());
    }
    if n < min || n > max {
        return Err(HostError::range_mismatch(range, format_number(n)).into());
    }
    Ok(n)
}

fn format_number(n: f64) -> String {
    if n.is_nan() {
        "NaN".to_string()
    } else if n.is_infinite() {
        if n > 0.0 { "Infinity" } else { "-Infinity" }.to_string()
    } else {
        n.to_string()
    }
}

macro_rules! impl_strict_int {
    ($($type:ty => $range:literal),* $(,)?) => {
        $(
            /// Accepts only an integral JS number in range; no truncation and no
            /// wrapping.
            impl<V> FromJSValue<V> for $type
            where
                V: JSValueImpl + JSTypeOf + TryInto<f64, Error = RongJSError>,
            {
                fn from_js_value(_ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
                    let n = integral_number(value, <$type>::MIN as f64, <$type>::MAX as f64, $range)?;
                    Ok(n as $type)
                }
            }
        )*
    };
}

impl_strict_int! {
    i8 => "an integer from -128 to 127",
    u8 => "an integer from 0 to 255",
    i16 => "an integer from -32768 to 32767",
    u16 => "an integer from 0 to 65535",
    i32 => "an integer from -2147483648 to 2147483647",
    u32 => "an integer from 0 to 4294967295",
}

macro_rules! impl_strict_wide_int {
    ($($type:ty => $range:literal),* $(,)?) => {
        $(
            /// Accepts an integral JS number within the safe-integer range or a
            /// BigInt that fits; no truncation.
            impl<V> FromJSValue<V> for $type
            where
                V: JSValueImpl
                    + JSTypeOf
                    + TryInto<f64, Error = RongJSError>
                    + TryInto<String, Error = RongJSError>,
            {
                fn from_js_value(_ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
                    if value.is_bigint() {
                        // Read the decimal digits: the engines' BigInt-to-integer
                        // calls wrap modulo 2^64 instead of failing.
                        let text = value.to_string_coerced()?;
                        return text.parse::<$type>().map_err(|_| {
                            HostError::range_mismatch($range, format!("{text}n")).into()
                        });
                    }
                    const MAX_SAFE: f64 = 9_007_199_254_740_991.0;
                    let min = (<$type>::MIN as f64).max(-MAX_SAFE);
                    let n = integral_number(value, min, MAX_SAFE, $range)?;
                    Ok(n as $type)
                }
            }
        )*
    };
}

impl_strict_wide_int! {
    i64 => "a safe integer or a bigint that fits i64",
    u64 => "a non-negative safe integer or a bigint that fits u64",
}

/// Accepts an integral JS number within the safe-integer range, or a BigInt
/// that fits.
impl<V> FromJSValue<V> for isize
where
    V: JSValueImpl
        + JSTypeOf
        + TryInto<f64, Error = RongJSError>
        + TryInto<String, Error = RongJSError>,
{
    fn from_js_value(ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        let n = i64::from_js_value(ctx, value)?;
        isize::try_from(n).map_err(|_| {
            HostError::range_mismatch("an integer that fits isize", n.to_string()).into()
        })
    }
}

/// Accepts a non-negative integral JS number within the safe-integer range, or
/// a BigInt that fits.
impl<V> FromJSValue<V> for usize
where
    V: JSValueImpl
        + JSTypeOf
        + TryInto<f64, Error = RongJSError>
        + TryInto<String, Error = RongJSError>,
{
    fn from_js_value(ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        let n = u64::from_js_value(ctx, value)?;
        usize::try_from(n).map_err(|_| {
            HostError::range_mismatch("an integer that fits usize", n.to_string()).into()
        })
    }
}

/// Opt-in JS coercion, for the places that want a JavaScript conversion
/// instead of a type check: Web APIs whose parameters are `DOMString` or
/// `long`, and code that formats arbitrary values.
///
/// - `Coerced<String>`: any value through the engine's `ToString` (`42` →
///   `"42"`, `undefined` → `"undefined"`); fails only if `ToString` throws
///   (a symbol, or a throwing `toString`).
/// - `Coerced<i32>`, `Coerced<u32>`: any number, truncated and wrapped as
///   ECMAScript `ToInt32` / `ToUint32` do; a non-number is refused like a
///   plain integer (`must be a number, got string`).
/// - `Coerced<i64>`, `Coerced<u64>`: any number, truncated; or a BigInt.
/// - `Coerced<f64>`, `Coerced<bool>`: no coercion; the strict conversion of
///   the plain type, with the same errors and location.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Coerced<T>(pub T);

impl<T> Coerced<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> std::ops::Deref for Coerced<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<V, T> FromJSValue<V> for Coerced<T>
where
    V: JSValueImpl,
    T: CoercedTarget<V>,
{
    fn from_js_value(ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        T::coerce(ctx, value).map(Coerced)
    }
}

/// Types [`Coerced`] can produce, and how each converts.
pub trait CoercedTarget<V: JSValueImpl>: Sized {
    fn coerce(ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self>;
}

impl<V> CoercedTarget<V> for String
where
    V: JSValueImpl + TryInto<String, Error = RongJSError>,
{
    fn coerce(_ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        value.into_value().try_into()
    }
}

/// Integers: the engine's truncating conversion, after the same kind check as
/// the plain types so a refusal carries its location.
macro_rules! impl_coerced_integer {
    ($($ty:ty, $expected:expr, |$value:ident| $accepts:expr);* $(;)?) => {$(
        impl<V> CoercedTarget<V> for $ty
        where
            V: JSValueImpl + JSTypeOf + TryInto<$ty, Error = RongJSError>,
        {
            fn coerce(_ctx: &JSContext<V::Context>, $value: JSValue<V>) -> JSResult<Self> {
                if !$accepts {
                    return Err(HostError::type_mismatch($expected, describe_got(&$value)).into());
                }
                $value.into_value().try_into()
            }
        }
    )*};
}
impl_coerced_integer!(
    i32, "a number", |value| value.is_number();
    u32, "a number", |value| value.is_number();
    i64, "a number or a bigint", |value| (value.is_number() || value.is_bigint());
    u64, "a number or a bigint", |value| (value.is_number() || value.is_bigint());
);

/// No coercion: the strict conversion, so errors and their location match the
/// plain type.
macro_rules! impl_coerced_strict {
    ($($ty:ty),*) => {$(
        impl<V> CoercedTarget<V> for $ty
        where
            V: JSValueImpl,
            $ty: FromJSValue<V>,
        {
            fn coerce(ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
                <$ty as FromJSValue<V>>::from_js_value(ctx, value)
            }
        }
    )*};
}
impl_coerced_strict!(f64, bool);

impl<T> crate::function::JSParameterType for Coerced<T> {}

impl<V, T> IntoJSValue<V> for Coerced<T>
where
    V: JSValueImpl,
    T: IntoJSValue<V>,
{
    fn into_js_value(self, ctx: &JSContext<V::Context>) -> JSValue<V> {
        self.0.into_js_value(ctx)
    }
}

impl<V> JSValue<V>
where
    V: JSValueImpl + TryInto<String, Error = RongJSError>,
{
    /// The value as text through the engine's `ToString`, as `String(value)`
    /// does for anything but a symbol. Use it to format; parameters that
    /// need a string should take `String`, which refuses non-strings.
    pub fn to_string_coerced(&self) -> JSResult<String> {
        self.clone().into_value().try_into()
    }
}

impl<V> FromJSValue<V> for ()
where
    V: JSValueImpl,
{
    fn from_js_value(_ctx: &JSContext<V::Context>, _value: JSValue<V>) -> JSResult<Self> {
        Ok(())
    }
}

/// Accepts only a JS string; use [`Coerced<String>`] or
/// [`JSValue::to_string_coerced`] for `ToString` semantics.
impl<V> FromJSValue<V> for String
where
    V: JSValueImpl + JSTypeOf,
    V: TryInto<String, Error = RongJSError>,
{
    fn from_js_value(_ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        if !value.is_string() {
            return Err(HostError::type_mismatch("a string", describe_got(&value)).into());
        }
        value.into_value().try_into()
    }
}

impl<V, T> FromJSValue<V> for Option<T>
where
    V: JSValueImpl + super::JSTypeOf,
    T: FromJSValue<V>,
{
    fn from_js_value(ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        if value.is_null() || value.is_undefined() {
            Ok(None)
        } else {
            T::from_js_value(ctx, value).map(Some)
        }
    }
}

/// convert to JS Value represented by trait JSValueImpl
pub trait IntoJSValue<V>
where
    V: JSValueImpl,
{
    fn into_js_value(self, ctx: &JSContext<V::Context>) -> JSValue<V>;
}

impl<V> IntoJSValue<V> for &str
where
    V: JSValueImpl,
    V: for<'a> From<(&'a V::Context, &'a str)>,
{
    fn into_js_value(self, ctx: &JSContext<V::Context>) -> JSValue<V> {
        let raw = V::from((ctx.as_ref(), self));
        JSValue::from_raw(ctx, raw)
    }
}

impl<V> IntoJSValue<V> for String
where
    V: JSValueImpl,
    V: for<'a> From<(&'a V::Context, &'a str)>,
{
    fn into_js_value(self, ctx: &JSContext<V::Context>) -> JSValue<V> {
        let raw = V::from((ctx.as_ref(), self.as_str()));
        JSValue::from_raw(ctx, raw)
    }
}

impl<V> IntoJSValue<V> for ()
where
    V: JSValueImpl,
{
    fn into_js_value(self, ctx: &JSContext<V::Context>) -> JSValue<V> {
        JSValue::from_raw(ctx, V::create_undefined(ctx.as_ref()))
    }
}

/// convert rust primitive type to JSValue
impl<V, T> IntoJSValue<V> for T
where
    V: JSValueImpl,
    V: for<'a> From<(&'a V::Context, T)>,
    T: JSCompatible,
{
    fn into_js_value(self, ctx: &JSContext<V::Context>) -> JSValue<V> {
        JSValue::from_raw(ctx, V::from((ctx.as_ref(), self)))
    }
}

impl<V, T> IntoJSValue<V> for Option<T>
where
    V: JSValueImpl,
    T: IntoJSValue<V>,
{
    fn into_js_value(self, ctx: &JSContext<V::Context>) -> JSValue<V> {
        match self {
            Some(value) => value.into_js_value(ctx),
            None => JSValue::from_raw(ctx, V::create_null(ctx.as_ref())), // Returns null in JS when None
        }
    }
}

/// Macro to implement IntoJSValue for the narrow and pointer-sized integers
macro_rules! impl_js_converter_for_int {
    ($($type:ty => $intermediate:ty),*) => {
        $(
            impl<V> IntoJSValue<V> for $type
            where
                V: JSValueImpl,
                V: for<'a> From<(&'a V::Context, $intermediate)>,
            {
                fn into_js_value(self, ctx: &JSContext<V::Context>) -> JSValue<V> {
                    JSValue::from_raw(ctx, V::from((ctx.as_ref(), self as $intermediate)))
                }
            }
        )*
    };
}

// Implement for i8, u8, i16, u16, usize, isize
impl_js_converter_for_int! {
    i8 => i32,
    u8 => u32,
    i16 => i32,
    u16 => u32,
    usize => u64,
    isize => i64
}
