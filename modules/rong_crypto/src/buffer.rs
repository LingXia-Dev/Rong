//! `BufferSource` reading and `ArrayBuffer` producing.
//!
//! WebCrypto is `BufferSource`-heavy: nearly every argument is an
//! `ArrayBuffer` or an `ArrayBufferView`, and nearly every result is a fresh
//! `ArrayBuffer`. These helpers follow the same shape `rong_compression` and
//! `rong_http` use for reading binary arguments.

use rong::{AnyJSTypedArray, JSArrayBuffer, JSContext, JSObject, JSResult, JSValue};

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

    if let Some(bytes) = data_view_bytes(&object)? {
        return Ok(bytes);
    }

    if let Some(buffer) = JSArrayBuffer::from_object(object) {
        return Ok(buffer.to_vec());
    }

    Err(not_buffer_source(label))
}

/// `BufferSource` includes `DataView`, which is an `ArrayBufferView` but not a
/// typed array, so `AnyJSTypedArray` does not accept it.
fn data_view_bytes(object: &JSObject) -> JSResult<Option<Vec<u8>>> {
    let ctor_name = object
        .get::<_, JSObject>("constructor")
        .ok()
        .and_then(|ctor| ctor.get::<_, String>("name").ok());
    if ctor_name.as_deref() != Some("DataView") {
        return Ok(None);
    }

    let buffer_obj: JSObject = object
        .get("buffer")
        .map_err(|_| error::type_error("DataView is missing its ArrayBuffer"))?;
    let buffer = JSArrayBuffer::from_object(buffer_obj)
        .ok_or_else(|| error::type_error("DataView.buffer must be an ArrayBuffer"))?;
    let byte_offset = object
        .get::<_, f64>("byteOffset")
        .map_err(|_| error::type_error("DataView.byteOffset must be a number"))?
        as usize;
    let byte_length = object
        .get::<_, f64>("byteLength")
        .map_err(|_| error::type_error("DataView.byteLength must be a number"))?
        as usize;
    let end = byte_offset
        .checked_add(byte_length)
        .ok_or_else(|| error::operation("DataView range overflows its ArrayBuffer"))?;
    let bytes = buffer
        .as_slice()
        .get(byte_offset..end)
        .ok_or_else(|| error::operation("DataView exceeds its ArrayBuffer"))?;
    Ok(Some(bytes.to_vec()))
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
