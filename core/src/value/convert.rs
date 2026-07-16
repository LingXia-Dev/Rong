use super::{JSValue, JSValueImpl};
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

/// extract rust primitive type from JSValue
impl<V, T> FromJSValue<V> for T
where
    V: JSValueImpl,
    V: TryInto<T, Error = RongJSError>,
    T: JSCompatible,
{
    fn from_js_value(_ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        value.into_value().try_into()
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

impl<V> FromJSValue<V> for String
where
    V: JSValueImpl,
    V: TryInto<String, Error = RongJSError>,
{
    fn from_js_value(_ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
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

/// Macro to implement FromJSValue and IntoJSValue for integer types
macro_rules! impl_js_converter_for_int {
    ($($type:ty => $intermediate:ty),*) => {
        $(
            impl<V> FromJSValue<V> for $type
            where
                V: JSValueImpl,
                V: TryInto<$intermediate, Error = RongJSError>,
            {
                fn from_js_value(_ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
                    let intermediate = TryInto::<$intermediate>::try_into(value.into_value())?;
                    <$type>::try_from(intermediate).map_err(|_| {
                        HostError::range_error(
                            crate::error::E_OUT_OF_RANGE,
                            format!(
                                "Value {} is out of range for {}",
                                intermediate,
                                stringify!($type)
                            ),
                        )
                        .into()
                    })
                }
            }

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
