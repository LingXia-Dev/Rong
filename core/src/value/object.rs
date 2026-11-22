use crate::{
    FromJSValue, HostError, JSContext, JSFunc, JSResult, JSTypeOf, JSValue, JSValueConversion,
    JSValueImpl, JsonToJSValue, RongJSError,
};
use std::fmt;
use std::ops::Deref;

mod property;
pub use property::{PropertyAttributes, PropertyDescriptor, PropertyKey};

use super::IntoJSValue;

#[derive(Hash, PartialEq, Eq)]
pub struct JSObject<V: JSValueImpl>(JSValue<V>);

impl<V: JSValueImpl> Clone for JSObject<V> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<V> From<JSValue<V>> for JSObject<V>
where
    V: JSValueImpl,
{
    fn from(v: JSValue<V>) -> Self {
        JSObject(v)
    }
}

impl<V> FromJSValue<V> for JSObject<V>
where
    V: JSTypeOf,
{
    fn from_js_value(_ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        if value.is_object() {
            Ok(value.into())
        } else {
            Err(HostError::type_mismatch("an object", value.type_of().to_string()).into())
        }
    }
}

impl<V: JSValueImpl> Deref for JSObject<V> {
    type Target = JSValue<V>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<V> IntoJSValue<V> for JSObject<V>
where
    V: JSValueImpl,
{
    fn into_js_value(self, _ctx: &JSContext<V::Context>) -> JSValue<V> {
        self.0
    }
}

pub trait JSObjectOps: JSValueConversion + JSTypeOf {
    /// Creates a new empty object in the given context.
    /// Returns EXCEPTION if creation fails.
    fn new_object(ctx: &Self::Context) -> Self;

    /// Creates a new instance using the given constructor and private data.
    /// Returns EXCEPTION if instantiation fails.
    ///
    /// # Arguments
    /// * `ctx` - The JavaScript context
    /// * `constructor` - The constructor function (JS Class)
    /// * `data` - Pointer to private data to store in the object
    fn make_instance(ctx: &Self::Context, constructor: Self, data: *mut ()) -> Self;

    /// Checks if this object is an instance of the given constructor.
    fn instance_of(&self, constructor: Self) -> bool;

    /// Gets the private data stored in the object.
    /// Returns a raw pointer to the opaque data.
    fn get_opaque(&self) -> *mut ();

    /// Deletes a property from the object.
    ///
    /// Returns `Ok(true)` when the property was deleted, `Ok(false)` when the
    /// property was not present or could not be deleted, and `Err(thrown)` when
    /// the engine raised a JavaScript exception.
    fn del_property(&self, key: Self) -> Result<bool, Self>;

    /// Checks if the object has the specified property.
    ///
    /// Returns `Ok(bool)` on success or `Err(thrown)` when the engine raised a
    /// JavaScript exception.
    fn has_property(&self, key: Self) -> Result<bool, Self>;

    /// Sets a property on the object with the given value.
    ///
    /// Returns `Err(thrown)` when the engine raised a JavaScript exception.
    fn set_property(&self, key: Self, value: Self) -> Result<(), Self>;

    /// Sets the prototype of the object.
    /// Returns true if the prototype was successfully set.
    fn set_prototype(&self, prototype: Self) -> bool;

    /// Defines a property with the given attributes and optional getter/setter.
    /// Returns true if the property was successfully defined.
    ///
    /// # Arguments
    /// * `key` - The property key
    /// * `value` - The property value
    /// * `getter` - Optional getter function
    /// * `setter` - Optional setter function
    /// * `attributes` - Property attributes (writable, enumerable, configurable)
    fn define_property(
        &self,
        key: Self,
        value: Self,
        getter: Self,
        setter: Self,
        attributes: PropertyAttributes,
    ) -> Result<(), Self>;

    /// Gets the value produced by a property lookup.
    ///
    /// Successful lookups must return `undefined` as a value, not as `None`.
    /// Use `has_property()` when you need to distinguish a missing property
    /// from a property whose value is `undefined`.
    fn get_property(&self, key: Self) -> Result<Option<Self>, Self>;

    /// Gets the names of all enumerable properties of the object.
    /// Returns `Err(thrown)` when the operation fails.
    fn get_own_property_names(&self) -> Result<Vec<Self>, Self>;
}

impl<V> JSObject<V>
where
    V: JSObjectOps,
{
    /// new a general object
    pub fn new(ctx: &JSContext<V::Context>) -> Self {
        let value = V::new_object(ctx.as_ref());
        JSObject::from_js_value(ctx, JSValue::from_raw(ctx, value)).unwrap()
    }

    /// Creates a JSObject from a raw JSValue and context
    pub fn from_raw(ctx: &JSContext<V::Context>, value: V) -> Self {
        JSValue::from_raw(ctx, value).into()
    }

    /// Creates a JSObject from a JSON string
    ///
    /// # Example
    /// ```rust,no_run
    /// use rong_core::prelude::*;
    ///
    /// fn demo<E: JSEngine + 'static>() -> JSResult<()> {
    ///     let runtime = E::runtime();
    ///     let ctx = runtime.context();
    ///
    ///     let json = r#"{"name":"John","age":30}"#;
    ///     let _obj = JSObject::<E::Value>::from_json_string(&ctx, json)?;
    ///     Ok(())
    /// }
    /// ```
    pub fn from_json_string(ctx: &JSContext<V::Context>, json: &str) -> JSResult<Self> {
        let v = json.json_to_js_value(ctx)?;
        Ok(JSObject(v))
    }

    fn thrown_error(ctx: &JSContext<V::Context>, thrown: V) -> RongJSError {
        RongJSError::from_thrown_value(JSValue::from_raw(ctx, thrown))
    }

    /// Convert JSObject to JSON string using JavaScript's JSON.stringify
    pub fn to_json_string(&self) -> JSResult<String> {
        let ctx = self.context();

        // Get the global JSON object
        let json = ctx.global().get::<_, JSObject<V>>("JSON")?;

        // Get the stringify function
        let stringify = json.get::<_, JSFunc<V>>("stringify")?;

        // Call stringify with this object
        stringify.call::<_, String>(None, (self.clone(),))
    }

    pub fn set<'a, K, KV>(&'a self, k: K, kv: KV) -> JSResult<&'a Self>
    where
        K: Into<PropertyKey<'a, V>>,
        KV: IntoJSValue<V>,
    {
        let ctx = &self.context();
        let key = k.into().into_value(ctx);
        self.as_value()
            .set_property(key, kv.into_js_value(ctx).into_value())
            .map_err(|thrown| Self::thrown_error(ctx, thrown))?;
        Ok(self)
    }

    pub fn define_property<'a, K>(
        &'a self,
        k: K,
        descriptor: PropertyDescriptor<V>,
    ) -> JSResult<&'a Self>
    where
        K: Into<PropertyKey<'a, V>>,
    {
        descriptor.define_on(self, k)?;
        Ok(self)
    }

    pub fn delete<'a, K>(&'a self, k: K) -> JSResult<bool>
    where
        K: Into<PropertyKey<'a, V>>,
    {
        let ctx = self.context();
        let key = k.into().into_value(&ctx);
        self.as_value()
            .del_property(key)
            .map_err(|thrown| Self::thrown_error(&ctx, thrown))
    }

    pub fn has_property<'a, K>(&self, k: K) -> JSResult<bool>
    where
        K: Into<PropertyKey<'a, V>>,
    {
        let ctx = self.context();
        let key = k.into().into_value(&ctx);
        self.as_value()
            .has_property(key)
            .map_err(|thrown| Self::thrown_error(&ctx, thrown))
    }

    pub fn get<'a, K, T>(&'a self, k: K) -> JSResult<T>
    where
        K: Into<PropertyKey<'a, V>>,
        T: FromJSValue<V>,
    {
        let ctx = &self.context();
        let key = k.into();
        let kv = key.clone().into_value(ctx);
        let value = self
            .as_value()
            .get_property(kv.clone())
            .map_err(|thrown| Self::thrown_error(ctx, thrown))?
            .map(|value| JSValue::from_raw(ctx, value));

        let value = match value {
            Some(value) if !value.is_undefined() => value,
            Some(value) => {
                if !self
                    .as_value()
                    .has_property(kv)
                    .map_err(|thrown| Self::thrown_error(ctx, thrown))?
                {
                    return Err(HostError::property_not_found(key).into());
                }
                value
            }
            None => {
                if !self
                    .as_value()
                    .has_property(kv)
                    .map_err(|thrown| Self::thrown_error(ctx, thrown))?
                {
                    return Err(HostError::property_not_found(key).into());
                }
                JSValue::undefined(ctx)
            }
        };

        T::from_js_value(ctx, value)
    }

    pub fn get_opt<'a, K, T>(&'a self, k: K) -> JSResult<Option<T>>
    where
        K: Into<PropertyKey<'a, V>>,
        T: FromJSValue<V>,
    {
        let ctx = &self.context();
        let key = k.into().into_value(ctx);
        let value = self
            .as_value()
            .get_property(key.clone())
            .map_err(|thrown| Self::thrown_error(ctx, thrown))?
            .map(|value| JSValue::from_raw(ctx, value));

        let value = match value {
            Some(value) if !value.is_undefined() => value,
            Some(value) => {
                if !self
                    .as_value()
                    .has_property(key)
                    .map_err(|thrown| Self::thrown_error(ctx, thrown))?
                {
                    return Ok(None);
                }
                value
            }
            None => {
                if !self
                    .as_value()
                    .has_property(key)
                    .map_err(|thrown| Self::thrown_error(ctx, thrown))?
                {
                    return Ok(None);
                }
                JSValue::undefined(ctx)
            }
        };

        T::from_js_value(ctx, value).map(Some)
    }
}

impl<V: JSValueImpl> JSObject<V> {
    /// Converts the JSObject into its underlying/raw JSValue implementation
    pub fn into_value(self) -> V {
        self.0.into_value()
    }

    /// Converts the JSObject into a JSValue
    pub fn into_js_value(self) -> JSValue<V> {
        self.0
    }

    pub fn as_js_value(&self) -> &JSValue<V> {
        &self.0
    }

    /// Returns a mutable reference to the underlying/raw JSValue
    pub fn as_mut_value(&mut self) -> &mut V {
        &mut self.0.inner
    }
}

pub struct Entry<V: JSValueImpl> {
    key: JSValue<V>,
    value: JSValue<V>,
}

impl<V: JSValueImpl> Entry<V> {
    pub fn key(&self) -> &JSValue<V> {
        &self.key
    }

    pub fn value(&self) -> &JSValue<V> {
        &self.value
    }

    pub fn into_tuple(self) -> (JSValue<V>, JSValue<V>) {
        (self.key, self.value)
    }

    pub fn try_into<K, T>(self) -> JSResult<(K, T)>
    where
        K: FromJSValue<V>,
        T: FromJSValue<V>,
    {
        let ctx = self.key.context();
        Ok((
            K::from_js_value(&ctx, self.key)?,
            T::from_js_value(&ctx, self.value)?,
        ))
    }
}

impl<V> JSObject<V>
where
    V: JSObjectOps,
{
    /// Returns an iterator over the object's own enumerable string-keyed property [key, value] pairs.
    pub fn entries(&self) -> JSResult<Vec<Entry<V>>> {
        let ctx = &self.context();
        let mut entries = Vec::new();

        // Get all enumerable property names
        let keys = self.own_keys()?;

        // Iterate through property names to get corresponding values
        for key in keys {
            if let Some(value) = self
                .as_value()
                .get_property(key.clone())
                .map_err(|thrown| Self::thrown_error(ctx, thrown))?
            {
                entries.push(Entry {
                    key: JSValue::from_raw(ctx, key),
                    value: JSValue::from_raw(ctx, value),
                });
            }
        }

        Ok(entries)
    }

    /// Returns entries with converted types
    pub fn entries_as<K, V2>(&self) -> JSResult<Vec<(K, V2)>>
    where
        K: FromJSValue<V>,
        V2: FromJSValue<V>,
    {
        self.entries()?
            .into_iter()
            .map(|entry| entry.try_into::<K, V2>())
            .collect()
    }

    /// Returns an array of a given object's own enumerable property names
    pub fn own_keys(&self) -> JSResult<Vec<V>> {
        let ctx = self.context();
        self.as_value()
            .get_own_property_names()
            .map_err(|thrown| Self::thrown_error(&ctx, thrown))
    }

    /// Returns an iterator over the object's own enumerable string-keyed property values.
    pub fn values(&self) -> JSResult<impl Iterator<Item = JSValue<V>> + '_> {
        Ok(self.entries()?.into_iter().map(|entry| entry.value))
    }

    /// Returns values with converted type
    pub fn values_as<T>(&self) -> JSResult<Vec<T>>
    where
        T: FromJSValue<V>,
    {
        let ctx = &self.context();
        self.values()?.map(|v| T::from_js_value(ctx, v)).collect()
    }

    /// Returns an iterator over the object's own enumerable string-keyed property names.
    pub fn keys(&self) -> JSResult<impl Iterator<Item = JSValue<V>> + '_> {
        Ok(self.entries()?.into_iter().map(|entry| entry.key))
    }

    /// Returns keys with converted type
    pub fn keys_as<K>(&self) -> JSResult<Vec<K>>
    where
        K: FromJSValue<V>,
    {
        let ctx = &self.context();
        self.keys()?.map(|k| K::from_js_value(ctx, k)).collect()
    }
}

// blanket implementing.
impl<V: JSValueImpl> crate::function::JSParameterType for JSObject<V> {}

impl<V> fmt::Display for JSObject<V>
where
    V: JSTypeOf + JSValueConversion,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Delegate to JSValue's Display implementation through Deref
        self.deref().fmt(f)
    }
}

impl<V> fmt::Debug for JSObject<V>
where
    V: JSTypeOf + JSValueConversion,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "JSObject({})", self)
    }
}
