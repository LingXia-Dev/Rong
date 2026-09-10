//! `BufferSource` reading and `ArrayBuffer` producing.
//!
//! WebCrypto is `BufferSource`-heavy: nearly every argument is an
//! `ArrayBuffer` or an `ArrayBufferView`, and nearly every result is a fresh
//! `ArrayBuffer`. These helpers follow the same shape `rong_compression` and
//! `rong_http` use for reading binary arguments.

use rong::{AnyJSTypedArray, JSArrayBuffer, JSContext, JSResult, JSValue};

use crate::error;

/// Copy the bytes out of an `ArrayBuffer` or any `ArrayBufferView`.
pub(crate) fn buffer_source(value: &JSValue, label: &str) -> JSResult<Vec<u8>> {
    let object = value
        .clone()
        .into_object()
        .ok_or_else(|| not_buffer_source(label))?;

    if let Some(view) = AnyJSTypedArray::from_object(object.clone()) {
        let bytes = view.byte_view().ok_or_else(|| not_buffer_source(label))?;
        return Ok(bytes.to_vec());
    }

    if let Some(buffer) = JSArrayBuffer::from_object(object) {
        return Ok(buffer.to_vec());
    }

    Err(not_buffer_source(label))
}

/// Wrap owned bytes in a fresh `ArrayBuffer`, which is what every
/// `SubtleCrypto` method that produces bytes resolves with.
pub(crate) fn to_array_buffer(ctx: &JSContext, bytes: Vec<u8>) -> JSResult<JSArrayBuffer> {
    JSArrayBuffer::from_bytes_owned(ctx, bytes)
}

fn not_buffer_source(label: &str) -> rong::RongJSError {
    error::type_error(format!(
        "{label} must be an ArrayBuffer or an ArrayBufferView"
    ))
}
