//! # TextDecoder Implementation
//!
//! This module provides a Rust implementation of the JavaScript `TextDecoder` interface,
//! as defined by the [Encoding Standard](https://encoding.spec.whatwg.org/).
//!
//! ## Features
//!
//! - **UTF-8 Support**: Currently only UTF-8 encoding is supported.
//! - **BOM Handling**: Automatically detects and skips UTF-8 BOM (Byte Order Mark) unless `ignoreBOM` is set to `true`.
//! - **Fatal Mode**: Throws errors on invalid UTF-8 sequences when `fatal` is `true`, otherwise replaces them with the replacement character (U+FFFD).
//!
//! ## Limitations
//!
//! - Only UTF-8 encoding is supported. Other encodings will result in a `TypeError`.
//! - The `stream` option in `decode` is currently ignored.

use rong::{function::Optional, *};

#[derive(Default)]
struct TextDecoderOptions {
    fatal: bool,
    ignore_bom: bool,
}

#[js_class]
pub struct TextDecoder {
    // TextDecoder supports different encodings, but we only implement UTF-8 for now
    encoding: &'static str,
    /// Whether to throw errors on invalid UTF-8 sequences.
    fatal: bool,
    /// Whether to ignore the UTF-8 BOM (Byte Order Mark).
    ignore_bom: bool,
}

#[js_class]
impl TextDecoder {
    /// Creates a new `TextDecoder` instance.
    ///
    /// # Arguments
    ///
    /// * `label` - The encoding label (currently only "utf-8" is supported).
    /// * `options` - Configuration options (`fatal` and `ignoreBOM`).
    ///
    /// # Errors
    ///
    /// Returns a `TypeError` if an unsupported encoding is specified.
    #[js_method(constructor)]
    fn new(label: Optional<String>, options: Optional<JSObject>) -> JSResult<Self> {
        // Only support UTF-8 encoding for now
        if let Some(label) = label.0 {
            let label = label.to_lowercase();
            // Check if the encoding is supported
            match label.as_str() {
                "utf-8" | "utf8" | "" => {}
                _ => {
                    return Err(HostError::new(
                        rong::error::E_INVALID_ARG,
                        format!("Unsupported encoding: {}", label),
                    )
                    .with_name("TypeError")
                    .into());
                }
            }
        }

        let mut opts = TextDecoderOptions::default();
        if let Some(options) = options.0 {
            if let Ok(fatal) = options.get::<_, bool>("fatal") {
                opts.fatal = fatal;
            }
            if let Ok(ignore_bom) = options.get::<_, bool>("ignoreBOM") {
                opts.ignore_bom = ignore_bom;
            }
        }

        Ok(Self {
            encoding: "utf-8",
            fatal: opts.fatal,
            ignore_bom: opts.ignore_bom,
        })
    }

    /// Gets the encoding used by the decoder.
    #[js_method(getter, enumerable)]
    fn encoding(&self) -> String {
        self.encoding.to_string()
    }

    /// Gets whether the decoder is in fatal mode.
    #[js_method(getter, enumerable)]
    fn fatal(&self) -> bool {
        self.fatal
    }

    /// Gets whether the decoder ignores the BOM.
    #[js_method(getter, enumerable, rename = "ignoreBOM")]
    fn ignore_bom(&self) -> bool {
        self.ignore_bom
    }

    /// Decodes the given input into a string.
    ///
    /// # Arguments
    ///
    /// * `input` - The input data (`ArrayBuffer` or `TypedArray`).
    /// * `options` - Decoding options (currently only `stream` is supported, but ignored).
    ///
    /// # Errors
    ///
    /// Returns a `TypeError` if the input is invalid or if an invalid UTF-8 sequence is encountered in fatal mode.
    #[js_method]
    fn decode(&self, input: Optional<JSObject>, options: Optional<JSObject>) -> JSResult<String> {
        // Handle stream option
        let mut _stream = false;
        if let Some(options) = options.0
            && let Ok(stream) = options.get::<_, bool>("stream")
        {
            _stream = stream;
        }

        // Get the bytes from input
        let bytes = if let Some(input) = input.0 {
            if let Some(typed_array) = AnyJSTypedArray::from_object(input.clone()) {
                // Get bytes from TypedArray
                if let Some(bytes) = typed_array.as_bytes() {
                    bytes.to_vec()
                } else {
                    return Err(
                        HostError::new(rong::error::E_INVALID_ARG, "Invalid TypedArray")
                            .with_name("TypeError")
                            .into(),
                    );
                }
            } else if let Some(buffer) = JSArrayBuffer::from_object(input) {
                // Get bytes from ArrayBuffer
                buffer.as_bytes().to_vec()
            } else {
                return Err(HostError::new(
                    rong::error::E_INVALID_ARG,
                    "Input must be an ArrayBuffer or TypedArray",
                )
                .with_name("TypeError")
                .into());
            }
        } else {
            Vec::new() // Empty input returns empty byte vector
        };

        // Handle BOM if present and not ignored
        let start =
            if !self.ignore_bom && bytes.len() >= 3 && bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
                3
            } else {
                0
            };

        // Decode bytes to string
        match String::from_utf8(bytes[start..].to_vec()) {
            Ok(text) => Ok(text),
            Err(e) => {
                if self.fatal {
                    Err(HostError::new(
                        rong::error::E_INVALID_ARG,
                        format!("Invalid UTF-8 sequence: {}", e),
                    )
                    .with_name("TypeError")
                    .into())
                } else {
                    // Replace invalid sequences with replacement character (U+FFFD)
                    Ok(String::from_utf8_lossy(&bytes[start..]).into_owned())
                }
            }
        }
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }
}

pub(crate) fn init(ctx: &JSContext) -> JSResult<()> {
    ctx.register_class::<TextDecoder>()?;
    Ok(())
}
