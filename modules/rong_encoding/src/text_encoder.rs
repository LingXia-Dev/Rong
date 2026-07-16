//! # TextEncoder Implementation
//!
//! This module provides a Rust implementation of the JavaScript `TextEncoder` interface,
//! as defined by the [Encoding Standard](https://encoding.spec.whatwg.org/).
//!
//! ## Features
//!
//! - **UTF-8 Support**: Encodes strings into UTF-8 byte sequences.
//! - **Efficient Encoding**: Provides both `encode` and `encodeInto` methods for different use cases.
//! - **Streaming Support**: `encodeInto` allows partial encoding into existing buffers.
//!
//! ## Limitations
//! - Only UTF-8 encoding is supported. Other encodings will result in a `TypeError`.
//!
//! ## Performance Considerations
//! - `encode` creates a new `Uint8Array` for each call, which may allocate memory.
//! - `encodeInto` allows reusing existing buffers, reducing allocations for repeated operations.

use rong::*;

/// Implementation of the JavaScript `TextEncoder` interface.
/// Encodes strings into UTF-8 byte sequences. Currently supports only UTF-8 encoding.
#[js_class]
pub struct TextEncoder {}

#[js_class]
impl TextEncoder {
    /// Creates a new `TextEncoder` instance.
    #[js_method(constructor)]
    fn new() -> Self {
        Self {}
    }

    /// Gets the encoding used by the encoder (always "utf-8").
    #[js_method(getter, enumerable)]
    fn encoding(&self) -> String {
        "utf-8".to_string()
    }

    /// Encodes a string into a `Uint8Array` of UTF-8 bytes.
    ///
    /// # Arguments
    ///
    /// * `input` - The string to encode.
    ///
    /// # Returns
    ///
    /// A `Uint8Array` containing the UTF-8 encoded bytes of the input string.
    #[js_method]
    fn encode(&self, ctx: JSContext, input: JSValue) -> JSResult<JSTypedArray> {
        let input = if input.is_undefined() || input.is_null() {
            String::new()
        } else {
            input.to_rust::<String>()?
        };
        // Convert the string to UTF-8 bytes
        let bytes = input.as_bytes();

        // Create a buffer with the bytes and create a Uint8Array from it
        let buffer = JSArrayBuffer::from_bytes(&ctx, bytes)?;
        JSTypedArray::<u8>::from_array_buffer(&ctx, buffer, 0, Some(bytes.len()))
    }

    /// Encodes a string into a provided `Uint8Array`, returning the number of bytes read and written.
    ///
    /// # Arguments
    ///
    /// * `input` - The string to encode.
    /// * `dest` - The destination `Uint8Array` to write the encoded bytes into.
    ///
    /// # Returns
    ///
    /// An object containing the number of bytes read and written.
    #[js_method(rename = "encodeInto")]
    fn encode_into(&self, ctx: JSContext, input: String, dest: JSObject) -> JSResult<JSObject> {
        // First, check if dest can be converted to JSTypedArray
        if let Some(typed_array) = AnyJSTypedArray::from_object(dest) {
            // Then, check if the typed array is Uint8Array
            if typed_array.kind() == JSTypedArrayKind::Uint8 {
                // Get the underlying buffer and its length
                let byte_offset = typed_array.byte_offset();
                let buffer_len = typed_array.byte_length();
                let mut buffer = typed_array.buffer()?;
                let buffer_end = byte_offset.checked_add(buffer_len).ok_or_else(|| {
                    HostError::new(
                        rong::error::E_OUT_OF_RANGE,
                        "Uint8Array view range overflow",
                    )
                    .with_name("RangeError")
                })?;
                let buffer_data = buffer
                    .as_mut_slice()
                    .get_mut(byte_offset..buffer_end)
                    .ok_or_else(|| {
                        HostError::new(
                            rong::error::E_OUT_OF_RANGE,
                            "Uint8Array view exceeds its backing buffer",
                        )
                        .with_name("RangeError")
                    })?;

                let mut read = 0usize;
                let mut written = 0usize;
                for ch in input.chars() {
                    let encoded_len = ch.len_utf8();
                    let Some(end) = written.checked_add(encoded_len) else {
                        break;
                    };
                    if end > buffer_data.len() {
                        break;
                    }

                    let mut encoded = [0u8; 4];
                    let bytes = ch.encode_utf8(&mut encoded).as_bytes();
                    buffer_data[written..end].copy_from_slice(bytes);
                    written = end;
                    read += ch.len_utf16();
                }

                // Create result object with read/written counts
                let result = JSObject::new(&ctx);
                result.set("read", read as f64)?;
                result.set("written", written as f64)?;

                return Ok(result);
            }
        }

        // If either check fails, return TypeError
        Err(HostError::new(
            rong::error::E_INVALID_ARG,
            "The \"dest\" argument must be an instance of Uint8Array.",
        )
        .with_name("TypeError")
        .into())
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }
}

impl Default for TextEncoder {
    fn default() -> Self {
        Self::new()
    }
}

/// Registers the `TextEncoder` class with the JavaScript context.
pub(crate) fn init(ctx: &JSContext) -> JSResult<()> {
    ctx.register_class::<TextEncoder>()?;
    Ok(())
}
