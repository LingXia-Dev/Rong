use crate::{
    FromJSValue, HostError, IntoJSValue, JSContext, JSObject, JSObjectOps, JSResult, JSTypeOf,
    JSValue, JSValueImpl, JSValueMapper, TypedArrayElement,
};

use std::ops::{Deref, DerefMut};

pub struct JSArrayBuffer<V: JSValueImpl> {
    inner: JSObject<V>,
}

impl<V: JSValueImpl> Deref for JSArrayBuffer<V> {
    type Target = JSObject<V>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<V: JSValueImpl> DerefMut for JSArrayBuffer<V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<V: JSValueImpl> Clone for JSArrayBuffer<V> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<V> IntoJSValue<V> for JSArrayBuffer<V>
where
    V: JSValueImpl,
{
    fn into_js_value(self, _ctx: &JSContext<V::Context>) -> JSValue<V> {
        self.inner.into_js_value()
    }
}

impl<V> FromJSValue<V> for JSArrayBuffer<V>
where
    V: JSTypeOf,
{
    fn from_js_value(ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        if value.is_array_buffer() {
            Ok(Self {
                inner: JSObject::from_js_value(ctx, value)?,
            })
        } else {
            Err(HostError::type_mismatch("an ArrayBuffer", value.type_of().to_string()).into())
        }
    }
}

/// Trait for JavaScript array buffer operations.
pub trait JSArrayBufferOps: JSValueImpl {
    /// Create an ArrayBuffer by copying existing data.
    fn from_bytes(ctx: &Self::Context, bytes: &[u8]) -> Self;

    /// Create an ArrayBuffer from an existing Vec without copying when possible.
    fn from_vec(ctx: &Self::Context, vec: Vec<u8>) -> Self;

    /// Get the byte length of the ArrayBuffer.
    fn length(&self) -> usize;

    /// Get a safe slice view of the ArrayBuffer's data.
    fn as_slice(&self) -> &[u8];

    /// Get a mutable slice view of the ArrayBuffer's data.
    fn as_mut_slice(&mut self) -> &mut [u8];
}

impl<V> JSArrayBuffer<V>
where
    V: JSObjectOps + JSArrayBufferOps,
{
    /// Create a new ArrayBuffer by copying the provided bytes.
    pub fn from_bytes(ctx: &JSContext<V::Context>, bytes: &[u8]) -> JSResult<Self> {
        let value = V::from_bytes(ctx.as_ref(), bytes);
        value.try_map(|value| Self::from_js_value(ctx, JSValue::from_raw(ctx, value)))?
    }

    /// Create a new ArrayBuffer from owned bytes.
    pub fn from_bytes_owned<B: Into<Vec<u8>>>(
        ctx: &JSContext<V::Context>,
        data: B,
    ) -> JSResult<Self> {
        let value = V::from_vec(ctx.as_ref(), data.into());
        value.try_map(|value| Self::from_js_value(ctx, JSValue::from_raw(ctx, value)))?
    }

    /// Get the byte length of the ArrayBuffer.
    pub fn len(&self) -> usize {
        self.as_value().length()
    }

    /// Check if the ArrayBuffer is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get a safe slice view of the ArrayBuffer's data.
    pub fn as_slice(&self) -> &[u8] {
        self.as_value().as_slice()
    }

    /// Get a reference to the ArrayBuffer's raw bytes.
    pub fn as_bytes(&self) -> &[u8] {
        self.as_slice()
    }

    /// Get a mutable slice view of the ArrayBuffer's data.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        self.as_mut_value().as_mut_slice()
    }

    /// Get a slice of the ArrayBuffer from start to end.
    pub fn slice(&self, start: usize, end: usize) -> &[u8] {
        &self.as_slice()[start..end]
    }

    /// Copy the contents of the ArrayBuffer into a new Vec.
    pub fn to_vec(&self) -> Vec<u8> {
        self.as_slice().to_vec()
    }

    /// Compute how many `T` elements this buffer can represent when aligned.
    pub fn element_count<T>(&self) -> JSResult<usize>
    where
        T: TypedArrayElement,
    {
        if !self.len().is_multiple_of(T::BYTES_PER_ELEMENT) {
            return Err(HostError::typed_array_alignment_error().into());
        }

        Ok(self.len() / T::BYTES_PER_ELEMENT)
    }

    /// Validate if the given byte offset is properly aligned for `T`.
    pub fn validate_alignment<T>(&self, offset: usize) -> bool
    where
        T: TypedArrayElement,
    {
        offset.is_multiple_of(T::BYTES_PER_ELEMENT)
    }

    /// Construct a JSArrayBuffer from a JSObject if it is an ArrayBuffer.
    pub fn from_object(obj: JSObject<V>) -> Option<Self> {
        if obj.as_value().is_array_buffer() {
            Some(Self { inner: obj })
        } else {
            None
        }
    }
}

// blanket implementing.
impl<V: JSValueImpl> crate::function::JSParameterType for JSArrayBuffer<V> {}
