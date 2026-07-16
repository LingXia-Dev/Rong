use crate::{
    FromJSValue, HostError, IntoJSValue, JSContext, JSObject, JSObjectOps, JSResult, JSTypeOf,
    JSValue, JSValueImpl, JSValueMapper,
};
use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;

pub struct JSArray<V: JSValueImpl>(JSObject<V>);

impl<V: JSValueImpl> Deref for JSArray<V> {
    type Target = JSObject<V>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<V: JSValueImpl> Clone for JSArray<V> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<V> IntoJSValue<V> for JSArray<V>
where
    V: JSValueImpl,
{
    fn into_js_value(self, _ctx: &JSContext<V::Context>) -> JSValue<V> {
        self.0.into_js_value()
    }
}

impl<V> FromJSValue<V> for JSArray<V>
where
    V: JSTypeOf,
{
    fn from_js_value(ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        if value.is_array() {
            JSObject::from_js_value(ctx, value).map(Self)
        } else {
            Err(HostError::not_array().into())
        }
    }
}

/// Trait for primitive JavaScript array index operations.
pub trait JSArrayOps: JSValueImpl {
    /// Create a new empty array.
    fn new_array(ctx: &Self::Context) -> Self;

    /// Get the array length.
    fn array_len(&self) -> Self;

    /// Get element at index.
    ///
    /// Returns the element value or an exception.
    fn get_index(&self, index: u32) -> Self;

    /// Set element at index.
    ///
    /// Returns `undefined` on success or an exception.
    fn set_index(&self, index: u32, value: Self) -> Self;
}

impl<V> JSArray<V>
where
    V: JSObjectOps + JSArrayOps,
{
    /// Create a new empty JavaScript array.
    pub fn new(ctx: &JSContext<V::Context>) -> JSResult<Self> {
        let value = V::new_array(ctx.as_ref());
        value.try_map(|value| Self::from_js_value(ctx, JSValue::from_raw(ctx, value)))?
    }

    /// Get the JavaScript `length` property.
    pub fn len(&self) -> JSResult<u32> {
        self.as_value().array_len().try_convert()
    }

    /// Check whether the array is empty.
    pub fn is_empty(&self) -> JSResult<bool> {
        self.len().map(|len| len == 0)
    }

    /// Get the raw JS value at the given index.
    pub fn get_value(&self, index: u32) -> JSResult<JSValue<V>> {
        let ctx = self.context();
        self.as_value()
            .get_index(index)
            .try_map(|value| JSValue::from_raw(&ctx, value))
    }

    /// Set the raw JS value at the given index.
    pub fn set_value(&self, index: u32, value: JSValue<V>) -> JSResult<()> {
        self.as_value()
            .set_index(index, value.into_value())
            .try_map(|_| ())
    }

    /// Get an optionally-present element with Rust conversion semantics.
    pub fn get_opt<T>(&self, index: u32) -> JSResult<Option<T>>
    where
        T: FromJSValue<V>,
    {
        if !self.has_index(index)? {
            return Ok(None);
        }

        let ctx = self.context();
        let value = self.get_value(index)?;
        T::from_js_value(&ctx, value).map(Some)
    }

    /// Set an element after Rust-to-JS conversion.
    pub fn set<T>(&self, index: u32, value: T) -> JSResult<()>
    where
        T: IntoJSValue<V>,
    {
        let ctx = self.context();
        self.set_value(index, value.into_js_value(&ctx))
    }

    /// Delete an array index using primitive object semantics.
    pub fn delete(&self, index: u32) -> JSResult<bool> {
        self.0.delete(index)
    }

    /// Check whether an index is present using primitive object semantics.
    pub fn has_index(&self, index: u32) -> JSResult<bool> {
        self.0.has_property(index)
    }

    /// Push a raw JS value using primitive index writes.
    pub fn push_value(&self, value: JSValue<V>) -> JSResult<u32> {
        let index = self.len()?;
        let new_len = index.checked_add(1).ok_or_else(|| {
            HostError::range_error(
                crate::error::E_OUT_OF_RANGE,
                "Maximum JavaScript array length exceeded",
            )
        })?;
        self.set_value(index, value)?;
        Ok(new_len)
    }

    /// Push a Rust value and return the new array length.
    pub fn push<T>(&self, value: T) -> JSResult<u32>
    where
        T: IntoJSValue<V>,
    {
        let ctx = self.context();
        self.push_value(value.into_js_value(&ctx))
    }

    /// Pop a raw JS value using primitive index operations.
    pub fn pop_value(&self) -> JSResult<JSValue<V>> {
        let len = self.len()?;
        let ctx = self.context();
        if len == 0 {
            return Ok(JSValue::undefined(&ctx));
        }

        let index = len - 1;
        let value = self.get_value(index)?;
        self.delete(index)?;
        self.0.set("length", index)?;
        Ok(value)
    }

    /// Pop an optionally-present element with Rust conversion semantics.
    pub fn pop_opt<T>(&self) -> JSResult<Option<T>>
    where
        T: FromJSValue<V>,
    {
        if self.is_empty()? {
            return Ok(None);
        }

        let ctx = self.context();
        let value = self.pop_value()?;
        T::from_js_value(&ctx, value).map(Some)
    }

    /// Iterate over typed values in `[0, length)`.
    pub fn iter<T>(&self) -> JSResult<ArrayIter<V, T>>
    where
        T: FromJSValue<V>,
    {
        Ok(ArrayIter {
            array: self.clone(),
            index: 0,
            count: self.len()?,
            marker: PhantomData,
        })
    }

    /// Iterate over raw values in `[0, length)`.
    pub fn iter_values(&self) -> JSResult<ArrayValueIter<V>> {
        Ok(ArrayValueIter {
            array: self.clone(),
            index: 0,
            count: self.len()?,
        })
    }

    /// Iterate over present values only, skipping holes.
    pub fn iter_present<T>(&self) -> JSResult<ArrayPresentIter<V, T>>
    where
        T: FromJSValue<V>,
    {
        Ok(ArrayPresentIter {
            array: self.clone(),
            index: 0,
            count: self.len()?,
            marker: PhantomData,
        })
    }

    /// Construct a JSArray from a JSObject if it is an array.
    pub fn from_object(obj: JSObject<V>) -> Option<Self> {
        if obj.as_value().is_array() {
            Some(Self(obj))
        } else {
            None
        }
    }
}

/// Iterator over typed JavaScript values in an array.
pub struct ArrayIter<V, T>
where
    V: JSObjectOps + JSArrayOps,
    T: FromJSValue<V>,
{
    array: JSArray<V>,
    index: u32,
    count: u32,
    marker: PhantomData<T>,
}

impl<V, T> Iterator for ArrayIter<V, T>
where
    V: JSObjectOps + JSArrayOps,
    T: FromJSValue<V>,
{
    type Item = JSResult<T>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.count {
            let ctx = self.array.context();
            let result = self
                .array
                .get_value(self.index)
                .and_then(|value| T::from_js_value(&ctx, value));
            self.index += 1;
            Some(result)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl<V, T> ExactSizeIterator for ArrayIter<V, T>
where
    V: JSObjectOps + JSArrayOps,
    T: FromJSValue<V>,
{
    fn len(&self) -> usize {
        (self.count - self.index) as usize
    }
}

/// Iterator over raw JavaScript values in an array.
pub struct ArrayValueIter<V>
where
    V: JSObjectOps + JSArrayOps,
{
    array: JSArray<V>,
    index: u32,
    count: u32,
}

impl<V> Iterator for ArrayValueIter<V>
where
    V: JSObjectOps + JSArrayOps,
{
    type Item = JSResult<JSValue<V>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.count {
            let result = self.array.get_value(self.index);
            self.index += 1;
            Some(result)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl<V> ExactSizeIterator for ArrayValueIter<V>
where
    V: JSObjectOps + JSArrayOps,
{
    fn len(&self) -> usize {
        (self.count - self.index) as usize
    }
}

/// Iterator over present JavaScript array entries converted into Rust values.
pub struct ArrayPresentIter<V, T>
where
    V: JSObjectOps + JSArrayOps,
    T: FromJSValue<V>,
{
    array: JSArray<V>,
    index: u32,
    count: u32,
    marker: PhantomData<T>,
}

impl<V, T> Iterator for ArrayPresentIter<V, T>
where
    V: JSObjectOps + JSArrayOps,
    T: FromJSValue<V>,
{
    type Item = JSResult<T>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.index < self.count {
            let index = self.index;
            self.index += 1;

            let has_index = match self.array.has_index(index) {
                Ok(has_index) => has_index,
                Err(err) => return Some(Err(err)),
            };

            if has_index {
                let ctx = self.array.context();
                let value = match self.array.get_value(index) {
                    Ok(value) => value,
                    Err(err) => return Some(Err(err)),
                };
                return Some(T::from_js_value(&ctx, value));
            }
        }

        None
    }
}

/// Converts a Rust Vec into a JavaScript array.
impl<V, T> IntoJSValue<V> for Vec<T>
where
    V: JSObjectOps + JSArrayOps,
    T: IntoJSValue<V>,
{
    fn into_js_value(self, ctx: &JSContext<V::Context>) -> JSValue<V> {
        let array = JSArray::new(ctx).unwrap();
        for item in self {
            array.push(item).expect("Failed to push value into array");
        }
        <JSArray<V> as IntoJSValue<V>>::into_js_value(array, ctx)
    }
}

/// Converts a JavaScript array to a Rust Vec using dense array semantics.
impl<V, T> FromJSValue<V> for Vec<T>
where
    V: JSTypeOf,
    V: JSObjectOps + JSArrayOps,
    T: FromJSValue<V>,
{
    fn from_js_value(ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        if value.is_array() {
            let array = JSArray::from_js_value(ctx, value)?;
            array.iter::<T>()?.collect::<JSResult<Vec<_>>>()
        } else {
            Err(HostError::not_array().into())
        }
    }
}

// blanket implementing.
impl<V: JSValueImpl> crate::function::JSParameterType for JSArray<V> {}

impl<V> fmt::Display for JSArray<V>
where
    V: JSTypeOf + crate::JSValueConversion,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.deref().fmt(f)
    }
}

impl<V> fmt::Debug for JSArray<V>
where
    V: JSTypeOf + crate::JSValueConversion,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "JSArray({})", self)
    }
}
