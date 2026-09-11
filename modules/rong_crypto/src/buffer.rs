//! `BufferSource` reading and `ArrayBuffer` producing.
//!
//! WebCrypto is `BufferSource`-heavy: nearly every argument is an
//! `ArrayBuffer` or an `ArrayBufferView`, and nearly every result is a fresh
//! `ArrayBuffer`. These helpers follow the same shape `rong_compression` and
//! `rong_http` use for reading binary arguments.

use rong::{
    AnyJSTypedArray, JSArray, JSArrayBuffer, JSContext, JSFunc, JSObject, JSResult, JSValue, Source,
};

use crate::error;

struct DataViewAccess {
    buffer: JSFunc,
    offset: JSFunc,
    length: JSFunc,
}

/// Capture the engine's brand-checking accessors before application code runs.
/// Calling them directly ignores shadowed properties and accepts subclasses.
pub(crate) fn init(ctx: &JSContext) -> JSResult<()> {
    if ctx.get_state::<DataViewAccess>().is_none() {
        let getters: JSArray = ctx.eval(Source::from_bytes(
            "['buffer', 'byteOffset', 'byteLength'].map(name => Object.getOwnPropertyDescriptor(DataView.prototype, name).get)",
        ))?;
        ctx.set_state(DataViewAccess {
            buffer: getters.get(0)?,
            offset: getters.get(1)?,
            length: getters.get(2)?,
        });
    }
    Ok(())
}

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

    if let Some(buffer) = JSArrayBuffer::from_object(object.clone()) {
        return Ok(buffer.to_vec());
    }

    if let Some(bytes) = data_view_bytes(&object)? {
        return Ok(bytes);
    }

    Err(not_buffer_source(label))
}

/// `BufferSource` includes `DataView`, which is an `ArrayBufferView` but not a
/// typed array, so `AnyJSTypedArray` does not accept it.
fn data_view_bytes(object: &JSObject) -> JSResult<Option<Vec<u8>>> {
    let ctx = object.context();
    let access = ctx
        .get_state::<DataViewAccess>()
        .ok_or_else(|| error::operation("DataView accessors are not initialized"))?;
    let Ok(buffer_obj) = access.buffer.call::<_, JSObject>(Some(object.clone()), ()) else {
        return Ok(None);
    };
    let buffer = JSArrayBuffer::from_object(buffer_obj)
        .ok_or_else(|| error::type_error("DataView.buffer must be an ArrayBuffer"))?;
    let byte_offset = access.offset.call::<_, f64>(Some(object.clone()), ())? as usize;
    let byte_length = access.length.call::<_, f64>(Some(object.clone()), ())? as usize;
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
