//! The primitive operations behind `SubtleCrypto`, kept free of JavaScript
//! types so they can be unit tested directly against published test vectors.

use aes::{Aes128, Aes192, Aes256};
use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{AesGcm, KeyInit as AeadKeyInit, Nonce};
use cbc::cipher::block_padding::Pkcs7;
use cbc::cipher::{BlockModeDecrypt, BlockModeEncrypt, KeyIvInit, consts::U12};
use hkdf::Hkdf;
use hmac::digest::block_api::EagerHash;
use hmac::{Hmac, KeyInit, Mac};
use rong::{JSResult, RongJSError};
use sha1::Sha1;
use sha2::{Sha256, Sha384, Sha512};

use crate::error;
use crate::hash::HashAlg;

/// AES-GCM as this module constrains it: a 96-bit nonce and a 128-bit tag.
type Aes128Gcm = AesGcm<Aes128, U12>;
type Aes192Gcm = AesGcm<Aes192, U12>;
type Aes256Gcm = AesGcm<Aes256, U12>;

type Aes128CbcEnc = cbc::Encryptor<Aes128>;
type Aes192CbcEnc = cbc::Encryptor<Aes192>;
type Aes256CbcEnc = cbc::Encryptor<Aes256>;
type Aes128CbcDec = cbc::Decryptor<Aes128>;
type Aes192CbcDec = cbc::Decryptor<Aes192>;
type Aes256CbcDec = cbc::Decryptor<Aes256>;

/// AES-GCM requires a 96-bit IV in this implementation.
pub(crate) const GCM_IV_LEN: usize = 12;
/// AES-CBC requires a full-block IV.
pub(crate) const CBC_IV_LEN: usize = 16;
/// AES key sizes the Web Cryptography API permits, in bits.
pub(crate) const AES_KEY_LENGTHS: [usize; 3] = [128, 192, 256];

/// Compute an HMAC tag.
pub(crate) fn hmac_sign(hash: HashAlg, key: &[u8], data: &[u8]) -> Vec<u8> {
    fn run<D: EagerHash>(key: &[u8], data: &[u8]) -> Vec<u8> {
        let mut mac =
            <Hmac<D> as KeyInit>::new_from_slice(key).expect("HMAC accepts a key of any length");
        mac.update(data);
        mac.finalize().into_bytes().to_vec()
    }

    match hash {
        HashAlg::Sha1 => run::<Sha1>(key, data),
        HashAlg::Sha256 => run::<Sha256>(key, data),
        HashAlg::Sha384 => run::<Sha384>(key, data),
        HashAlg::Sha512 => run::<Sha512>(key, data),
    }
}

/// Verify an HMAC tag.
///
/// The comparison is delegated to `Mac::verify_slice`, which compares through
/// `CtOutput`/`subtle` in constant time. Tag bytes are never compared with `==`.
pub(crate) fn hmac_verify(hash: HashAlg, key: &[u8], data: &[u8], signature: &[u8]) -> bool {
    fn run<D: EagerHash>(key: &[u8], data: &[u8], signature: &[u8]) -> bool {
        let mut mac =
            <Hmac<D> as KeyInit>::new_from_slice(key).expect("HMAC accepts a key of any length");
        mac.update(data);
        mac.verify_slice(signature).is_ok()
    }

    match hash {
        HashAlg::Sha1 => run::<Sha1>(key, data, signature),
        HashAlg::Sha256 => run::<Sha256>(key, data, signature),
        HashAlg::Sha384 => run::<Sha384>(key, data, signature),
        HashAlg::Sha512 => run::<Sha512>(key, data, signature),
    }
}

fn bad_aes_key_len(algorithm: &str, len: usize) -> RongJSError {
    error::operation(format!(
        "{algorithm} key must be 16, 24 or 32 bytes, got {len}"
    ))
}

fn bad_iv_len(algorithm: &str, expected: usize, len: usize) -> RongJSError {
    error::operation(format!(
        "{algorithm} requires a {expected}-byte iv, got {len}"
    ))
}

fn unsupported_gcm_iv(len: usize) -> RongJSError {
    error::not_supported(format!(
        "AES-GCM only supports a 12-byte (96-bit) iv, got {len}"
    ))
}

/// Run `$body` with `$cipher` bound to the AES variant matching the key length.
macro_rules! by_aes_key_len {
    ($algorithm:literal, $key:expr, |$cipher:ident| $body:expr, $c128:ty, $c192:ty, $c256:ty) => {
        match $key.len() {
            16 => {
                type $cipher = $c128;
                $body
            }
            24 => {
                type $cipher = $c192;
                $body
            }
            32 => {
                type $cipher = $c256;
                $body
            }
            other => Err(bad_aes_key_len($algorithm, other)),
        }
    };
}

/// `AES-GCM` encryption; the 128-bit tag is appended to the ciphertext, which
/// is the layout the Web Cryptography API specifies.
pub(crate) fn aes_gcm_encrypt(
    key: &[u8],
    iv: &[u8],
    aad: &[u8],
    plaintext: &[u8],
) -> JSResult<Vec<u8>> {
    if iv.len() != GCM_IV_LEN {
        return Err(unsupported_gcm_iv(iv.len()));
    }
    let nonce = Nonce::<U12>::try_from(iv).map_err(|_| unsupported_gcm_iv(iv.len()))?;

    by_aes_key_len!(
        "AES-GCM",
        key,
        |Cipher| {
            let cipher = <Cipher as AeadKeyInit>::new_from_slice(key)
                .map_err(|_| bad_aes_key_len("AES-GCM", key.len()))?;
            cipher
                .encrypt(
                    &nonce,
                    Payload {
                        msg: plaintext,
                        aad,
                    },
                )
                .map_err(|_| error::operation("AES-GCM encryption failed"))
        },
        Aes128Gcm,
        Aes192Gcm,
        Aes256Gcm
    )
}

/// `AES-GCM` decryption. A wrong key, IV, additional data or tag all surface as
/// `OperationError`, exactly as the specification requires.
pub(crate) fn aes_gcm_decrypt(
    key: &[u8],
    iv: &[u8],
    aad: &[u8],
    ciphertext: &[u8],
) -> JSResult<Vec<u8>> {
    if iv.len() != GCM_IV_LEN {
        return Err(unsupported_gcm_iv(iv.len()));
    }
    let nonce = Nonce::<U12>::try_from(iv).map_err(|_| unsupported_gcm_iv(iv.len()))?;

    by_aes_key_len!(
        "AES-GCM",
        key,
        |Cipher| {
            let cipher = <Cipher as AeadKeyInit>::new_from_slice(key)
                .map_err(|_| bad_aes_key_len("AES-GCM", key.len()))?;
            cipher
                .decrypt(
                    &nonce,
                    Payload {
                        msg: ciphertext,
                        aad,
                    },
                )
                .map_err(|_| error::operation("AES-GCM decryption failed"))
        },
        Aes128Gcm,
        Aes192Gcm,
        Aes256Gcm
    )
}

/// `AES-CBC` encryption with the PKCS#7 padding the specification mandates.
pub(crate) fn aes_cbc_encrypt(key: &[u8], iv: &[u8], plaintext: &[u8]) -> JSResult<Vec<u8>> {
    if iv.len() != CBC_IV_LEN {
        return Err(bad_iv_len("AES-CBC", CBC_IV_LEN, iv.len()));
    }

    by_aes_key_len!(
        "AES-CBC",
        key,
        |Cipher| {
            let cipher = <Cipher as KeyIvInit>::new_from_slices(key, iv)
                .map_err(|_| bad_aes_key_len("AES-CBC", key.len()))?;
            Ok(cipher.encrypt_padded_vec::<Pkcs7>(plaintext))
        },
        Aes128CbcEnc,
        Aes192CbcEnc,
        Aes256CbcEnc
    )
}

/// `AES-CBC` decryption; invalid padding is an `OperationError`, which is also
/// what a wrong key or IV produces.
pub(crate) fn aes_cbc_decrypt(key: &[u8], iv: &[u8], ciphertext: &[u8]) -> JSResult<Vec<u8>> {
    if iv.len() != CBC_IV_LEN {
        return Err(bad_iv_len("AES-CBC", CBC_IV_LEN, iv.len()));
    }
    if ciphertext.is_empty() || !ciphertext.len().is_multiple_of(CBC_IV_LEN) {
        return Err(error::operation(
            "AES-CBC ciphertext length must be a non-zero multiple of 16",
        ));
    }

    by_aes_key_len!(
        "AES-CBC",
        key,
        |Cipher| {
            let cipher = <Cipher as KeyIvInit>::new_from_slices(key, iv)
                .map_err(|_| bad_aes_key_len("AES-CBC", key.len()))?;
            cipher
                .decrypt_padded_vec::<Pkcs7>(ciphertext)
                .map_err(|_| error::operation("AES-CBC decryption failed"))
        },
        Aes128CbcDec,
        Aes192CbcDec,
        Aes256CbcDec
    )
}

/// PBKDF2 (RFC 2898) with HMAC as the pseudo-random function.
pub(crate) fn pbkdf2(
    hash: HashAlg,
    password: &[u8],
    salt: &[u8],
    iterations: u32,
    out_len: usize,
) -> Vec<u8> {
    let mut out = vec![0u8; out_len];
    match hash {
        HashAlg::Sha1 => ::pbkdf2::pbkdf2_hmac::<Sha1>(password, salt, iterations, &mut out),
        HashAlg::Sha256 => ::pbkdf2::pbkdf2_hmac::<Sha256>(password, salt, iterations, &mut out),
        HashAlg::Sha384 => ::pbkdf2::pbkdf2_hmac::<Sha384>(password, salt, iterations, &mut out),
        HashAlg::Sha512 => ::pbkdf2::pbkdf2_hmac::<Sha512>(password, salt, iterations, &mut out),
    }
    out
}

/// HKDF (RFC 5869): extract-then-expand.
pub(crate) fn hkdf(
    hash: HashAlg,
    ikm: &[u8],
    salt: &[u8],
    info: &[u8],
    out_len: usize,
) -> JSResult<Vec<u8>> {
    fn run<D: EagerHash>(
        ikm: &[u8],
        salt: &[u8],
        info: &[u8],
        out_len: usize,
    ) -> JSResult<Vec<u8>> {
        let mut out = vec![0u8; out_len];
        Hkdf::<D>::new(Some(salt), ikm)
            .expand(info, &mut out)
            .map_err(|_| error::operation("HKDF expansion failed"))?;
        Ok(out)
    }

    // RFC 5869 section 2.3 caps HKDF-Expand at 255 * HashLen octets.
    let max_len = 255 * hash.output_len();
    if out_len > max_len {
        return Err(error::operation(format!(
            "HKDF with {} cannot derive more than {max_len} bytes, requested {out_len}",
            hash.canonical_name()
        )));
    }

    match hash {
        HashAlg::Sha1 => run::<Sha1>(ikm, salt, info, out_len),
        HashAlg::Sha256 => run::<Sha256>(ikm, salt, info, out_len),
        HashAlg::Sha384 => run::<Sha384>(ikm, salt, info, out_len),
        HashAlg::Sha512 => run::<Sha512>(ikm, salt, info, out_len),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unhex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&text[index..index + 2], 16).expect("valid hex"))
            .collect()
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn digests_match_fips_180_4_one_block_vectors() {
        // FIPS 180-4 / NIST secure hashing examples for the message "abc".
        assert_eq!(
            hex(&HashAlg::Sha1.digest(b"abc")),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hex(&HashAlg::Sha256.digest(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(&HashAlg::Sha384.digest(b"abc")),
            concat!(
                "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed",
                "8086072ba1e7cc2358baeca134c825a7"
            )
        );
        assert_eq!(
            hex(&HashAlg::Sha512.digest(b"abc")),
            concat!(
                "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a",
                "2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
            )
        );
    }

    #[test]
    fn hmac_matches_rfc_2202_and_rfc_4231_case_1() {
        // RFC 2202 test case 1 (SHA-1) and RFC 4231 test case 1 (SHA-2 family):
        // key = 0x0b repeated 20 times, data = "Hi There".
        let key = [0x0b_u8; 20];
        let data = b"Hi There";

        assert_eq!(
            hex(&hmac_sign(HashAlg::Sha1, &key, data)),
            "b617318655057264e28bc0b6fb378c8ef146be00"
        );
        assert_eq!(
            hex(&hmac_sign(HashAlg::Sha256, &key, data)),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
        assert_eq!(
            hex(&hmac_sign(HashAlg::Sha384, &key, data)),
            concat!(
                "afd03944d84895626b0825f4ab46907f15f9dadbe4101ec682aa034c7cebc59c",
                "faea9ea9076ede7f4af152e8b2fa9cb6"
            )
        );
        assert_eq!(
            hex(&hmac_sign(HashAlg::Sha512, &key, data)),
            concat!(
                "87aa7cdea5ef619d4ff0b4241a1d6cb02379f4e2ce4ec2787ad0b30545e17cde",
                "daa833b7d6b8a702038b274eaea3f4e4be9d914eeb61f1702e696c203a126854"
            )
        );
    }

    #[test]
    fn hmac_verify_accepts_only_the_exact_tag() {
        let key = b"Jefe";
        let data = b"what do ya want for nothing?";
        let mut signature = hmac_sign(HashAlg::Sha256, key, data);
        // RFC 4231 test case 2.
        assert_eq!(
            hex(&signature),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        assert!(hmac_verify(HashAlg::Sha256, key, data, &signature));

        signature[0] ^= 0x01;
        assert!(!hmac_verify(HashAlg::Sha256, key, data, &signature));

        let short = hmac_sign(HashAlg::Sha256, key, data);
        assert!(!hmac_verify(HashAlg::Sha256, key, data, &short[..16]));
        assert!(!hmac_verify(HashAlg::Sha256, b"other", data, &short));
    }

    #[test]
    fn aes_gcm_matches_mcgrew_viega_vectors() {
        // "The Galois/Counter Mode of Operation (GCM)", McGrew & Viega,
        // test cases 3 (AES-128), 13 and 14 (AES-256).
        let key = unhex("feffe9928665731c6d6a8f9467308308");
        let iv = unhex("cafebabefacedbaddecaf888");
        let plaintext = unhex(concat!(
            "d9313225f88406e5a55909c5aff5269a86a7a9531534f7da2e4c303d8a318a72",
            "1c3c0c95956809532fcf0e2449a6b525b16aedf5aa0de657ba637b391aafd255"
        ));
        let ciphertext = aes_gcm_encrypt(&key, &iv, &[], &plaintext).expect("encrypts");
        assert_eq!(
            hex(&ciphertext),
            concat!(
                "42831ec2217774244b7221b784d0d49ce3aa212f2c02a4e035c17e2329aca12e",
                "21d514b25466931c7d8f6a5aac84aa051ba30b396a0aac973d58e091473f5985",
                "4d5c2af327cd64a62cf35abd2ba6fab4"
            )
        );
        assert_eq!(
            aes_gcm_decrypt(&key, &iv, &[], &ciphertext).expect("decrypts"),
            plaintext
        );

        let zero_key = [0u8; 32];
        let zero_iv = [0u8; 12];
        assert_eq!(
            hex(&aes_gcm_encrypt(&zero_key, &zero_iv, &[], &[]).expect("encrypts")),
            "530f8afbc74536b9a963b4f1c4cb738b"
        );
        assert_eq!(
            hex(&aes_gcm_encrypt(&zero_key, &zero_iv, &[], &[0u8; 16]).expect("encrypts")),
            "cea7403d4d606b6e074ec5d3baf39d18d0d1c8a799996bf0265b98b5d48ab919"
        );
    }

    #[test]
    fn aes_gcm_rejects_a_wrong_key_iv_or_aad() {
        let key = [1u8; 16];
        let iv = [2u8; GCM_IV_LEN];
        let ciphertext = aes_gcm_encrypt(&key, &iv, b"aad", b"message").expect("encrypts");

        assert!(aes_gcm_decrypt(&[9u8; 16], &iv, b"aad", &ciphertext).is_err());
        assert!(aes_gcm_decrypt(&key, &[3u8; GCM_IV_LEN], b"aad", &ciphertext).is_err());
        assert!(aes_gcm_decrypt(&key, &iv, b"other", &ciphertext).is_err());
        assert_eq!(
            aes_gcm_decrypt(&key, &iv, b"aad", &ciphertext).expect("decrypts"),
            b"message"
        );
    }

    #[test]
    fn aes_gcm_rejects_an_iv_that_is_not_96_bits() {
        assert!(aes_gcm_encrypt(&[0u8; 16], &[0u8; 16], &[], b"x").is_err());
        assert!(aes_gcm_encrypt(&[0u8; 16], &[], &[], b"x").is_err());
    }

    #[test]
    fn aes_cbc_matches_nist_sp_800_38a_f_2_1() {
        // NIST SP 800-38A, F.2.1 CBC-AES128.Encrypt. The Web Cryptography API
        // always applies PKCS#7, so a full-block message gains a padding block.
        let key = unhex("2b7e151628aed2a6abf7158809cf4f3c");
        let iv = unhex("000102030405060708090a0b0c0d0e0f");
        let plaintext = unhex(concat!(
            "6bc1bee22e409f96e93d7e117393172a",
            "ae2d8a571e03ac9c9eb76fac45af8e51",
            "30c81c46a35ce411e5fbc1191a0a52ef",
            "f69f2445df4f9b17ad2b417be66c3710"
        ));
        let ciphertext = aes_cbc_encrypt(&key, &iv, &plaintext).expect("encrypts");
        assert_eq!(ciphertext.len(), plaintext.len() + 16);
        assert_eq!(
            hex(&ciphertext[..plaintext.len()]),
            concat!(
                "7649abac8119b246cee98e9b12e9197d",
                "5086cb9b507219ee95db113a917678b2",
                "73bed6b8e3c1743b7116e69e22229516",
                "3ff1caa1681fac09120eca307586e1a7"
            )
        );
        assert_eq!(
            aes_cbc_decrypt(&key, &iv, &ciphertext).expect("decrypts"),
            plaintext
        );
    }

    #[test]
    fn aes_cbc_rejects_a_wrong_key_and_a_bad_length() {
        let key = [4u8; 32];
        let iv = [5u8; CBC_IV_LEN];
        let ciphertext = aes_cbc_encrypt(&key, &iv, b"secret message").expect("encrypts");

        // A wrong key almost always produces invalid PKCS#7 padding.
        assert!(aes_cbc_decrypt(&[6u8; 32], &iv, &ciphertext).is_err());
        assert!(aes_cbc_decrypt(&key, &iv, &ciphertext[..15]).is_err());
        assert!(aes_cbc_decrypt(&key, &iv, &[]).is_err());
        assert!(aes_cbc_encrypt(&key, &[0u8; 8], b"x").is_err());
    }

    #[test]
    fn pbkdf2_matches_rfc_6070_and_rfc_7914() {
        // RFC 6070 test vectors for PBKDF2-HMAC-SHA-1.
        assert_eq!(
            hex(&pbkdf2(HashAlg::Sha1, b"password", b"salt", 1, 20)),
            "0c60c80f961f0e71f3a9b524af6012062fe037a6"
        );
        assert_eq!(
            hex(&pbkdf2(HashAlg::Sha1, b"password", b"salt", 2, 20)),
            "ea6c014dc72d6f8ccd1ed92ace1d41f0d8de8957"
        );
        assert_eq!(
            hex(&pbkdf2(HashAlg::Sha1, b"password", b"salt", 4096, 20)),
            "4b007901b765489abead49d926f721d065a429c1"
        );
        // RFC 7914 section 11, PBKDF2-HMAC-SHA-256.
        assert_eq!(
            hex(&pbkdf2(HashAlg::Sha256, b"passwd", b"salt", 1, 64)),
            concat!(
                "55ac046e56e3089fec1691c22544b605f94185216dde0465e68b9d57c20dacbc",
                "49ca9cccf179b645991664b39d77ef317c71b845b1e30bd509112041d3a19783"
            )
        );
    }

    #[test]
    fn hkdf_matches_rfc_5869_test_cases_1_and_3() {
        let ikm = [0x0b_u8; 22];
        let okm = hkdf(
            HashAlg::Sha256,
            &ikm,
            &unhex("000102030405060708090a0b0c"),
            &unhex("f0f1f2f3f4f5f6f7f8f9"),
            42,
        )
        .expect("expands");
        assert_eq!(
            hex(&okm),
            concat!(
                "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf",
                "34007208d5b887185865"
            )
        );

        // Test case 3: zero-length salt and info.
        let okm = hkdf(HashAlg::Sha256, &ikm, &[], &[], 42).expect("expands");
        assert_eq!(
            hex(&okm),
            concat!(
                "8da4e775a563c18f715f802a063c5a31b8a11f5c5ee1879ec3454e5f3c738d2d",
                "9d201395faa4b61a96c8"
            )
        );
    }

    #[test]
    fn hkdf_rejects_more_than_255_hash_lengths() {
        assert!(hkdf(HashAlg::Sha256, b"ikm", b"salt", b"", 255 * 32).is_ok());
        assert!(hkdf(HashAlg::Sha256, b"ikm", b"salt", b"", 255 * 32 + 1).is_err());
    }
}
