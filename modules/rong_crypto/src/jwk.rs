//! JSON Web Key import/export for symmetric (`kty: "oct"`) keys.
//!
//! Only the members the Web Cryptography API defines for HMAC and AES keys are
//! handled: `kty`, `k`, `alg`, `ext`, `key_ops` and `use` (RFC 7517 §4,
//! RFC 7518 §6.4).

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rong::{JSArray, JSContext, JSObject, JSResult, JSValue};

use crate::algorithm::Algorithm;
use crate::error;
use crate::key::{CryptoKey, KeyAlgorithm, KeyUsage};

/// The `alg` value a symmetric key of this algorithm must carry, if any.
fn expected_alg(algorithm: KeyAlgorithm) -> Option<String> {
    match algorithm {
        KeyAlgorithm::Hmac { hash, .. } => Some(hash.jwk_hmac_alg().to_string()),
        KeyAlgorithm::Aes {
            algorithm,
            length_bits,
        } => {
            let suffix = match algorithm {
                Algorithm::AesCbc => "CBC",
                Algorithm::AesGcm => "GCM",
                Algorithm::AesCtr => "CTR",
                Algorithm::AesKw => "KW",
                _ => return None,
            };
            Some(format!("A{length_bits}{suffix}"))
        }
        KeyAlgorithm::Derivation { .. } => None,
    }
}

/// Decode the key material from a JWK object.
///
/// This runs before the key algorithm is known, because for AES the key length
/// - and therefore the expected `alg` - follows from the decoded material.
pub(crate) fn decode_secret(jwk: &JSObject) -> JSResult<Vec<u8>> {
    let kty = jwk
        .get::<_, String>("kty")
        .map_err(|_| error::data("JWK is missing a string 'kty' member"))?;
    if kty != "oct" {
        return Err(error::data(format!(
            "JWK kty '{kty}' cannot be imported as a symmetric key; expected 'oct'"
        )));
    }

    let encoded = jwk
        .get::<_, String>("k")
        .map_err(|_| error::data("JWK is missing a string 'k' member"))?;
    URL_SAFE_NO_PAD
        .decode(encoded.as_bytes())
        .map_err(|error| error::data(format!("JWK 'k' is not valid base64url: {error}")))
}

fn jwk_string(jwk: &JSObject, member: &str) -> JSResult<String> {
    jwk.get::<_, JSValue>(member)?
        .to_rust::<String>()
        .map_err(|_| error::data(format!("JWK '{member}' must be a string")))
}

/// Check the JWK members that constrain how the key may be used, once the key
/// algorithm is known.
pub(crate) fn validate(
    jwk: &JSObject,
    algorithm: KeyAlgorithm,
    extractable: bool,
    usages: &[KeyUsage],
) -> JSResult<()> {
    if jwk.has_property("use")? {
        let use_value = jwk_string(jwk, "use")?;
        let expected = match algorithm {
            KeyAlgorithm::Hmac { .. } => "sig",
            KeyAlgorithm::Aes { .. } => "enc",
            KeyAlgorithm::Derivation { .. } => {
                return Err(error::data(
                    "JWK 'use' is not valid for a key-derivation base key",
                ));
            }
        };
        if !usages.is_empty() && use_value != expected {
            return Err(error::data(format!(
                "JWK use '{use_value}' is not valid for this algorithm (expected '{expected}')"
            )));
        }
    }

    if jwk.has_property("alg")?
        && let Some(expected) = expected_alg(algorithm)
    {
        let alg = jwk_string(jwk, "alg")?;
        if alg != expected {
            return Err(error::data(format!(
                "JWK alg '{alg}' does not match the requested algorithm (expected '{expected}')"
            )));
        }
    }

    if jwk.has_property("ext")? {
        let ext = jwk
            .get::<_, JSValue>("ext")?
            .to_rust::<bool>()
            .map_err(|_| error::data("JWK 'ext' must be a boolean"))?;
        if !ext && extractable {
            return Err(error::data(
                "JWK is marked non-extractable but an extractable key was requested",
            ));
        }
    }

    if jwk.has_property("key_ops")? {
        let ops = jwk.get::<_, JSValue>("key_ops")?;
        let ops = ops
            .into_object()
            .and_then(JSArray::from_object)
            .ok_or_else(|| error::data("JWK 'key_ops' must be an array of operation strings"))?;
        let mut allowed = Vec::new();
        for entry in ops.iter_values()? {
            let name = entry?
                .to_rust::<String>()
                .map_err(|_| error::data("JWK 'key_ops' entries must be strings"))?;
            let Some(usage) = KeyUsage::from_name(&name) else {
                // RFC 7517 allows other operation names; they simply do not
                // satisfy a Web Crypto usage.
                continue;
            };
            if allowed.contains(&usage) {
                return Err(error::data(format!(
                    "JWK key_ops contains duplicate '{name}'"
                )));
            }
            allowed.push(usage);
        }
        if let Some(missing) = usages.iter().find(|usage| !allowed.contains(usage)) {
            return Err(error::data(format!(
                "JWK key_ops does not permit '{}'",
                missing.as_str()
            )));
        }
    }

    Ok(())
}

/// Build the JWK object for an already-validated extractable key.
pub(crate) fn export(ctx: &JSContext, key: &CryptoKey) -> JSResult<JSObject> {
    let jwk = JSObject::new(ctx);
    jwk.set("kty", "oct")?;
    jwk.set("k", URL_SAFE_NO_PAD.encode(key.material()))?;
    if let Some(alg) = expected_alg(key.key_algorithm()) {
        jwk.set("alg", alg)?;
    }
    jwk.set("ext", key.is_extractable())?;

    let key_ops = JSArray::new(ctx)?;
    for usage in key.usage_list() {
        key_ops.push(usage.as_str())?;
    }
    jwk.set("key_ops", key_ops)?;
    Ok(jwk)
}
