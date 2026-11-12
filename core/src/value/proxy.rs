use crate::{
    FromJSValue, HostError, IntoJSValue, JSContext, JSObject, JSObjectOps, JSResult, JSTypeOf,
    JSValue, JSValueImpl, RongJSError,
};
use std::ops::Deref;

#[derive(Hash, PartialEq, Eq)]
pub struct JSProxy<V: JSValueImpl>(JSObject<V>);

impl<V: JSValueImpl> Clone for JSProxy<V> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<V: JSValueImpl> Deref for JSProxy<V> {
    type Target = JSObject<V>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub trait JSProxyOps: JSTypeOf {
    /// Creates a JavaScript Proxy equivalent to `new Proxy(target, handler)`.
    fn new_proxy(ctx: &Self::Context, target: Self, handler: Self) -> Result<Self, Self>;

    /// Returns the target of a JavaScript Proxy.
    fn proxy_target(&self) -> Result<Self, Self>;
}

impl<V> JSProxy<V>
where
    V: JSProxyOps,
{
    pub fn new(
        ctx: &JSContext<V::Context>,
        target: JSObject<V>,
        handler: JSObject<V>,
    ) -> JSResult<Self>
    where
        V: JSObjectOps,
    {
        let value = V::new_proxy(ctx.as_ref(), target.into_value(), handler.into_value())
            .map_err(|thrown| RongJSError::from_thrown_value(JSValue::from_raw(ctx, thrown)))?;
        Self::from_js_value(ctx, JSValue::from_raw(ctx, value))
    }

    pub fn target(&self) -> JSResult<JSObject<V>>
    where
        V: JSObjectOps,
    {
        let ctx = self.context();
        let value = self
            .as_value()
            .proxy_target()
            .map_err(|thrown| RongJSError::from_thrown_value(JSValue::from_raw(&ctx, thrown)))?;
        JSObject::from_js_value(&ctx, JSValue::from_raw(&ctx, value))
    }

    pub fn from_object(obj: JSObject<V>) -> Option<Self> {
        if obj.is_proxy() {
            Some(Self(obj))
        } else {
            None
        }
    }
}

impl<V: JSValueImpl> JSProxy<V> {
    pub fn into_value(self) -> V {
        self.0.into_value()
    }
}

impl<V> FromJSValue<V> for JSProxy<V>
where
    V: JSTypeOf,
{
    fn from_js_value(_ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        if value.is_proxy() {
            Ok(Self(value.into()))
        } else {
            Err(HostError::not_proxy().into())
        }
    }
}

impl<V> IntoJSValue<V> for JSProxy<V>
where
    V: JSValueImpl,
{
    fn into_js_value(self, _ctx: &JSContext<V::Context>) -> JSValue<V> {
        self.0.into_js_value()
    }
}
