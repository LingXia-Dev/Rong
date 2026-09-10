//! Algorithm identifiers and their normalization.
//!
//! `SubtleCrypto` takes an `AlgorithmIdentifier`, which is either a string or a
//! dictionary carrying a `name` member. Every entry point normalizes that value
//! into an [`NormalizedAlgorithm`] first, so each operation only ever matches on
//! [`Algorithm`]. Adding RSA/ECDSA/Ed25519 later is a new match arm in the
//! operation, not a new parsing path: the identifiers are already recognized
//! here and rejected with `NotSupportedError` by
//! [`NormalizedAlgorithm::require_implemented`].

use rong::{JSObject, JSResult, JSValue};

use crate::error;
use crate::hash::HashAlg;

/// Every algorithm name this module recognizes, implemented or not.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Algorithm {
    /// A bare hash, usable with `digest` and as an HMAC/PBKDF2/HKDF parameter.
    Hash(HashAlg),
    Hmac,
    AesCbc,
    AesGcm,
    Pbkdf2,
    Hkdf,
    // Recognized but deliberately out of scope for this pass. Keeping them in
    // the enum means an unknown name and an unimplemented name produce
    // different, more useful messages, and adding support later is local.
    AesCtr,
    AesKw,
    RsassaPkcs1V1_5,
    RsaPss,
    RsaOaep,
    Ecdsa,
    Ecdh,
    Ed25519,
    X25519,
}

impl Algorithm {
    /// Web Crypto matches registered algorithm names ASCII case-insensitively.
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        if let Some(hash) = HashAlg::from_name(name) {
            return Some(Self::Hash(hash));
        }
        let normalized = name.to_ascii_uppercase();
        Some(match normalized.as_str() {
            "HMAC" => Self::Hmac,
            "AES-CBC" => Self::AesCbc,
            "AES-GCM" => Self::AesGcm,
            "AES-CTR" => Self::AesCtr,
            "AES-KW" => Self::AesKw,
            "PBKDF2" => Self::Pbkdf2,
            "HKDF" => Self::Hkdf,
            "RSASSA-PKCS1-V1_5" => Self::RsassaPkcs1V1_5,
            "RSA-PSS" => Self::RsaPss,
            "RSA-OAEP" => Self::RsaOaep,
            "ECDSA" => Self::Ecdsa,
            "ECDH" => Self::Ecdh,
            "ED25519" => Self::Ed25519,
            "X25519" => Self::X25519,
            _ => return None,
        })
    }

    /// The spelling the specification uses, which is also what `CryptoKey`
    /// reports through `key.algorithm.name`.
    pub(crate) fn canonical_name(self) -> &'static str {
        match self {
            Self::Hash(hash) => hash.canonical_name(),
            Self::Hmac => "HMAC",
            Self::AesCbc => "AES-CBC",
            Self::AesGcm => "AES-GCM",
            Self::AesCtr => "AES-CTR",
            Self::AesKw => "AES-KW",
            Self::Pbkdf2 => "PBKDF2",
            Self::Hkdf => "HKDF",
            Self::RsassaPkcs1V1_5 => "RSASSA-PKCS1-v1_5",
            Self::RsaPss => "RSA-PSS",
            Self::RsaOaep => "RSA-OAEP",
            Self::Ecdsa => "ECDSA",
            Self::Ecdh => "ECDH",
            Self::Ed25519 => "Ed25519",
            Self::X25519 => "X25519",
        }
    }

    /// Whether this pass implements the algorithm at all.
    pub(crate) fn is_implemented(self) -> bool {
        matches!(
            self,
            Self::Hash(_) | Self::Hmac | Self::AesCbc | Self::AesGcm | Self::Pbkdf2 | Self::Hkdf
        )
    }

    /// AES modes share key-length and key-usage rules.
    pub(crate) fn is_aes(self) -> bool {
        matches!(
            self,
            Self::AesCbc | Self::AesGcm | Self::AesCtr | Self::AesKw
        )
    }
}

/// An `AlgorithmIdentifier` resolved to a known name plus its dictionary, if
/// the caller passed one.
pub(crate) struct NormalizedAlgorithm {
    pub(crate) algorithm: Algorithm,
    pub(crate) params: Option<JSObject>,
}

impl NormalizedAlgorithm {
    /// Reject anything this build does not implement, naming the algorithm.
    pub(crate) fn require_implemented(&self, operation: &str) -> JSResult<Algorithm> {
        if self.algorithm.is_implemented() {
            return Ok(self.algorithm);
        }
        Err(error::not_supported(format!(
            "{} is not supported for {}",
            self.algorithm.canonical_name(),
            operation
        )))
    }

    /// Read a member of the algorithm dictionary, if one was supplied.
    pub(crate) fn param(&self, key: &str) -> JSResult<Option<JSValue>> {
        let Some(params) = self.params.as_ref() else {
            return Ok(None);
        };
        if !params.has_property(key)? {
            return Ok(None);
        }
        let value = params.get::<_, JSValue>(key)?;
        Ok(if value.is_undefined() || value.is_null() {
            None
        } else {
            Some(value)
        })
    }

    /// Read a required member of the algorithm dictionary.
    pub(crate) fn require_param(&self, key: &str) -> JSResult<JSValue> {
        self.param(key)?.ok_or_else(|| {
            error::type_error(format!(
                "{}: required member '{}' is missing",
                self.algorithm.canonical_name(),
                key
            ))
        })
    }

    /// Read the `hash` member, which is itself an `AlgorithmIdentifier`.
    pub(crate) fn require_hash(&self) -> JSResult<HashAlg> {
        let value = self.require_param("hash")?;
        let inner = normalize(&value)?;
        match inner.algorithm {
            Algorithm::Hash(hash) => Ok(hash),
            other => Err(error::not_supported(format!(
                "{} is not a supported hash algorithm",
                other.canonical_name()
            ))),
        }
    }
}

/// Turn an `AlgorithmIdentifier` (string or `{ name }` dictionary) into a
/// [`NormalizedAlgorithm`].
pub(crate) fn normalize(value: &JSValue) -> JSResult<NormalizedAlgorithm> {
    let (requested_name, params) = if value.is_string() {
        (value.clone().to_rust::<String>()?, None)
    } else if let Some(object) = value.clone().into_object() {
        let name = object
            .get::<_, String>("name")
            .map_err(|_| error::type_error("algorithm object is missing a string 'name' member"))?;
        (name, Some(object))
    } else {
        return Err(error::type_error(
            "algorithm must be a string or an object with a 'name' member",
        ));
    };

    let algorithm = Algorithm::from_name(&requested_name).ok_or_else(|| {
        error::not_supported(format!("Unrecognized algorithm name: {requested_name}"))
    })?;

    Ok(NormalizedAlgorithm { algorithm, params })
}
