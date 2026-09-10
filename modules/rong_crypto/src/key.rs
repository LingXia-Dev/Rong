//! `CryptoKey` and the key-usage rules the operations enforce.

use rong::{Class, JSArray, JSContext, JSObject, JSResult, JSValue, js_class, js_method};

use crate::algorithm::Algorithm;
use crate::error;
use crate::hash::HashAlg;

/// A member of the `KeyUsage` enumeration.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum KeyUsage {
    Encrypt,
    Decrypt,
    Sign,
    Verify,
    DeriveKey,
    DeriveBits,
    WrapKey,
    UnwrapKey,
}

impl KeyUsage {
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "encrypt" => Self::Encrypt,
            "decrypt" => Self::Decrypt,
            "sign" => Self::Sign,
            "verify" => Self::Verify,
            "deriveKey" => Self::DeriveKey,
            "deriveBits" => Self::DeriveBits,
            "wrapKey" => Self::WrapKey,
            "unwrapKey" => Self::UnwrapKey,
            _ => return None,
        })
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Encrypt => "encrypt",
            Self::Decrypt => "decrypt",
            Self::Sign => "sign",
            Self::Verify => "verify",
            Self::DeriveKey => "deriveKey",
            Self::DeriveBits => "deriveBits",
            Self::WrapKey => "wrapKey",
            Self::UnwrapKey => "unwrapKey",
        }
    }
}

/// Read a `KeyUsage[]` argument.
pub(crate) fn parse_usages(value: &JSValue) -> JSResult<Vec<KeyUsage>> {
    let array = value
        .clone()
        .into_object()
        .and_then(JSArray::from_object)
        .ok_or_else(|| error::type_error("keyUsages must be an array of key usage strings"))?;

    let mut usages = Vec::new();
    for entry in array.iter_values()? {
        let name = entry?
            .to_rust::<String>()
            .map_err(|_| error::type_error("keyUsages entries must be key usage strings"))?;
        let usage = KeyUsage::from_name(&name)
            .ok_or_else(|| error::type_error(format!("'{name}' is not a valid key usage")))?;
        if !usages.contains(&usage) {
            usages.push(usage);
        }
    }
    Ok(usages)
}

/// The algorithm-specific half of `CryptoKey.algorithm`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum KeyAlgorithm {
    /// `HmacKeyAlgorithm`: `{ name, hash: { name }, length }` (length in bits).
    Hmac { hash: HashAlg, length_bits: usize },
    /// `AesKeyAlgorithm`: `{ name, length }` (length in bits).
    Aes {
        algorithm: Algorithm,
        length_bits: usize,
    },
    /// `KeyAlgorithm`: `{ name }`, used by the PBKDF2/HKDF base keys.
    Derivation { algorithm: Algorithm },
}

impl KeyAlgorithm {
    pub(crate) fn algorithm(self) -> Algorithm {
        match self {
            Self::Hmac { .. } => Algorithm::Hmac,
            Self::Aes { algorithm, .. } | Self::Derivation { algorithm } => algorithm,
        }
    }

    pub(crate) fn name(self) -> &'static str {
        self.algorithm().canonical_name()
    }

    /// Build the JavaScript-visible `key.algorithm` dictionary.
    fn to_js_object(self, ctx: &JSContext) -> JSResult<JSObject> {
        let object = JSObject::new(ctx);
        object.set("name", self.name())?;
        match self {
            Self::Hmac { hash, length_bits } => {
                let hash_object = JSObject::new(ctx);
                hash_object.set("name", hash.canonical_name())?;
                object.set("hash", hash_object)?;
                object.set("length", length_bits as f64)?;
            }
            Self::Aes { length_bits, .. } => {
                object.set("length", length_bits as f64)?;
            }
            Self::Derivation { .. } => {}
        }
        Ok(object)
    }
}

/// The Web Cryptography `CryptoKey` interface.
///
/// Instances are only ever produced by `SubtleCrypto`; the constructor is
/// unreachable from JavaScript, matching the platform.
#[js_class(clone)]
pub struct CryptoKey {
    key_type: &'static str,
    extractable: bool,
    algorithm: KeyAlgorithm,
    usages: Vec<KeyUsage>,
    /// Raw key material. Only symmetric keys exist in this pass, so this is
    /// always the secret itself.
    secret: Vec<u8>,
}

impl CryptoKey {
    pub(crate) fn secret(
        algorithm: KeyAlgorithm,
        extractable: bool,
        usages: Vec<KeyUsage>,
        secret: Vec<u8>,
    ) -> Self {
        Self {
            key_type: "secret",
            extractable,
            algorithm,
            usages,
            secret,
        }
    }

    pub(crate) fn key_algorithm(&self) -> KeyAlgorithm {
        self.algorithm
    }

    pub(crate) fn is_extractable(&self) -> bool {
        self.extractable
    }

    pub(crate) fn usage_list(&self) -> &[KeyUsage] {
        &self.usages
    }

    pub(crate) fn material(&self) -> &[u8] {
        &self.secret
    }

    /// Reject a key that was not imported or generated for this operation, and
    /// a key whose usage list does not cover it.
    pub(crate) fn require(&self, algorithm: Algorithm, usage: KeyUsage) -> JSResult<()> {
        if self.algorithm.algorithm() != algorithm {
            return Err(error::invalid_access(format!(
                "key algorithm is {}, but the requested operation is {}",
                self.algorithm.name(),
                algorithm.canonical_name()
            )));
        }
        if !self.usages.contains(&usage) {
            return Err(error::invalid_access(format!(
                "key usages do not include '{}'",
                usage.as_str()
            )));
        }
        Ok(())
    }

    /// Turn this key into a JavaScript `CryptoKey` object.
    pub(crate) fn into_js(self, ctx: &JSContext) -> JSResult<JSObject> {
        Ok(Class::lookup::<Self>(ctx)?.instance(self))
    }
}

#[js_class]
impl CryptoKey {
    #[js_method(constructor, private)]
    fn new() -> JSResult<Self> {
        rong::illegal_constructor(
            "CryptoKey cannot be constructed directly. Use crypto.subtle.importKey(), \
             generateKey(), or deriveKey().",
        )
    }

    #[js_method(getter, enumerable, rename = "type")]
    fn js_type(&self) -> String {
        self.key_type.to_string()
    }

    #[js_method(getter, enumerable)]
    fn extractable(&self) -> bool {
        self.extractable
    }

    #[js_method(getter, enumerable)]
    fn algorithm(&self, ctx: JSContext) -> JSResult<JSObject> {
        self.algorithm.to_js_object(&ctx)
    }

    #[js_method(getter, enumerable)]
    fn usages(&self, ctx: JSContext) -> JSResult<JSArray> {
        let array = JSArray::new(&ctx)?;
        for usage in &self.usages {
            array.push(usage.as_str())?;
        }
        Ok(array)
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }
}
