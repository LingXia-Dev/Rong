//! The `Crypto` interface: `globalThis.crypto`.

use rong::{
    AnyJSTypedArray, JSContext, JSObject, JSResult, JSTypedArrayKind, JSValue, js_class, js_method,
};

use crate::error;

/// `getRandomValues` refuses to fill more than this many bytes at once
/// (Web Cryptography API, "Crypto interface").
const MAX_RANDOM_BYTES: usize = 65_536;

/// Fill a slice from the platform CSPRNG.
pub(crate) fn fill_random(dest: &mut [u8]) -> JSResult<()> {
    getrandom::fill(dest).map_err(|source| {
        error::operation(format!(
            "the platform random number generator failed: {source}"
        ))
    })
}

/// Allocate `len` cryptographically random bytes.
pub(crate) fn random_bytes(len: usize) -> JSResult<Vec<u8>> {
    let mut bytes = vec![0u8; len];
    fill_random(&mut bytes)?;
    Ok(bytes)
}

/// Format 16 random bytes as an RFC 4122 version 4 UUID.
fn format_uuid_v4(mut bytes: [u8; 16]) -> String {
    // Version 4 in the high nibble of octet 6, IETF variant in octet 8.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    let mut uuid = String::with_capacity(36);
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(index, 4 | 6 | 8 | 10) {
            uuid.push('-');
        }
        uuid.push(char::from_digit((byte >> 4) as u32, 16).unwrap_or('0'));
        uuid.push(char::from_digit((byte & 0x0f) as u32, 16).unwrap_or('0'));
    }
    uuid
}

/// The Web Cryptography `Crypto` interface.
///
/// Like the platform, the constructor is unreachable from JavaScript; the
/// single instance is installed as `globalThis.crypto` by [`crate::init`].
#[js_class]
pub struct Crypto {}

#[js_class]
impl Crypto {
    #[js_method(constructor, private)]
    fn new() -> JSResult<Self> {
        rong::illegal_constructor("Crypto cannot be constructed directly. Use globalThis.crypto.")
    }

    /// Fill an integer typed array with random values and return it.
    #[js_method(rename = "getRandomValues")]
    fn get_random_values(&self, array: JSObject) -> JSResult<JSObject> {
        let view = AnyJSTypedArray::from_object(array.clone()).ok_or_else(|| {
            error::type_mismatch("getRandomValues expects an integer typed array")
        })?;

        // Float views (and DataView, which is not a typed array at all) are a
        // TypeMismatchError; every integer view is accepted.
        if matches!(
            view.kind(),
            JSTypedArrayKind::Float32 | JSTypedArrayKind::Float64
        ) {
            return Err(error::type_mismatch(
                "getRandomValues does not accept floating point typed arrays",
            ));
        }

        let byte_length = view.byte_length();
        if byte_length > MAX_RANDOM_BYTES {
            return Err(error::quota_exceeded(format!(
                "getRandomValues cannot fill more than {MAX_RANDOM_BYTES} bytes, requested {byte_length}"
            )));
        }

        let byte_offset = view.byte_offset();
        let end = byte_offset.checked_add(byte_length).ok_or_else(|| {
            error::operation("typed array view range overflows its backing buffer")
        })?;
        let mut buffer = view.buffer()?;
        let target = buffer
            .as_mut_slice()
            .get_mut(byte_offset..end)
            .ok_or_else(|| error::operation("typed array view exceeds its backing buffer"))?;
        fill_random(target)?;

        Ok(array)
    }

    /// A randomly generated, lowercase, hyphenated RFC 4122 version 4 UUID.
    #[js_method(rename = "randomUUID")]
    fn random_uuid(&self) -> JSResult<String> {
        let mut bytes = [0u8; 16];
        fill_random(&mut bytes)?;
        Ok(format_uuid_v4(bytes))
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }
}

impl Crypto {
    pub(crate) fn instance(ctx: &JSContext) -> JSResult<JSObject> {
        Ok(rong::Class::lookup::<Self>(ctx)?.instance(Self {}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_v4_has_the_rfc_4122_shape() {
        let uuid = format_uuid_v4([0xff; 16]);
        assert_eq!(uuid.len(), 36);
        assert_eq!(
            uuid.match_indices('-')
                .map(|(index, _)| index)
                .collect::<Vec<_>>(),
            vec![8, 13, 18, 23]
        );
        // Version 4 and the IETF variant are forced regardless of the input.
        assert_eq!(uuid.as_bytes()[14], b'4');
        assert!(matches!(uuid.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
        assert!(uuid.chars().all(|c| c == '-' || c.is_ascii_hexdigit()));
        assert_eq!(uuid, uuid.to_lowercase());
    }

    #[test]
    fn uuid_v4_preserves_every_other_bit() {
        let uuid = format_uuid_v4([0x00; 16]);
        assert_eq!(uuid, "00000000-0000-4000-8000-000000000000");
    }

    #[test]
    fn random_bytes_are_not_constant() {
        let a = random_bytes(32).expect("the platform CSPRNG is available");
        let b = random_bytes(32).expect("the platform CSPRNG is available");
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
        assert!(a.iter().any(|byte| *byte != 0));
    }
}
