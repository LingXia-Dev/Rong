use crate::{
    FromJSValue, HostError, IntoJSValue, JSArrayBuffer, JSArrayBufferOps, JSContext, JSObject,
    JSObjectOps, JSResult, JSTypeOf, JSValue, JSValueImpl, JSValueMapper,
};
use std::marker::PhantomData;
use std::ops::Deref;

/// Represents the different kinds of TypedArrays available in JavaScript.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JSTypedArrayKind {
    Int8,
    Uint8,
    Uint8Clamped,
    Int16,
    Uint16,
    Int32,
    Uint32,
    BigInt64,
    BigUint64,
    Float32,
    Float64,
}

fn typed_array_name(kind: JSTypedArrayKind) -> &'static str {
    match kind {
        JSTypedArrayKind::Int8 => "Int8Array",
        JSTypedArrayKind::Uint8 => "Uint8Array",
        JSTypedArrayKind::Uint8Clamped => "Uint8ClampedArray",
        JSTypedArrayKind::Int16 => "Int16Array",
        JSTypedArrayKind::Uint16 => "Uint16Array",
        JSTypedArrayKind::Int32 => "Int32Array",
        JSTypedArrayKind::Uint32 => "Uint32Array",
        JSTypedArrayKind::BigInt64 => "BigInt64Array",
        JSTypedArrayKind::BigUint64 => "BigUint64Array",
        JSTypedArrayKind::Float32 => "Float32Array",
        JSTypedArrayKind::Float64 => "Float64Array",
    }
}

/// Trait for types that can be used as compile-time typed array view markers.
pub trait TypedArrayElement: Sized {
    /// Number of bytes per element.
    const BYTES_PER_ELEMENT: usize;
    /// The corresponding TypedArray kind.
    const TYPE: JSTypedArrayKind;
}

/// Marker type for `Uint8ClampedArray`.
pub struct Uint8Clamped;

impl TypedArrayElement for i8 {
    const BYTES_PER_ELEMENT: usize = 1;
    const TYPE: JSTypedArrayKind = JSTypedArrayKind::Int8;
}

impl TypedArrayElement for u8 {
    const BYTES_PER_ELEMENT: usize = 1;
    const TYPE: JSTypedArrayKind = JSTypedArrayKind::Uint8;
}

impl TypedArrayElement for Uint8Clamped {
    const BYTES_PER_ELEMENT: usize = 1;
    const TYPE: JSTypedArrayKind = JSTypedArrayKind::Uint8Clamped;
}

impl TypedArrayElement for i16 {
    const BYTES_PER_ELEMENT: usize = 2;
    const TYPE: JSTypedArrayKind = JSTypedArrayKind::Int16;
}

impl TypedArrayElement for u16 {
    const BYTES_PER_ELEMENT: usize = 2;
    const TYPE: JSTypedArrayKind = JSTypedArrayKind::Uint16;
}

impl TypedArrayElement for i32 {
    const BYTES_PER_ELEMENT: usize = 4;
    const TYPE: JSTypedArrayKind = JSTypedArrayKind::Int32;
}

impl TypedArrayElement for u32 {
    const BYTES_PER_ELEMENT: usize = 4;
    const TYPE: JSTypedArrayKind = JSTypedArrayKind::Uint32;
}

impl TypedArrayElement for f32 {
    const BYTES_PER_ELEMENT: usize = 4;
    const TYPE: JSTypedArrayKind = JSTypedArrayKind::Float32;
}

impl TypedArrayElement for f64 {
    const BYTES_PER_ELEMENT: usize = 8;
    const TYPE: JSTypedArrayKind = JSTypedArrayKind::Float64;
}

impl TypedArrayElement for i64 {
    const BYTES_PER_ELEMENT: usize = 8;
    const TYPE: JSTypedArrayKind = JSTypedArrayKind::BigInt64;
}

impl TypedArrayElement for u64 {
    const BYTES_PER_ELEMENT: usize = 8;
    const TYPE: JSTypedArrayKind = JSTypedArrayKind::BigUint64;
}

impl JSTypedArrayKind {
    /// Get the number of bytes per element for this type.
    pub fn bytes_per_element(&self) -> usize {
        match self {
            JSTypedArrayKind::Int8 | JSTypedArrayKind::Uint8 | JSTypedArrayKind::Uint8Clamped => 1,
            JSTypedArrayKind::Int16 | JSTypedArrayKind::Uint16 => 2,
            JSTypedArrayKind::Int32 | JSTypedArrayKind::Uint32 | JSTypedArrayKind::Float32 => 4,
            JSTypedArrayKind::BigInt64
            | JSTypedArrayKind::BigUint64
            | JSTypedArrayKind::Float64 => 8,
        }
    }
}

pub struct AnyJSTypedArray<V: JSValueImpl>(JSObject<V>);

impl<V: JSValueImpl> Clone for AnyJSTypedArray<V> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<V: JSValueImpl> Deref for AnyJSTypedArray<V> {
    type Target = JSObject<V>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<V> FromJSValue<V> for AnyJSTypedArray<V>
where
    V: JSTypeOf + JSTypedArrayOps,
{
    fn from_js_value(ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        if value.is_object() && value.as_value().get_kind().is_some() {
            JSObject::from_js_value(ctx, value).map(Self)
        } else {
            Err(HostError::type_mismatch("a typed array", value.type_of().to_string()).into())
        }
    }
}

impl<V> IntoJSValue<V> for AnyJSTypedArray<V>
where
    V: JSValueImpl,
{
    fn into_js_value(self, _ctx: &JSContext<V::Context>) -> JSValue<V> {
        self.0.into_js_value()
    }
}

pub struct JSTypedArray<V: JSValueImpl, T: TypedArrayElement = u8> {
    inner: AnyJSTypedArray<V>,
    marker: PhantomData<T>,
}

impl<V: JSValueImpl, T: TypedArrayElement> Clone for JSTypedArray<V, T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            marker: PhantomData,
        }
    }
}

impl<V: JSValueImpl, T: TypedArrayElement> Deref for JSTypedArray<V, T> {
    type Target = AnyJSTypedArray<V>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<V, T> FromJSValue<V> for JSTypedArray<V, T>
where
    V: JSTypeOf + JSTypedArrayOps,
    T: TypedArrayElement,
{
    fn from_js_value(ctx: &JSContext<V::Context>, value: JSValue<V>) -> JSResult<Self> {
        let inner = AnyJSTypedArray::from_js_value(ctx, value)?;
        Self::from_any(inner)
    }
}

impl<V, T> IntoJSValue<V> for JSTypedArray<V, T>
where
    V: JSValueImpl,
    T: TypedArrayElement,
{
    fn into_js_value(self, ctx: &JSContext<V::Context>) -> JSValue<V> {
        self.inner.into_js_value(ctx)
    }
}

impl<V, T> JSTypedArray<V, T>
where
    V: JSTypedArrayOps,
    T: TypedArrayElement,
{
    pub fn from_any(inner: AnyJSTypedArray<V>) -> JSResult<Self> {
        let actual = inner.kind();
        if actual != T::TYPE {
            let expected = typed_array_name(T::TYPE);
            let article = if expected.starts_with("Int") {
                "an"
            } else {
                "a"
            };
            return Err(HostError::type_mismatch(
                format!("{article} {expected}"),
                typed_array_name(actual),
            )
            .into());
        }

        Ok(Self {
            inner,
            marker: PhantomData,
        })
    }
}

/// Trait for JavaScript typed array operations.
pub trait JSTypedArrayOps: JSValueImpl {
    /// Create a new typed array from an existing array buffer.
    fn from_array_buffer(
        ctx: &Self::Context,
        kind: JSTypedArrayKind,
        buffer: Self,
        byte_offset: usize,
        length: Option<usize>,
    ) -> Self;

    /// Get the kind of typed array.
    fn get_kind(&self) -> Option<JSTypedArrayKind>;

    /// Get the underlying array buffer.
    fn get_array_buffer(&self) -> Option<Self>;

    /// Get the byte offset into the array buffer.
    fn get_byte_offset(&self) -> usize;

    /// Get the length of the typed array (in elements).
    fn get_length(&self) -> usize;

    /// Get the byte length of the typed array.
    fn get_byte_length(&self) -> usize;
}

impl<V> AnyJSTypedArray<V>
where
    V: JSTypedArrayOps,
{
    /// Get the kind of typed array.
    pub fn kind(&self) -> JSTypedArrayKind {
        self.as_value().get_kind().expect("Invalid typed array")
    }

    /// Get the byte offset into the array buffer.
    pub fn byte_offset(&self) -> usize {
        self.as_value().get_byte_offset()
    }

    /// Get the length of the typed array (in elements).
    pub fn len(&self) -> usize {
        self.as_value().get_length()
    }

    /// Check if the typed array is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get the byte length of the typed array.
    pub fn byte_length(&self) -> usize {
        self.as_value().get_byte_length()
    }

    /// Get the number of bytes per element.
    pub fn bytes_per_element(&self) -> usize {
        self.kind().bytes_per_element()
    }

    /// Construct a dynamic typed array from a JSObject if it is a TypedArray.
    pub fn from_object(obj: JSObject<V>) -> Option<Self> {
        if obj.as_value().get_kind().is_some() {
            Some(Self(obj))
        } else {
            None
        }
    }
}

impl<V> AnyJSTypedArray<V>
where
    V: JSObjectOps + JSTypedArrayOps + JSArrayBufferOps,
{
    /// Create a dynamically-kinded typed array view from an ArrayBuffer.
    pub fn from_array_buffer(
        ctx: &JSContext<V::Context>,
        kind: JSTypedArrayKind,
        buffer: JSArrayBuffer<V>,
        byte_offset: usize,
        length: Option<usize>,
    ) -> JSResult<Self> {
        let length = resolve_typed_array_length(kind, &buffer, byte_offset, length)?;

        let buffer_value =
            <JSArrayBuffer<V> as IntoJSValue<V>>::into_js_value(buffer, ctx).into_value();
        let value =
            V::from_array_buffer(ctx.as_ref(), kind, buffer_value, byte_offset, Some(length));
        value.try_map(|value| Self::from_js_value(ctx, JSValue::from_raw(ctx, value)))?
    }

    /// Get the underlying array buffer.
    pub fn buffer(&self) -> JSResult<JSArrayBuffer<V>> {
        let buffer = self
            .as_value()
            .get_array_buffer()
            .ok_or_else(|| -> crate::RongJSError { HostError::not_array_buffer().into() })?;
        let ctx = self.context();
        JSArrayBuffer::from_js_value(&ctx, JSValue::from_raw(&ctx, buffer))
    }

    /// Get a byte slice for the view range.
    pub fn byte_view(&self) -> Option<&[u8]> {
        let buffer = self.as_value().get_array_buffer()?;
        let offset = self.byte_offset();
        let length = self.byte_length();
        let view = buffer.as_slice().get(offset..offset + length)?;
        // SAFETY: `view` points into the same engine-managed backing store as `self`.
        // We only reborrow the exact slice range without changing pointer/length.
        Some(unsafe { std::slice::from_raw_parts(view.as_ptr(), view.len()) })
    }

    /// Backward-compatible alias for `byte_view`.
    pub fn as_bytes(&self) -> Option<&[u8]> {
        self.byte_view()
    }

    pub fn cast<T>(&self) -> JSResult<JSTypedArray<V, T>>
    where
        T: TypedArrayElement,
    {
        JSTypedArray::from_any(self.clone())
    }
}

impl<V, T> JSTypedArray<V, T>
where
    V: JSObjectOps + JSTypedArrayOps + JSArrayBufferOps,
    T: TypedArrayElement,
{
    /// Create a statically-kinded typed array view from an ArrayBuffer.
    pub fn from_array_buffer(
        ctx: &JSContext<V::Context>,
        buffer: JSArrayBuffer<V>,
        byte_offset: usize,
        length: Option<usize>,
    ) -> JSResult<Self> {
        AnyJSTypedArray::from_array_buffer(ctx, T::TYPE, buffer, byte_offset, length).map(|inner| {
            Self {
                inner,
                marker: PhantomData,
            }
        })
    }
    /// Get the statically-known kind of typed array.
    pub fn kind(&self) -> JSTypedArrayKind {
        T::TYPE
    }

    /// Get the number of bytes per element.
    pub fn bytes_per_element(&self) -> usize {
        T::BYTES_PER_ELEMENT
    }

    /// Construct a statically-kinded typed array from a JSObject if it matches the expected kind.
    pub fn from_object(obj: JSObject<V>) -> Option<Self> {
        let inner = AnyJSTypedArray::from_object(obj)?;
        Self::from_any(inner).ok()
    }

    pub fn into_any(self) -> AnyJSTypedArray<V> {
        self.inner
    }
}

fn resolve_typed_array_length<V>(
    kind: JSTypedArrayKind,
    buffer: &JSArrayBuffer<V>,
    byte_offset: usize,
    length: Option<usize>,
) -> JSResult<usize>
where
    V: JSObjectOps + JSArrayBufferOps + JSValueImpl,
{
    let bytes_per_element = kind.bytes_per_element();
    if !byte_offset.is_multiple_of(bytes_per_element) {
        return Err(HostError::typed_array_alignment_error().into());
    }

    let buffer_size = buffer.len();
    if byte_offset > buffer_size {
        return Err(HostError::typed_array_range_error().into());
    }

    let available_bytes = buffer_size - byte_offset;
    match length {
        Some(length) => {
            if length > available_bytes / bytes_per_element {
                return Err(HostError::typed_array_range_error().into());
            }
            Ok(length)
        }
        None => {
            if !available_bytes.is_multiple_of(bytes_per_element) {
                return Err(HostError::typed_array_alignment_error().into());
            }
            Ok(available_bytes / bytes_per_element)
        }
    }
}

// blanket implementing.
impl<V: JSValueImpl> crate::function::JSParameterType for AnyJSTypedArray<V> {}
impl<V: JSValueImpl, T: TypedArrayElement> crate::function::JSParameterType for JSTypedArray<V, T> {}
