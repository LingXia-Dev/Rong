//! Reads the text of a thrown value without running any script.

use crate::{JSContext, JSContextImpl, JSObjectOps, JSValue, JSValueImpl};

/// How far up a prototype chain a thrown object is followed.
const MAX_CHAIN: usize = 16;

/// Built-ins captured when a context is created, before any script runs in
/// it, so describing a thrown value never looks anything up that a script
/// could have replaced.
///
/// Called on an object that is not a Proxy, each of the three functions runs
/// engine code only. An accessor is recognised by its descriptor and never
/// called, and a Proxy anywhere on the way ends the read.
pub(crate) struct ThrownReader<V: JSValueImpl> {
    get_own_property_descriptor: V,
    has_own_property: V,
    get_prototype_of: V,
    error_prototype: V,
}

impl<V: JSObjectOps> ThrownReader<V> {
    pub(crate) fn capture(ctx: &JSContext<V::Context>) -> Option<Self> {
        let global = ctx.global();
        let object = global.get::<_, crate::JSObject<V>>("Object").ok()?;
        let function = |owner: &crate::JSObject<V>, name: &str| {
            let value = owner.get::<_, JSValue<V>>(name).ok()?;
            value.is_function().then(|| value.into_value())
        };
        let object_prototype = object.get::<_, crate::JSObject<V>>("prototype").ok()?;
        let error_prototype = global
            .get::<_, crate::JSObject<V>>("Error")
            .ok()?
            .get::<_, crate::JSObject<V>>("prototype")
            .ok()?;
        Some(Self {
            get_own_property_descriptor: function(&object, "getOwnPropertyDescriptor")?,
            has_own_property: function(&object_prototype, "hasOwnProperty")?,
            get_prototype_of: function(&object, "getPrototypeOf")?,
            error_prototype: error_prototype.into_value(),
        })
    }

    /// `name: message` of a thrown `Error`, or a thrown string, number or
    /// boolean. `None` for anything else, and for an `Error` that cannot be
    /// read without running script.
    pub(crate) fn describe(&self, ctx: &JSContext<V::Context>, value: &V) -> Option<String> {
        if value.is_string() {
            return Self::string(value.clone());
        }
        if value.is_number() || value.is_boolean() {
            return value.clone().try_into().ok();
        }
        if !value.is_object() {
            return None;
        }

        // The object and its prototypes, up to `Error.prototype`.
        let mut chain = Vec::new();
        let mut current = value.clone();
        loop {
            if chain.len() == MAX_CHAIN || current.is_proxy() {
                return None;
            }
            let is_error_prototype = current == self.error_prototype;
            chain.push(current.clone());
            if is_error_prototype {
                break;
            }
            current = self.call(ctx, &self.get_prototype_of, None, &[current])?;
            if !current.is_object() {
                // The chain ended without reaching `Error.prototype`.
                return None;
            }
        }

        let message = self.string_property(ctx, &chain, "message");
        let name = self.string_property(ctx, &chain, "name");
        match (name, message) {
            (Some(name), Some(message)) => Some(format!("{name}: {message}")),
            (name, message) => message.or(name),
        }
    }

    /// The first `key` found along `chain`, if it is a data property holding a
    /// non-empty string.
    fn string_property(
        &self,
        ctx: &JSContext<V::Context>,
        chain: &[V],
        key: &str,
    ) -> Option<String> {
        let key: V = (ctx.as_ref(), key).into();
        let value_key: V = (ctx.as_ref(), "value").into();
        for object in chain {
            let descriptor = self.call(
                ctx,
                &self.get_own_property_descriptor,
                None,
                &[object.clone(), key.clone()],
            )?;
            if !descriptor.is_object() {
                continue;
            }
            // The descriptor is a fresh ordinary object. An accessor's has no
            // own `value`, and reading it anyway would reach `Object.prototype`.
            let is_data: bool = self
                .call(
                    ctx,
                    &self.has_own_property,
                    Some(descriptor.clone()),
                    std::slice::from_ref(&value_key),
                )?
                .try_into()
                .ok()?;
            if !is_data {
                return None;
            }
            return Self::string(descriptor.get_property(value_key).ok()??);
        }
        None
    }

    fn string(value: V) -> Option<String> {
        if !value.is_string() {
            return None;
        }
        let text: String = value.try_into().ok()?;
        (!text.is_empty()).then_some(text)
    }

    /// Calls a captured built-in directly: no promise jobs run afterwards, as
    /// they would after a host call into script.
    fn call(
        &self,
        ctx: &JSContext<V::Context>,
        function: &V,
        this: Option<V>,
        argv: &[V],
    ) -> Option<V> {
        let this = this.unwrap_or_else(|| V::create_undefined(ctx.as_ref()));
        let result = ctx.as_ref().call(function, this, argv);
        (!result.is_exception()).then_some(result)
    }
}
