use crate::{JSContext, JSContextImpl, JSResult, RongJSError};
use std::fmt;
use std::hash::Hash;

mod convert;
pub use convert::*;

mod error;
pub use error::*;

mod exception;
pub use exception::*;

mod valuetype;
pub use valuetype::{JSTypeOf, JSValueType};

mod object;
pub use object::*;

mod array;
pub use array::*;

mod array_buffer;
pub use array_buffer::*;

mod bytes;
pub(crate) use bytes::JSBytesData;
pub use bytes::*;

mod typed_array;
pub use typed_array::*;

mod function;
pub use function::*;

mod symbol;
pub use symbol::*;

mod date;
pub use date::*;

mod proxy;
pub use proxy::*;

pub trait JSValueImpl: Clone + PartialEq + Hash {
    /// the JS engine specific type of JavaScript Value
    type RawValue: Copy;

    /// Associates with a type that implements JSContextImpl
    /// This represents the context wrapper type (e.g. QJSContext)
    type Context: JSContextImpl<Value = Self>;

    /// Create a JSValue from borrowed raw parts, increasing reference count to ensure safety.
    /// Used for values received from JS engine callbacks or external sources.
    fn from_borrowed_raw(
        ctx: <Self::Context as JSContextImpl>::RawContext,
        value: Self::RawValue,
    ) -> Self;

    /// Create a JSValue from owned raw parts without reference counting.
    /// Used for values newly created by Rust code that we own directly.
    fn from_owned_raw(
        ctx: <Self::Context as JSContextImpl>::RawContext,
        value: Self::RawValue,
    ) -> Self;

    /// Consumes the ownship and returns the FFI level of JSValue but stop triggering drop.
    /// This API should be used when engine API needs the ownshipe of JS variable
    fn into_raw_value(self) -> Self::RawValue;

    fn as_raw_value(&self) -> &Self::RawValue;
    fn raw_value_for_api(&self) -> Self::RawValue {
        *self.as_raw_value()
    }
    fn as_raw_context(&self) -> &<Self::Context as JSContextImpl>::RawContext;

    /// Create JavaScript null value
    fn create_null(ctx: &Self::Context) -> Self;

    /// Create JavaScript undefined value
    fn create_undefined(ctx: &Self::Context) -> Self;

    /// Create a JavaScript Symbol with the given description
    fn create_symbol(ctx: &Self::Context, descripiton: &str) -> Self;

    /// Creates a JSValue by parsing a JSON string
    fn from_json_str(ctx: &Self::Context, str: &str) -> Self;

    /// Create a Date object from epoch milliseconds
    fn create_date(ctx: &Self::Context, epoch_ms: f64) -> Self;
}

pub struct JSValue<V: JSValueImpl> {
    inner: V,
}

impl<V: JSValueImpl> Clone for JSValue<V> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<V: JSValueImpl> PartialEq for JSValue<V> {
    fn eq(&self, other: &Self) -> bool {
        self.inner.eq(&other.inner)
    }
}

impl<V: JSValueImpl> Eq for JSValue<V> {}

impl<V: JSValueImpl> Hash for JSValue<V> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.inner.hash(state);
    }
}

impl<V> JSValue<V>
where
    V: JSValueImpl,
{
    pub fn from_raw(_ctx: &JSContext<V::Context>, value: V) -> Self {
        Self { inner: value }
    }

    pub fn as_value(&self) -> &V {
        &self.inner
    }

    pub fn into_value(self) -> V {
        self.inner
    }

    /// Returns a non-owning context handle associated with this value.
    ///
    /// The handle can be used while a runtime-created context owner is alive, but does
    /// not delay context shutdown when retained by callbacks or async work.
    pub fn context(&self) -> JSContext<V::Context> {
        JSContext::from_borrowed_raw_ptr(self.as_value().as_raw_context())
    }

    /// Converts a Rust value into a `JSValue`.
    pub fn from_rust<T>(ctx: &JSContext<V::Context>, val: T) -> Self
    where
        T: IntoJSValue<V>,
    {
        <T as IntoJSValue<V>>::into_js_value(val, ctx)
    }

    /// Converts a JavaScript value into a Rust value.
    pub fn to_rust<T>(self) -> JSResult<T>
    where
        T: FromJSValue<V>,
    {
        let ctx = self.context();
        T::from_js_value(&ctx, self)
    }

    /// create JS UNDEFINED Value
    pub fn undefined(ctx: &JSContext<V::Context>) -> Self {
        let value = V::create_undefined(ctx.as_ref());
        JSValue::from_raw(ctx, value)
    }

    /// create JS NULL Value
    pub fn null(ctx: &JSContext<V::Context>) -> Self {
        let value = V::create_null(ctx.as_ref());
        JSValue::from_raw(ctx, value)
    }
}

impl<V> JSValue<V>
where
    V: JSValueImpl + JSTypeOf,
{
    /// Convert JSValue into JSObject if it is an object
    ///
    /// # Returns
    /// - `Some(JSObject)` if the value is an object
    /// - `None` if the value is not an object
    pub fn into_object(self) -> Option<JSObject<V>> {
        self.take_is_object().map(|v| v.into())
    }
}

impl<V> FromJSValue<V> for JSValue<V>
where
    V: JSValueImpl,
{
    fn from_js_value(_ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        Ok(value)
    }
}

impl<V> IntoJSValue<V> for JSValue<V>
where
    V: JSValueImpl,
{
    fn into_js_value(self, _ctx: &JSContext<V::Context>) -> JSValue<V> {
        self
    }
}

/// Converts a JSON string into a JSValue
pub trait JsonToJSValue<V>
where
    V: JSValueImpl,
{
    /// Converts the JSON string into a JSValue using the provided context
    fn json_to_js_value(self, ctx: &JSContext<V::Context>) -> JSResult<JSValue<V>>;
}

impl<V> JsonToJSValue<V> for &str
where
    V: JSObjectOps + JSTypeOf,
{
    fn json_to_js_value(self, ctx: &JSContext<V::Context>) -> JSResult<JSValue<V>> {
        let result = V::from_json_str(ctx.as_ref(), self);
        result.try_map(|v| JSValue::from_raw(ctx, v))
    }
}

/// Provides safe conversion and mapping operations for JS values
pub trait JSValueMapper<V: JSValueImpl> {
    /// Attempts to convert the value using FromJSValue, returns error if it's an exception
    fn try_convert<T>(self) -> JSResult<T>
    where
        T: FromJSValue<V>;

    /// Maps the value using the provided closure if it's not an exception,
    /// otherwise returns the error
    fn try_map<T, F>(self, f: F) -> JSResult<T>
    where
        F: FnOnce(Self) -> T,
        Self: Sized;
}

impl<V> JSValueMapper<V> for V
where
    V: JSTypeOf,
    V: JSObjectOps,
{
    fn try_convert<T>(self) -> JSResult<T>
    where
        T: FromJSValue<V>,
    {
        self.try_map(|value| {
            let ctx = JSContext::from_borrowed_raw_ptr(value.as_raw_context());
            T::from_js_value(&ctx, JSValue::from_raw(&ctx, value))
        })?
    }

    fn try_map<T, F>(self, f: F) -> JSResult<T>
    where
        F: FnOnce(Self) -> T,
    {
        if self.is_exception() {
            let ctx: JSContext<V::Context> =
                JSContext::from_borrowed_raw_ptr(self.as_raw_context());
            Err(RongJSError::from_thrown_value(JSValue::from_raw(
                &ctx, self,
            )))
        } else {
            Ok(f(self))
        }
    }
}

#[macro_export]
macro_rules! impl_js_converter {
    ($target:ty, $in_type:ty, $out_type:ty, $create_fn:expr, $to_fn:expr) => {
        impl TryInto<$out_type> for $target
        where
            Self: JSValueImpl,
        {
            type Error = RongJSError;
            fn try_into(self) -> Result<$out_type, Self::Error> {
                let mut result: $out_type = Default::default();
                if unsafe {
                    $to_fn(
                        *self.as_raw_context(),
                        self.raw_value_for_api(),
                        &mut result,
                    )
                } < 0
                {
                    Err($crate::HostError::new(
                        $crate::error::E_TYPE,
                        format!(
                            "Expected JSValue to be type {}, but got {:?}",
                            std::any::type_name::<$out_type>(),
                            self.type_of()
                        ),
                    )
                    .with_name("TypeError")
                    .into())
                } else {
                    Ok(result)
                }
            }
        }

        impl<T> From<(&T, $in_type)> for $target
        where
            T: JSContextImpl<RawContext = <$target as JSRawContext>::RawContext>,
            $target: JSValueImpl<Context = T>,
        {
            fn from(t: (&T, $in_type)) -> Self {
                let ctx = t.0.as_raw();
                let raw = unsafe { $create_fn(*ctx, t.1) };
                Self::from_owned_raw(*ctx, raw)
            }
        }
    };

    ($target:ty, $type:ty, $create_fn:expr, $to_fn:expr) => {
        impl_js_converter!($target, $type, $type, $create_fn, $to_fn);
    };
}

// blanket implementing.
impl<V: JSValueImpl> crate::function::JSParameterType for JSValue<V> {}

impl<V> fmt::Display for JSValue<V>
where
    V: JSTypeOf + JSValueConversion,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.type_of() {
            JSValueType::Boolean => {
                if let Ok(val) = self.clone().to_rust::<bool>() {
                    write!(f, "{}", val)
                } else {
                    write!(f, "boolean")
                }
            }
            JSValueType::Number => {
                if let Ok(val) = self.clone().to_rust::<f64>() {
                    write!(f, "{}", val)
                } else {
                    write!(f, "number")
                }
            }
            JSValueType::String => {
                if let Ok(val) = self.clone().to_rust::<String>() {
                    write!(f, "{}", val)
                } else {
                    write!(f, "string")
                }
            }
            JSValueType::Date => {
                if let Ok(val) = self.clone().to_rust::<String>() {
                    write!(f, "{}", val)
                } else {
                    write!(f, "Date")
                }
            }
            JSValueType::Undefined => write!(f, "undefined"),
            JSValueType::Null => write!(f, "null"),
            JSValueType::BigInt => write!(f, "bigint"),
            JSValueType::Object => write!(f, "object"),
            JSValueType::Array => write!(f, "array"),
            JSValueType::ArrayBuffer => write!(f, "arrayBuffer"),
            JSValueType::Function => write!(f, "function"),
            JSValueType::Constructor => write!(f, "constructor"),
            JSValueType::Promise => write!(f, "promise"),
            JSValueType::Symbol => write!(f, "symbol"),
            JSValueType::Error => write!(f, "error"),
            JSValueType::Exception => write!(f, "exception"),
            JSValueType::Unknown => write!(f, "unknown"),
        }
    }
}

impl<V> fmt::Debug for JSValue<V>
where
    V: JSTypeOf + JSValueConversion,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "JSValue({})", self)
    }
}
