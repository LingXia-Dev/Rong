//! The `SubtleCrypto` interface: `crypto.subtle`.
//!
//! Every method is `async`, so the `#[js_class]` macro turns it into a
//! promise-returning JavaScript method, matching the specification.
//!
//! Each entry point normalizes its `AlgorithmIdentifier` first and then matches
//! on [`Algorithm`]. Asymmetric algorithms are recognized by
//! [`crate::algorithm`] but not implemented, so they fall out of
//! `require_implemented` as a `NotSupportedError` naming the algorithm.

use rong::function::Optional;
use rong::{
    Class, IntoJSValue, JSArrayBuffer, JSContext, JSObject, JSResult, JSValue, js_class, js_method,
};

use crate::algorithm::{self, Algorithm, NormalizedAlgorithm};
use crate::buffer;
use crate::crypto::random_bytes;
use crate::error;
use crate::jwk;
use crate::key::{CryptoKey, KeyAlgorithm, KeyUsage, parse_usages};
use crate::ops;

const HMAC_USAGES: &[KeyUsage] = &[KeyUsage::Sign, KeyUsage::Verify];
const AES_USAGES: &[KeyUsage] = &[
    KeyUsage::Encrypt,
    KeyUsage::Decrypt,
    KeyUsage::WrapKey,
    KeyUsage::UnwrapKey,
];
const DERIVE_USAGES: &[KeyUsage] = &[KeyUsage::DeriveKey, KeyUsage::DeriveBits];

/// Reject an empty or out-of-range `keyUsages` list, which the specification
/// reports as a `SyntaxError`.
fn check_usages(algorithm: &str, allowed: &[KeyUsage], usages: &[KeyUsage]) -> JSResult<()> {
    if usages.is_empty() {
        return Err(error::syntax(format!(
            "{algorithm} keys require at least one key usage"
        )));
    }
    if let Some(usage) = usages.iter().find(|usage| !allowed.contains(usage)) {
        return Err(error::syntax(format!(
            "'{}' is not a valid key usage for {algorithm}",
            usage.as_str()
        )));
    }
    Ok(())
}

/// Read a dictionary member that must be a non-negative integer.
fn integer_param(normalized: &NormalizedAlgorithm, key: &str) -> JSResult<u64> {
    let value = normalized.require_param(key)?;
    let number = value.to_rust::<f64>().map_err(|_| {
        error::type_error(format!(
            "{}: '{key}' must be a number",
            normalized.algorithm.canonical_name()
        ))
    })?;
    if !number.is_finite() || number < 0.0 || number.fract() != 0.0 {
        return Err(error::type_error(format!(
            "{}: '{key}' must be a non-negative integer",
            normalized.algorithm.canonical_name()
        )));
    }
    Ok(number as u64)
}

/// Read an optional `BufferSource` dictionary member, defaulting to empty.
fn buffer_param(normalized: &NormalizedAlgorithm, key: &str) -> JSResult<Vec<u8>> {
    match normalized.param(key)? {
        Some(value) => buffer::buffer_source(&value, key),
        None => Ok(Vec::new()),
    }
}

/// Read a required `BufferSource` dictionary member.
fn require_buffer_param(normalized: &NormalizedAlgorithm, key: &str) -> JSResult<Vec<u8>> {
    let value = normalized.require_param(key)?;
    buffer::buffer_source(&value, key)
}

/// AES-GCM here is fixed at a 128-bit tag; anything else is out of scope.
fn check_gcm_tag_length(normalized: &NormalizedAlgorithm) -> JSResult<()> {
    let Some(value) = normalized.param("tagLength")? else {
        return Ok(());
    };
    let bits = value
        .to_rust::<f64>()
        .map_err(|_| error::type_error("AES-GCM: 'tagLength' must be a number"))?;
    if bits != 128.0 {
        return Err(error::not_supported(format!(
            "AES-GCM only supports a 128-bit tagLength, got {bits}"
        )));
    }
    Ok(())
}

/// Turn raw key material plus a normalized algorithm into a [`CryptoKey`].
fn build_symmetric_key(
    normalized: &NormalizedAlgorithm,
    algorithm: Algorithm,
    secret: Vec<u8>,
    extractable: bool,
    usages: Vec<KeyUsage>,
) -> JSResult<CryptoKey> {
    let key_algorithm = match algorithm {
        Algorithm::Hmac => {
            let hash = normalized.require_hash()?;
            if secret.is_empty() {
                return Err(error::data("HMAC key material must not be empty"));
            }
            KeyAlgorithm::Hmac {
                hash,
                length_bits: secret.len() * 8,
            }
        }
        Algorithm::AesCbc | Algorithm::AesGcm => {
            let length_bits = secret.len() * 8;
            if !ops::AES_KEY_LENGTHS.contains(&length_bits) {
                return Err(error::data(format!(
                    "{} key must be 128, 192 or 256 bits, got {length_bits}",
                    algorithm.canonical_name()
                )));
            }
            KeyAlgorithm::Aes {
                algorithm,
                length_bits,
            }
        }
        Algorithm::Pbkdf2 | Algorithm::Hkdf => KeyAlgorithm::Derivation { algorithm },
        other => {
            return Err(error::not_supported(format!(
                "{} is not supported for importKey",
                other.canonical_name()
            )));
        }
    };

    Ok(CryptoKey::secret(
        key_algorithm,
        extractable,
        usages,
        secret,
    ))
}

/// The shared body of `deriveBits` and `deriveKey`.
fn derive_bits_from(
    normalized: &NormalizedAlgorithm,
    base_key: &CryptoKey,
    usage: KeyUsage,
    bits: usize,
) -> JSResult<Vec<u8>> {
    if bits == 0 || !bits.is_multiple_of(8) {
        return Err(error::operation(format!(
            "derived length must be a non-zero multiple of 8, got {bits}"
        )));
    }
    let out_len = bits / 8;
    let algorithm = normalized.require_implemented("deriveBits")?;

    match algorithm {
        Algorithm::Pbkdf2 => {
            base_key.require(Algorithm::Pbkdf2, usage)?;
            let hash = normalized.require_hash()?;
            let salt = require_buffer_param(normalized, "salt")?;
            let iterations = integer_param(normalized, "iterations")?;
            if iterations == 0 || iterations > u32::MAX as u64 {
                return Err(error::operation(
                    "PBKDF2 'iterations' must be between 1 and 2^32 - 1",
                ));
            }
            Ok(ops::pbkdf2(
                hash,
                base_key.material(),
                &salt,
                iterations as u32,
                out_len,
            ))
        }
        Algorithm::Hkdf => {
            base_key.require(Algorithm::Hkdf, usage)?;
            let hash = normalized.require_hash()?;
            let salt = require_buffer_param(normalized, "salt")?;
            let info = buffer_param(normalized, "info")?;
            ops::hkdf(hash, base_key.material(), &salt, &info, out_len)
        }
        other => Err(error::not_supported(format!(
            "{} is not supported for key derivation",
            other.canonical_name()
        ))),
    }
}

/// How many bits a derived key of `normalized` needs.
fn derived_key_length(normalized: &NormalizedAlgorithm, algorithm: Algorithm) -> JSResult<usize> {
    match algorithm {
        Algorithm::Hmac => match normalized.param("length")? {
            Some(_) => Ok(integer_param(normalized, "length")? as usize),
            None => Ok(normalized.require_hash()?.block_len() * 8),
        },
        Algorithm::AesCbc | Algorithm::AesGcm => Ok(integer_param(normalized, "length")? as usize),
        other => Err(error::not_supported(format!(
            "{} keys cannot be derived",
            other.canonical_name()
        ))),
    }
}

/// The Web Cryptography `SubtleCrypto` interface.
#[js_class]
pub struct SubtleCrypto {}

#[js_class]
impl SubtleCrypto {
    #[js_method(constructor, private)]
    fn new() -> JSResult<Self> {
        rong::illegal_constructor(
            "SubtleCrypto cannot be constructed directly. Use globalThis.crypto.subtle.",
        )
    }

    /// `digest(algorithm, data)` for SHA-1, SHA-256, SHA-384 and SHA-512.
    #[js_method]
    async fn digest(
        &self,
        ctx: JSContext,
        algorithm: JSValue,
        data: JSValue,
    ) -> JSResult<JSArrayBuffer> {
        let normalized = algorithm::normalize(&algorithm)?;
        let bytes = buffer::buffer_source(&data, "data")?;
        match normalized.require_implemented("digest")? {
            Algorithm::Hash(hash) => buffer::to_array_buffer(&ctx, hash.digest(&bytes)),
            other => Err(error::not_supported(format!(
                "{} is not supported for digest",
                other.canonical_name()
            ))),
        }
    }

    /// `importKey(format, keyData, algorithm, extractable, keyUsages)` for the
    /// `raw` and `jwk` formats.
    #[js_method(rename = "importKey")]
    async fn import_key(
        &self,
        ctx: JSContext,
        format: String,
        key_data: JSValue,
        algorithm: JSValue,
        extractable: bool,
        key_usages: JSValue,
    ) -> JSResult<JSObject> {
        let normalized = algorithm::normalize(&algorithm)?;
        let usages = parse_usages(&key_usages)?;
        let algorithm_id = normalized.require_implemented("importKey")?;

        let allowed = match algorithm_id {
            Algorithm::Hmac => HMAC_USAGES,
            id if id.is_aes() => AES_USAGES,
            Algorithm::Pbkdf2 | Algorithm::Hkdf => DERIVE_USAGES,
            other => {
                return Err(error::not_supported(format!(
                    "{} is not supported for importKey",
                    other.canonical_name()
                )));
            }
        };
        check_usages(algorithm_id.canonical_name(), allowed, &usages)?;

        // PBKDF2/HKDF base keys are never extractable.
        if matches!(algorithm_id, Algorithm::Pbkdf2 | Algorithm::Hkdf) {
            if format != "raw" {
                return Err(error::not_supported(format!(
                    "{} keys can only be imported in 'raw' format",
                    algorithm_id.canonical_name()
                )));
            }
            if extractable {
                return Err(error::syntax(format!(
                    "{} keys must be imported as non-extractable",
                    algorithm_id.canonical_name()
                )));
            }
        }

        let (secret, source_jwk) = match format.as_str() {
            "raw" => (buffer::buffer_source(&key_data, "keyData")?, None),
            "jwk" => {
                let object = key_data.clone().into_object().ok_or_else(|| {
                    error::data("a 'jwk' import requires the key data to be a JSON Web Key object")
                })?;
                (jwk::decode_secret(&object)?, Some(object))
            }
            other => {
                return Err(error::not_supported(format!(
                    "key format '{other}' is not supported"
                )));
            }
        };

        let key = build_symmetric_key(
            &normalized,
            algorithm_id,
            secret,
            extractable,
            usages.clone(),
        )?;
        if let Some(object) = source_jwk {
            // `alg`, `ext` and `key_ops` are checked against the key the other
            // arguments describe, which needs the decoded material first.
            jwk::validate(&object, key.key_algorithm(), extractable, &usages)?;
        }
        key.into_js(&ctx)
    }

    /// `exportKey(format, key)` for the `raw` and `jwk` formats.
    #[js_method(rename = "exportKey")]
    async fn export_key(
        &self,
        ctx: JSContext,
        format: String,
        key: CryptoKey,
    ) -> JSResult<JSValue> {
        if !key.is_extractable() {
            return Err(error::invalid_access("key is not extractable"));
        }
        if matches!(key.key_algorithm(), KeyAlgorithm::Derivation { .. }) {
            return Err(error::not_supported(format!(
                "{} keys cannot be exported",
                key.key_algorithm().name()
            )));
        }

        match format.as_str() {
            "raw" => {
                let raw = buffer::to_array_buffer(&ctx, key.material().to_vec())?;
                Ok(raw.into_js_value(&ctx))
            }
            "jwk" => Ok(jwk::export(&ctx, &key)?.into_js_value()),
            other => Err(error::not_supported(format!(
                "key format '{other}' is not supported"
            ))),
        }
    }

    /// `generateKey(algorithm, extractable, keyUsages)` for HMAC, AES-GCM and
    /// AES-CBC.
    #[js_method(rename = "generateKey")]
    async fn generate_key(
        &self,
        ctx: JSContext,
        algorithm: JSValue,
        extractable: bool,
        key_usages: JSValue,
    ) -> JSResult<JSObject> {
        let normalized = algorithm::normalize(&algorithm)?;
        let usages = parse_usages(&key_usages)?;
        let algorithm_id = normalized.require_implemented("generateKey")?;

        let (key_algorithm, byte_len) = match algorithm_id {
            Algorithm::Hmac => {
                check_usages(algorithm_id.canonical_name(), HMAC_USAGES, &usages)?;
                let hash = normalized.require_hash()?;
                let length_bits = match normalized.param("length")? {
                    Some(_) => integer_param(&normalized, "length")? as usize,
                    None => hash.block_len() * 8,
                };
                if length_bits == 0 || !length_bits.is_multiple_of(8) {
                    return Err(error::operation(format!(
                        "HMAC key length must be a non-zero multiple of 8, got {length_bits}"
                    )));
                }
                (KeyAlgorithm::Hmac { hash, length_bits }, length_bits / 8)
            }
            Algorithm::AesCbc | Algorithm::AesGcm => {
                check_usages(algorithm_id.canonical_name(), AES_USAGES, &usages)?;
                let length_bits = integer_param(&normalized, "length")? as usize;
                if !ops::AES_KEY_LENGTHS.contains(&length_bits) {
                    return Err(error::operation(format!(
                        "{} key length must be 128, 192 or 256, got {length_bits}",
                        algorithm_id.canonical_name()
                    )));
                }
                (
                    KeyAlgorithm::Aes {
                        algorithm: algorithm_id,
                        length_bits,
                    },
                    length_bits / 8,
                )
            }
            other => {
                return Err(error::not_supported(format!(
                    "{} is not supported for generateKey",
                    other.canonical_name()
                )));
            }
        };

        let secret = random_bytes(byte_len)?;
        CryptoKey::secret(key_algorithm, extractable, usages, secret).into_js(&ctx)
    }

    /// `sign(algorithm, key, data)`; HMAC only in this pass.
    #[js_method]
    async fn sign(
        &self,
        ctx: JSContext,
        algorithm: JSValue,
        key: CryptoKey,
        data: JSValue,
    ) -> JSResult<JSArrayBuffer> {
        let normalized = algorithm::normalize(&algorithm)?;
        let bytes = buffer::buffer_source(&data, "data")?;
        match normalized.require_implemented("sign")? {
            Algorithm::Hmac => {
                key.require(Algorithm::Hmac, KeyUsage::Sign)?;
                let KeyAlgorithm::Hmac { hash, .. } = key.key_algorithm() else {
                    return Err(error::invalid_access("key is not an HMAC key"));
                };
                let signature = ops::hmac_sign(hash, key.material(), &bytes);
                buffer::to_array_buffer(&ctx, signature)
            }
            other => Err(error::not_supported(format!(
                "{} is not supported for sign",
                other.canonical_name()
            ))),
        }
    }

    /// `verify(algorithm, key, signature, data)`; HMAC only in this pass.
    ///
    /// The tag comparison runs in constant time inside [`ops::hmac_verify`].
    #[js_method]
    async fn verify(
        &self,
        algorithm: JSValue,
        key: CryptoKey,
        signature: JSValue,
        data: JSValue,
    ) -> JSResult<bool> {
        let normalized = algorithm::normalize(&algorithm)?;
        let signature = buffer::buffer_source(&signature, "signature")?;
        let bytes = buffer::buffer_source(&data, "data")?;
        match normalized.require_implemented("verify")? {
            Algorithm::Hmac => {
                key.require(Algorithm::Hmac, KeyUsage::Verify)?;
                let KeyAlgorithm::Hmac { hash, .. } = key.key_algorithm() else {
                    return Err(error::invalid_access("key is not an HMAC key"));
                };
                Ok(ops::hmac_verify(hash, key.material(), &bytes, &signature))
            }
            other => Err(error::not_supported(format!(
                "{} is not supported for verify",
                other.canonical_name()
            ))),
        }
    }

    /// `encrypt(algorithm, key, data)` for AES-GCM and AES-CBC.
    #[js_method]
    async fn encrypt(
        &self,
        ctx: JSContext,
        algorithm: JSValue,
        key: CryptoKey,
        data: JSValue,
    ) -> JSResult<JSArrayBuffer> {
        let normalized = algorithm::normalize(&algorithm)?;
        let plaintext = buffer::buffer_source(&data, "data")?;
        let algorithm_id = normalized.require_implemented("encrypt")?;
        key.require(algorithm_id, KeyUsage::Encrypt)?;

        let ciphertext = match algorithm_id {
            Algorithm::AesGcm => {
                check_gcm_tag_length(&normalized)?;
                let iv = require_buffer_param(&normalized, "iv")?;
                let aad = buffer_param(&normalized, "additionalData")?;
                ops::aes_gcm_encrypt(key.material(), &iv, &aad, &plaintext)?
            }
            Algorithm::AesCbc => {
                let iv = require_buffer_param(&normalized, "iv")?;
                ops::aes_cbc_encrypt(key.material(), &iv, &plaintext)?
            }
            other => {
                return Err(error::not_supported(format!(
                    "{} is not supported for encrypt",
                    other.canonical_name()
                )));
            }
        };
        buffer::to_array_buffer(&ctx, ciphertext)
    }

    /// `decrypt(algorithm, key, data)` for AES-GCM and AES-CBC.
    #[js_method]
    async fn decrypt(
        &self,
        ctx: JSContext,
        algorithm: JSValue,
        key: CryptoKey,
        data: JSValue,
    ) -> JSResult<JSArrayBuffer> {
        let normalized = algorithm::normalize(&algorithm)?;
        let ciphertext = buffer::buffer_source(&data, "data")?;
        let algorithm_id = normalized.require_implemented("decrypt")?;
        key.require(algorithm_id, KeyUsage::Decrypt)?;

        let plaintext = match algorithm_id {
            Algorithm::AesGcm => {
                check_gcm_tag_length(&normalized)?;
                let iv = require_buffer_param(&normalized, "iv")?;
                let aad = buffer_param(&normalized, "additionalData")?;
                ops::aes_gcm_decrypt(key.material(), &iv, &aad, &ciphertext)?
            }
            Algorithm::AesCbc => {
                let iv = require_buffer_param(&normalized, "iv")?;
                ops::aes_cbc_decrypt(key.material(), &iv, &ciphertext)?
            }
            other => {
                return Err(error::not_supported(format!(
                    "{} is not supported for decrypt",
                    other.canonical_name()
                )));
            }
        };
        buffer::to_array_buffer(&ctx, plaintext)
    }

    /// `deriveBits(algorithm, baseKey, length)` for PBKDF2 and HKDF.
    #[js_method(rename = "deriveBits")]
    async fn derive_bits(
        &self,
        ctx: JSContext,
        algorithm: JSValue,
        base_key: CryptoKey,
        length: Optional<JSValue>,
    ) -> JSResult<JSArrayBuffer> {
        let normalized = algorithm::normalize(&algorithm)?;
        let bits = length
            .0
            .filter(|value| !value.is_undefined() && !value.is_null())
            .ok_or_else(|| error::operation("deriveBits requires a length in bits"))?
            .to_rust::<f64>()
            .map_err(|_| error::type_error("deriveBits length must be a number"))?;
        if !bits.is_finite() || bits < 0.0 || bits.fract() != 0.0 {
            return Err(error::operation(
                "deriveBits length must be a non-negative integer number of bits",
            ));
        }

        let derived =
            derive_bits_from(&normalized, &base_key, KeyUsage::DeriveBits, bits as usize)?;
        buffer::to_array_buffer(&ctx, derived)
    }

    /// `deriveKey(algorithm, baseKey, derivedKeyAlgorithm, extractable, keyUsages)`.
    #[js_method(rename = "deriveKey")]
    async fn derive_key(
        &self,
        ctx: JSContext,
        algorithm: JSValue,
        base_key: CryptoKey,
        derived_key_algorithm: JSValue,
        extractable: bool,
        key_usages: JSValue,
    ) -> JSResult<JSObject> {
        let normalized = algorithm::normalize(&algorithm)?;
        let derived_normalized = algorithm::normalize(&derived_key_algorithm)?;
        let usages = parse_usages(&key_usages)?;
        let derived_id = derived_normalized.require_implemented("deriveKey")?;

        let allowed = match derived_id {
            Algorithm::Hmac => HMAC_USAGES,
            id if id.is_aes() => AES_USAGES,
            other => {
                return Err(error::not_supported(format!(
                    "{} keys cannot be derived",
                    other.canonical_name()
                )));
            }
        };
        check_usages(derived_id.canonical_name(), allowed, &usages)?;

        let bits = derived_key_length(&derived_normalized, derived_id)?;
        let secret = derive_bits_from(&normalized, &base_key, KeyUsage::DeriveKey, bits)?;
        build_symmetric_key(&derived_normalized, derived_id, secret, extractable, usages)?
            .into_js(&ctx)
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }
}

impl SubtleCrypto {
    pub(crate) fn instance(ctx: &JSContext) -> JSResult<JSObject> {
        Ok(Class::lookup::<Self>(ctx)?.instance(Self {}))
    }
}
