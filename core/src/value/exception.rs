use super::{JSError, JSErrorFactory};
use crate::{
    FromJSValue, HostError, IntoJSValue, JSContext, JSContextImpl, JSObject, JSObjectOps, JSResult,
    JSTypeOf, JSValue, JSValueImpl,
};
use std::fmt;
use std::ops::Deref;
use std::string::String;

/// Represents a JavaScript exception object wrapper
pub struct JSException<V: JSValueImpl>(JSObject<V>);

impl<V: JSValueImpl> Deref for JSException<V> {
    type Target = JSObject<V>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<V> IntoJSValue<V> for JSException<V>
where
    V: JSValueImpl,
{
    fn into_js_value(self, _ctx: &JSContext<V::Context>) -> JSValue<V> {
        self.0.into_js_value()
    }
}

impl<V> JSException<V>
where
    V: JSValueImpl + JSTypeOf,
{
    pub fn from_object(value: JSObject<V>) -> Option<Self> {
        if value.is_exception() {
            return Some(Self(value));
        }
        None
    }
}

impl<V> FromJSValue<V> for JSException<V>
where
    V: JSTypeOf,
{
    fn from_js_value(ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        if value.is_exception() {
            Ok(Self(JSObject::from_js_value(ctx, value)?))
        } else {
            Err(HostError::not_exception().into())
        }
    }
}

impl<V> JSException<V>
where
    V: JSObjectOps,
{
    /// Returns the message of the error.
    ///
    /// Same as retrieving `error.message` in JavaScript.
    pub fn message(&self) -> Option<String> {
        self.get("message").ok()
    }

    /// Returns the stack of the error.
    ///
    /// Same as retrieving `error.stack` in JavaScript.
    pub fn stack(&self) -> Option<String> {
        self.get("stack").ok()
    }

    /// Convert the exception into JSError
    pub fn into_error(self) -> JSError {
        let ctx = self.context().clone();
        if self.is_error() {
            JSError {
                message: self.message(),
                stack: self.stack(),
            }
        } else {
            let js_value = self.into_js_value(&ctx);
            JSError {
                message: js_value.to_string_coerced().ok(),
                stack: None,
            }
        }
    }
}

/// Enters the JavaScript exception channel by throwing a value.
///
/// This is the only mechanism that should mark a value as an exception (`is_exception()`).
pub trait JSExceptionThrower: JSContextImpl {
    fn throw(&self, value: Self::Value) -> Self::Value;
}

impl<C> JSContext<C>
where
    C: JSContextImpl + JSExceptionThrower,
    C::Value: JSValueImpl,
{
    pub fn throw(&self, value: JSValue<C::Value>) -> JSValue<C::Value> {
        let raw = self.as_ref().throw(value.into_value());
        JSValue::from_raw(self, raw)
    }
}

impl<C> JSContext<C>
where
    C: JSContextImpl + JSExceptionThrower + JSErrorFactory,
    C::Value: JSObjectOps + crate::JSArrayOps,
{
    /// Throws an error built like [`JSContext::new_error_value`]: an instance of
    /// the class registered for `name`, if any.
    pub fn throw_named_error(
        &self,
        name: &str,
        message: impl AsRef<str>,
        code: Option<&str>,
    ) -> JSValue<C::Value> {
        let raw = self.create_error(name, message.as_ref(), code);
        let raw = self.as_ref().throw(raw);
        JSValue::from_raw(self, raw)
    }

    pub fn throw_syntax_error(&self, message: impl AsRef<str>) -> JSValue<C::Value> {
        self.throw_named_error("SyntaxError", message, None)
    }

    pub fn throw_type_error(&self, message: impl AsRef<str>) -> JSValue<C::Value> {
        self.throw_named_error("TypeError", message, None)
    }

    pub fn throw_reference_error(&self, message: impl AsRef<str>) -> JSValue<C::Value> {
        self.throw_named_error("ReferenceError", message, None)
    }

    pub fn throw_range_error(&self, message: impl AsRef<str>) -> JSValue<C::Value> {
        self.throw_named_error("RangeError", message, None)
    }

    pub fn throw_error(&self, message: impl AsRef<str>) -> JSValue<C::Value> {
        self.throw_named_error("Error", message, None)
    }
}

impl<V: JSObjectOps> fmt::Debug for JSException<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Exception")
            .field("message", &self.message())
            .field("stack", &self.stack())
            .finish()
    }
}

impl<V: JSObjectOps> fmt::Display for JSException<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_error() {
            "Error:".fmt(f)?;
            if let Some(message) = &self.message() {
                ' '.fmt(f)?;
                message.fmt(f)?;
            }
            if let Some(stack) = &self.stack() {
                '\n'.fmt(f)?;
                stack.fmt(f)?;
            }
        } else {
            let value = self.as_js_value().clone();
            match value.to_string_coerced() {
                Ok(text) => text.fmt(f)?,
                Err(_) => value.type_of().fmt(f)?,
            }
        }
        Ok(())
    }
}

// blanket implementing.
impl<V: JSValueImpl> crate::function::JSParameterType for JSException<V> {}
