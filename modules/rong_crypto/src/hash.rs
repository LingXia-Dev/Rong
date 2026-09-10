//! The hash functions shared by `digest`, HMAC, PBKDF2 and HKDF.

use sha1::Sha1;
use sha2::{Digest, Sha256, Sha384, Sha512};

/// The four hashes the Web Cryptography API registers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum HashAlg {
    Sha1,
    Sha256,
    Sha384,
    Sha512,
}

impl HashAlg {
    /// Names are matched ASCII case-insensitively, like every other registered
    /// algorithm name.
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Some(match name.to_ascii_uppercase().as_str() {
            "SHA-1" => Self::Sha1,
            "SHA-256" => Self::Sha256,
            "SHA-384" => Self::Sha384,
            "SHA-512" => Self::Sha512,
            _ => return None,
        })
    }

    pub(crate) fn canonical_name(self) -> &'static str {
        match self {
            Self::Sha1 => "SHA-1",
            Self::Sha256 => "SHA-256",
            Self::Sha384 => "SHA-384",
            Self::Sha512 => "SHA-512",
        }
    }

    /// Digest size in bytes.
    pub(crate) fn output_len(self) -> usize {
        match self {
            Self::Sha1 => 20,
            Self::Sha256 => 32,
            Self::Sha384 => 48,
            Self::Sha512 => 64,
        }
    }

    /// Compression-function block size in bytes. `generateKey` for HMAC uses it
    /// as the default key length, per the HMAC key generation steps.
    pub(crate) fn block_len(self) -> usize {
        match self {
            Self::Sha1 | Self::Sha256 => 64,
            Self::Sha384 | Self::Sha512 => 128,
        }
    }

    pub(crate) fn digest(self, data: &[u8]) -> Vec<u8> {
        match self {
            Self::Sha1 => Sha1::digest(data).to_vec(),
            Self::Sha256 => Sha256::digest(data).to_vec(),
            Self::Sha384 => Sha384::digest(data).to_vec(),
            Self::Sha512 => Sha512::digest(data).to_vec(),
        }
    }

    /// The JWK `alg` value for an HMAC key using this hash (RFC 7518 §3.2).
    pub(crate) fn jwk_hmac_alg(self) -> &'static str {
        match self {
            Self::Sha1 => "HS1",
            Self::Sha256 => "HS256",
            Self::Sha384 => "HS384",
            Self::Sha512 => "HS512",
        }
    }
}
