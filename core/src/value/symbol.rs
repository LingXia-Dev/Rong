use crate::{
    FromJSValue, HostError, IntoJSValue, JSContext, JSObject, JSObjectOps, JSResult, JSTypeOf,
    JSValue, JSValueImpl, JSValueMapper,
};
use std::ops::Deref;

#[derive(Hash, PartialEq, Eq, Clone)]
pub struct JSSymbol<V: JSValueImpl>(JSObject<V>);

impl<V> JSSymbol<V>
where
    V: JSObjectOps + JSValueMapper<V>,
{
    /// create JS Symbol Value
    pub fn new(ctx: &JSContext<V::Context>, descripiton: impl AsRef<str>) -> JSResult<Self> {
        let value = V::create_symbol(ctx.as_ref(), descripiton.as_ref());
        value.try_map(|value| Self(JSValue::from_raw(ctx, value).into()))
    }

    pub fn descripiton(&self) -> JSResult<String> {
        self.0.get::<_, String>("description")
    }

    /// Construct a JSSymbol from a JSObject if it is a JS Symbol
    pub fn from_object(obj: JSObject<V>) -> Option<Self> {
        if obj.is_symbol() {
            Some(Self(obj))
        } else {
            None
        }
    }
}

impl<V> JSSymbol<V>
where
    V: JSValueImpl,
{
    pub fn into_value(self) -> V {
        self.0.into_value()
    }
}

impl<V: JSValueImpl> Deref for JSSymbol<V> {
    type Target = JSObject<V>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<V> FromJSValue<V> for JSSymbol<V>
where
    V: JSTypeOf,
{
    fn from_js_value(_ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        if value.is_symbol() {
            Ok(Self(value.into()))
        } else {
            Err(HostError::type_mismatch("a symbol", value.type_of().to_string()).into())
        }
    }
}

impl<V> IntoJSValue<V> for JSSymbol<V>
where
    V: JSValueImpl,
{
    fn into_js_value(self, _ctx: &JSContext<V::Context>) -> JSValue<V> {
        self.0.into_js_value()
    }
}
