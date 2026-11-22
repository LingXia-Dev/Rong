//! Opaque bytes wrapper for Rust/JS interop.
//!
//! Typical flow:
//! 1. Rust creates `JSBytes` from an existing `Bytes` payload.
//! 2. JavaScript receives the object and forwards it without unpacking.
//! 3. Rust accepts `JSBytes` or `Bytes` again and continues processing.
//!
//! The main scenario is `rust -> js -> rust`.
//! JavaScript is not expected to interpret the payload structure here; it only
//! carries an opaque object across API boundaries.
//!
//! Boundaries:
//! - `JSBytes` is a transport type, not a semantic type.
//! - It does not mean JSON, protobuf, UTF-8 text, or any other schema.
//! - If callers need structured meaning, that should be expressed by the API
//!   using `JSBytes`, not by `JSBytes` itself.
//!
//! Typical uses:
//! - Rust produces a request body, JS routes it, Rust sends it onward.
//! - Rust returns bytes to JS, JS passes them into another Rust callback.
//! - A text payload is converted to bytes once and then carried opaquely.
//!
use bytes::Bytes;

use crate::function::{Constructor, Optional};
use crate::{
    Class, ClassSetup, FromJSValue, HostError, IntoJSValue, JSArrayOps, JSClass, JSContext,
    JSErrorFactory, JSExceptionThrower, JSObject, JSObjectOps, JSResult, JSTypeOf, JSValue,
    JSValueConversion, JSValueImpl, PropertyDescriptor,
};

use std::ops::Deref;

#[derive(Clone)]
pub(crate) struct JSBytesData {
    bytes: Bytes,
}

#[derive(Clone)]
/// JavaScript-visible opaque byte handle.
///
/// `JSBytes` intentionally exposes a small surface:
/// - construct from Rust
/// - carry through JS
/// - extract back in Rust
///
/// It is not intended to compete with `ArrayBuffer`/`TypedArray`, and it does
/// not expose general byte-manipulation APIs on the JS side.
pub struct JSBytes<V: JSValueImpl> {
    inner: JSObject<V>,
}

impl<V: JSValueImpl> Deref for JSBytes<V> {
    type Target = JSObject<V>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<V> JSBytes<V>
where
    V: JSValueImpl + JSObjectOps + JSTypeOf + JSValueConversion + JSArrayOps + 'static,
    V::Context: JSErrorFactory + JSExceptionThrower,
{
    /// Byte length of the wrapped payload.
    pub fn len(&self) -> JSResult<usize> {
        Ok(self.inner.borrow::<JSBytesData>()?.bytes.len())
    }

    pub fn is_empty(&self) -> JSResult<bool> {
        Ok(self.inner.borrow::<JSBytesData>()?.bytes.is_empty())
    }

    /// Clone out the underlying bytes.
    ///
    /// `Bytes` is cheap to clone, so this is the primary Rust-side extraction API.
    pub fn to_bytes(&self) -> JSResult<Bytes> {
        Ok(self.inner.borrow::<JSBytesData>()?.bytes.clone())
    }

    pub(crate) fn to_vec(&self) -> JSResult<Vec<u8>> {
        Ok(self.to_bytes()?.to_vec())
    }

    /// Decode the payload as UTF-8 text.
    ///
    /// Use this only when the surrounding API already knows the bytes represent
    /// text. Binary callers should prefer [`JSBytes::to_bytes`].
    pub fn to_string(&self) -> JSResult<String> {
        String::from_utf8(self.to_vec()?).map_err(|err| {
            HostError::new(
                crate::error::E_TYPE,
                format!("JSBytes contains invalid UTF-8: {}", err),
            )
            .with_name("TypeError")
            .into()
        })
    }

    pub(crate) fn from_object(obj: JSObject<V>) -> Option<Self> {
        if Class::instance_of::<JSBytesData>(&obj) {
            Some(Self { inner: obj })
        } else {
            None
        }
    }
}

impl<V> JSBytes<V>
where
    V: JSValueImpl + JSObjectOps + JSTypeOf + JSValueConversion + JSArrayOps + 'static,
    V::Context: JSErrorFactory + JSExceptionThrower,
{
    /// Create `JSBytes` from an existing Rust `Bytes` payload.
    ///
    /// This is the main constructor for exchange-oriented flows where Rust
    /// creates data and JS only forwards it.
    pub fn from_bytes(ctx: &JSContext<V::Context>, bytes: Bytes) -> JSResult<Self> {
        ctx.register_hidden_class::<JSBytesData>()?;
        let instance = Class::lookup::<JSBytesData>(ctx)?.instance(JSBytesData { bytes });
        Ok(Self { inner: instance })
    }

    /// Create `JSBytes` from UTF-8 text by storing the underlying text bytes.
    pub fn from_string<S>(ctx: &JSContext<V::Context>, text: S) -> JSResult<Self>
    where
        S: Into<String>,
    {
        Self::from_bytes(ctx, Bytes::from(text.into()))
    }
}

impl<V> JSClass<V> for JSBytesData
where
    V: JSValueImpl + JSObjectOps + JSTypeOf + JSValueConversion + JSArrayOps + 'static,
    V::Context: JSErrorFactory + JSExceptionThrower,
{
    const NAME: &'static str = "JSBytes";

    fn data_constructor() -> Constructor<V> {
        Constructor::new(
            |_ctx: JSContext<V::Context>, _arg: Optional<JSValue<V>>| -> JSResult<JSBytes<V>> {
                crate::illegal_constructor("JSBytes cannot be constructed from JavaScript")
            },
        )
    }

    fn call_without_new() -> Constructor<V> {
        Constructor::new(
            |_ctx: JSContext<V::Context>, _arg: Optional<JSValue<V>>| -> JSResult<JSBytes<V>> {
                crate::illegal_constructor("JSBytes cannot be constructed from JavaScript")
            },
        )
    }

    fn class_setup(class: &ClassSetup<V>) -> JSResult<()> {
        let getter = class.new_func(|this: crate::function::This<JSBytes<V>>| this.len())?;
        class.property(
            "length",
            PropertyDescriptor::from_getter(getter).configurable(),
        )?;

        class.method("toString", |this: crate::function::This<JSBytes<V>>| {
            this.to_string()
        })?;

        Ok(())
    }
}

impl<V> IntoJSValue<V> for JSBytes<V>
where
    V: JSValueImpl,
{
    fn into_js_value(self, _ctx: &JSContext<V::Context>) -> JSValue<V> {
        self.inner.into_js_value()
    }
}

impl<V> FromJSValue<V> for JSBytes<V>
where
    V: JSValueImpl + JSTypeOf + JSObjectOps + JSValueConversion + JSArrayOps + 'static,
    V::Context: JSErrorFactory + JSExceptionThrower,
{
    fn from_js_value(ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        let refuse = |got: String| -> crate::RongJSError {
            HostError::type_mismatch("a JSBytes instance", got).into()
        };
        let got = value.type_of().to_string();
        if !value.is_object() {
            return Err(refuse(got));
        }
        let obj = JSObject::from_js_value(ctx, value)?;
        Self::from_object(obj).ok_or_else(|| refuse(got))
    }
}

impl<V> FromJSValue<V> for Bytes
where
    V: JSValueImpl + JSTypeOf + JSObjectOps + JSValueConversion + JSArrayOps + 'static,
    V::Context: JSErrorFactory + JSExceptionThrower,
{
    fn from_js_value(ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        JSBytes::from_js_value(ctx, value)?.to_bytes()
    }
}

impl<V: JSValueImpl> crate::function::JSParameterType for JSBytes<V> {}
impl crate::function::JSParameterType for Bytes {}
