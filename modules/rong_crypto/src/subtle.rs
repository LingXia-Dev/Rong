//! The `SubtleCrypto` interface: `crypto.subtle`.
//!
//! Entry points copy and normalize inputs synchronously, then return a promise
//! for the operation. No caller-owned dictionary or buffer crosses that boundary.
//!
//! Each entry point normalizes its `AlgorithmIdentifier` first and then matches
//! on [`Algorithm`]. Asymmetric algorithms are recognized by
//! [`crate::algorithm`] but not implemented, so they fall out of
//! `require_implemented` as a `NotSupportedError` naming the algorithm.

use rong::function::Optional;
use rong::{
    Class, IntoJSValue, JSContext, JSEngineValue, JSObject, JSResult, JSValue, Promise, js_class,
    js_method,
};

use crate::algorithm::{self, Algorithm, NormalizedAlgorithm};
use crate::buffer;
use crate::crypto::random_bytes;
use crate::error;
use crate::hash::HashAlg;
use crate::jwk;
use crate::key::{CryptoKey, KeyAlgorithm, KeyUsage, parse_usages};
use crate::ops;
use std::future::Future;

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

/// Prepare inputs now; even preparation failures are delivered as rejections.
fn dispatch<F, T>(ctx: &JSContext, prepare: impl FnOnce() -> JSResult<F>) -> JSResult<Promise>
where
    F: Future<Output = JSResult<T>> + 'static,
    T: IntoJSValue<JSEngineValue> + 'static,
{
    let prepared = prepare();
    Promise::from_future(ctx, None, async move { prepared?.await })
}

/// WebIDL [EnforceRange]: truncate first, then check the destination range.
fn unsigned_param(value: JSValue, label: &str, max: u32) -> JSResult<u32> {
    let number = value.to_rust::<f64>()?;
    let integer = number.trunc();
    if !number.is_finite() || integer < 0.0 || integer > f64::from(max) {
        return Err(error::type_error(format!(
            "{label} must be between 0 and {max}"
        )));
    }
    Ok(integer as u32)
}

/// Unlike dictionary members, deriveBits' length has no [EnforceRange].
/// WebIDL unsigned long conversion wraps modulo 2^32 before the operation.
fn derived_bits_length(value: JSValue) -> JSResult<usize> {
    let number = value.to_rust::<f64>()?;
    if !number.is_finite() || number == 0.0 {
        return Ok(0);
    }
    Ok(number.trunc().rem_euclid(4_294_967_296.0) as u32 as usize)
}

fn integer_param(normalized: &NormalizedAlgorithm, key: &str) -> JSResult<u32> {
    let max = if normalized.algorithm.is_aes() && key == "length" {
        u32::from(u16::MAX)
    } else {
        u32::MAX
    };
    unsigned_param(normalized.require_param(key)?, key, max)
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
    let bits = unsigned_param(value, "AES-GCM tagLength", u32::from(u8::MAX))?;
    if bits != 128 {
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
    let mut secret = secret;
    let key_algorithm = match algorithm {
        Algorithm::Hmac => {
            let hash = normalized.require_hash()?;
            if secret.is_empty() {
                return Err(error::data("HMAC key material must not be empty"));
            }
            let data_bits = secret.len() * 8;
            let (length_bits, truncated) = match normalized.param("length")? {
                Some(_) => {
                    let length = integer_param(normalized, "length")? as usize;
                    hmac_import_length(length, data_bits, secret)?
                }
                None => (data_bits, secret),
            };
            secret = truncated;
            KeyAlgorithm::Hmac { hash, length_bits }
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

/// HMAC import steps 7–9: `length` must sit in `(dataBits - 8, dataBits]`,
/// except that `0` is always `DataError`. The key is truncated to that many
/// bits (only the last 1–7 bits of the final byte may be dropped).
fn hmac_import_length(
    length: usize,
    data_bits: usize,
    mut secret: Vec<u8>,
) -> JSResult<(usize, Vec<u8>)> {
    if length == 0 {
        return Err(error::data("HMAC key length must not be 0"));
    }
    if length > data_bits {
        return Err(error::data(format!(
            "HMAC key length {length} is greater than the {data_bits}-bit key material"
        )));
    }
    if length <= data_bits.saturating_sub(8) {
        return Err(error::data(format!(
            "HMAC key length {length} is too short for the {data_bits}-bit key material"
        )));
    }
    let byte_len = length.div_ceil(8);
    secret.truncate(byte_len);
    if let Some(spare) = (8 - (length % 8)).checked_rem(8)
        && spare != 0
        && let Some(last) = secret.last_mut()
    {
        *last &= 0xff << spare;
    }
    Ok((length, secret))
}

fn derived_length_bytes(bits: usize) -> JSResult<usize> {
    if !bits.is_multiple_of(8) {
        return Err(error::operation(format!(
            "derived length must be a multiple of 8, got {bits}"
        )));
    }
    Ok(bits / 8)
}

/// Owned derivation inputs; preparation never starts background work.
struct Derivation {
    algorithm: Algorithm,
    hash: HashAlg,
    material: Vec<u8>,
    salt: Vec<u8>,
    info: Vec<u8>,
    iterations: u32,
    out_len: usize,
}

impl Derivation {
    fn prepare(
        normalized: &NormalizedAlgorithm,
        base_key: &CryptoKey,
        usage: KeyUsage,
        bits: usize,
    ) -> JSResult<Self> {
        let algorithm = normalized.require_implemented("deriveBits")?;
        if !matches!(algorithm, Algorithm::Pbkdf2 | Algorithm::Hkdf) {
            return Err(error::not_supported(format!(
                "{} is not supported for key derivation",
                algorithm.canonical_name()
            )));
        }
        base_key.require(algorithm, usage)?;
        let hash = normalized.require_hash()?;
        let salt = require_buffer_param(normalized, "salt")?;
        let (iterations, info) = if algorithm == Algorithm::Pbkdf2 {
            let iterations = integer_param(normalized, "iterations")?;
            if iterations == 0 {
                return Err(error::operation("PBKDF2 iterations must not be zero"));
            }
            (iterations, Vec::new())
        } else {
            (0, require_buffer_param(normalized, "info")?)
        };
        Ok(Self {
            algorithm,
            hash,
            material: base_key.material().to_vec(),
            salt,
            info,
            iterations,
            out_len: derived_length_bytes(bits)?,
        })
    }

    async fn run(self) -> JSResult<Vec<u8>> {
        if self.out_len == 0 {
            return Ok(Vec::new());
        }
        if self.algorithm == Algorithm::Pbkdf2 {
            tokio::task::spawn_blocking(move || {
                ops::pbkdf2(
                    self.hash,
                    &self.material,
                    &self.salt,
                    self.iterations,
                    self.out_len,
                )
            })
            .await
            .map_err(|error| error::operation(format!("PBKDF2 derivation task failed: {error}")))
        } else {
            ops::hkdf(
                self.hash,
                &self.material,
                &self.salt,
                &self.info,
                self.out_len,
            )
        }
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
    fn digest(&self, ctx: JSContext, algorithm: JSValue, data: JSValue) -> JSResult<Promise> {
        let promise_ctx = ctx.clone();
        dispatch(&promise_ctx, || {
            let bytes = buffer::buffer_source(&data, "data")?;
            let normalized = algorithm::normalize(&algorithm)?;
            let algorithm = normalized.require_implemented("digest")?;

            Ok(async move {
                match algorithm {
                    Algorithm::Hash(hash) => buffer::to_array_buffer(&ctx, hash.digest(&bytes)),
                    other => Err(error::not_supported(format!(
                        "{} is not supported for digest",
                        other.canonical_name()
                    ))),
                }
            })
        })
    }

    /// `importKey(format, keyData, algorithm, extractable, keyUsages)` for the
    /// `raw` and `jwk` formats.
    #[js_method(rename = "importKey")]
    fn import_key(
        &self,
        ctx: JSContext,
        format: String,
        key_data: JSValue,
        algorithm: JSValue,
        extractable: bool,
        key_usages: JSValue,
    ) -> JSResult<Promise> {
        let promise_ctx = ctx.clone();
        dispatch(&promise_ctx, || {
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
                        error::data(
                            "a 'jwk' import requires the key data to be a JSON Web Key object",
                        )
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

            Ok(async move { key.into_js(&ctx) })
        })
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
    fn generate_key(
        &self,
        ctx: JSContext,
        algorithm: JSValue,
        extractable: bool,
        key_usages: JSValue,
    ) -> JSResult<Promise> {
        let promise_ctx = ctx.clone();
        dispatch(&promise_ctx, || {
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

            Ok(async move {
                let secret = random_bytes(byte_len)?;
                CryptoKey::secret(key_algorithm, extractable, usages, secret).into_js(&ctx)
            })
        })
    }

    /// `sign(algorithm, key, data)`; HMAC only.
    #[js_method]
    fn sign(
        &self,
        ctx: JSContext,
        algorithm: JSValue,
        key: CryptoKey,
        data: JSValue,
    ) -> JSResult<Promise> {
        let promise_ctx = ctx.clone();
        dispatch(&promise_ctx, || {
            let bytes = buffer::buffer_source(&data, "data")?;
            let normalized = algorithm::normalize(&algorithm)?;
            let algorithm = normalized.require_implemented("sign")?;

            Ok(async move {
                match algorithm {
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
            })
        })
    }

    /// `verify(algorithm, key, signature, data)`; HMAC only.
    ///
    /// The tag comparison runs in constant time inside [`ops::hmac_verify`].
    #[js_method]
    fn verify(
        &self,
        ctx: JSContext,
        algorithm: JSValue,
        key: CryptoKey,
        signature: JSValue,
        data: JSValue,
    ) -> JSResult<Promise> {
        let promise_ctx = ctx.clone();
        dispatch(&promise_ctx, || {
            let signature = buffer::buffer_source(&signature, "signature")?;
            let bytes = buffer::buffer_source(&data, "data")?;
            let normalized = algorithm::normalize(&algorithm)?;
            let algorithm = normalized.require_implemented("verify")?;

            Ok(async move {
                match algorithm {
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
            })
        })
    }

    /// `encrypt(algorithm, key, data)` for AES-GCM and AES-CBC.
    #[js_method]
    fn encrypt(
        &self,
        ctx: JSContext,
        algorithm: JSValue,
        key: CryptoKey,
        data: JSValue,
    ) -> JSResult<Promise> {
        let promise_ctx = ctx.clone();
        dispatch(&promise_ctx, || {
            let bytes = buffer::buffer_source(&data, "data")?;
            let normalized = algorithm::normalize(&algorithm)?;
            let algorithm = normalized.require_implemented("encrypt")?;
            if !matches!(algorithm, Algorithm::AesGcm | Algorithm::AesCbc) {
                return Err(error::not_supported(format!(
                    "{} is not supported for encrypt",
                    algorithm.canonical_name()
                )));
            }
            key.require(algorithm, KeyUsage::Encrypt)?;
            let iv = require_buffer_param(&normalized, "iv")?;
            let aad = if algorithm == Algorithm::AesGcm {
                check_gcm_tag_length(&normalized)?;
                buffer_param(&normalized, "additionalData")?
            } else {
                Vec::new()
            };

            Ok(async move {
                let result = if algorithm == Algorithm::AesGcm {
                    ops::aes_gcm_encrypt(key.material(), &iv, &aad, &bytes)?
                } else {
                    ops::aes_cbc_encrypt(key.material(), &iv, &bytes)?
                };
                buffer::to_array_buffer(&ctx, result)
            })
        })
    }

    /// `decrypt(algorithm, key, data)` for AES-GCM and AES-CBC.
    #[js_method]
    fn decrypt(
        &self,
        ctx: JSContext,
        algorithm: JSValue,
        key: CryptoKey,
        data: JSValue,
    ) -> JSResult<Promise> {
        let promise_ctx = ctx.clone();
        dispatch(&promise_ctx, || {
            let bytes = buffer::buffer_source(&data, "data")?;
            let normalized = algorithm::normalize(&algorithm)?;
            let algorithm = normalized.require_implemented("decrypt")?;
            if !matches!(algorithm, Algorithm::AesGcm | Algorithm::AesCbc) {
                return Err(error::not_supported(format!(
                    "{} is not supported for decrypt",
                    algorithm.canonical_name()
                )));
            }
            key.require(algorithm, KeyUsage::Decrypt)?;
            let iv = require_buffer_param(&normalized, "iv")?;
            let aad = if algorithm == Algorithm::AesGcm {
                check_gcm_tag_length(&normalized)?;
                buffer_param(&normalized, "additionalData")?
            } else {
                Vec::new()
            };

            Ok(async move {
                let result = if algorithm == Algorithm::AesGcm {
                    ops::aes_gcm_decrypt(key.material(), &iv, &aad, &bytes)?
                } else {
                    ops::aes_cbc_decrypt(key.material(), &iv, &bytes)?
                };
                buffer::to_array_buffer(&ctx, result)
            })
        })
    }

    /// `deriveBits(algorithm, baseKey, length)` for PBKDF2 and HKDF.
    #[js_method(rename = "deriveBits")]
    fn derive_bits(
        &self,
        ctx: JSContext,
        algorithm: JSValue,
        base_key: CryptoKey,
        length: Optional<JSValue>,
    ) -> JSResult<Promise> {
        let promise_ctx = ctx.clone();
        dispatch(&promise_ctx, || {
            let normalized = algorithm::normalize(&algorithm)?;
            let length = length
                .0
                .filter(|value| !value.is_undefined() && !value.is_null())
                .ok_or_else(|| error::operation("deriveBits requires a length in bits"))?;
            let bits = derived_bits_length(length)?;
            let derivation =
                Derivation::prepare(&normalized, &base_key, KeyUsage::DeriveBits, bits)?;
            Ok(async move { buffer::to_array_buffer(&ctx, derivation.run().await?) })
        })
    }

    /// `deriveKey(algorithm, baseKey, derivedKeyAlgorithm, extractable, keyUsages)`.
    #[js_method(rename = "deriveKey")]
    fn derive_key(
        &self,
        ctx: JSContext,
        algorithm: JSValue,
        base_key: CryptoKey,
        derived_key_algorithm: JSValue,
        extractable: bool,
        key_usages: JSValue,
    ) -> JSResult<Promise> {
        let promise_ctx = ctx.clone();
        dispatch(&promise_ctx, || {
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
            let key_algorithm = match derived_id {
                Algorithm::Hmac => {
                    if bits == 0 {
                        return Err(error::operation("HMAC key length must not be zero"));
                    }
                    KeyAlgorithm::Hmac {
                        hash: derived_normalized.require_hash()?,
                        length_bits: bits,
                    }
                }
                _ => {
                    if !ops::AES_KEY_LENGTHS.contains(&bits) {
                        return Err(error::operation("AES key length must be 128, 192 or 256"));
                    }
                    KeyAlgorithm::Aes {
                        algorithm: derived_id,
                        length_bits: bits,
                    }
                }
            };
            let derivation =
                Derivation::prepare(&normalized, &base_key, KeyUsage::DeriveKey, bits)?;
            Ok(async move {
                let secret = derivation.run().await?;
                CryptoKey::secret(key_algorithm, extractable, usages, secret).into_js(&ctx)
            })
        })
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
