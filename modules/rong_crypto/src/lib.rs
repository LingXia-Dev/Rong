//! # Web Cryptography module
//!
//! Installs `globalThis.crypto` ([`Crypto`]) together with the
//! [`SubtleCrypto`] and [`CryptoKey`] interfaces, following the
//! [Web Cryptography API](https://w3c.github.io/webcrypto/) and the
//! `globalThis.crypto` semantics shared by browsers, Deno and workerd.
//!
//! ## Implemented
//!
//! - `crypto.getRandomValues(typedArray)` and `crypto.randomUUID()`
//! - `crypto.subtle.digest` - SHA-1, SHA-256, SHA-384, SHA-512
//! - `crypto.subtle.importKey` / `exportKey` - `raw` and `jwk` (`kty: "oct"`)
//! - `crypto.subtle.generateKey` - HMAC, AES-GCM, AES-CBC
//! - `crypto.subtle.sign` / `verify` - HMAC with any of the four hashes
//! - `crypto.subtle.encrypt` / `decrypt` - AES-GCM (96-bit IV, 128-bit tag)
//!   and AES-CBC (PKCS#7)
//! - `crypto.subtle.deriveBits` / `deriveKey` - PBKDF2 and HKDF
//!
//! ## Not implemented
//!
//! Asymmetric algorithms (RSASSA-PKCS1-v1_5, RSA-PSS, RSA-OAEP, ECDSA, ECDH,
//! Ed25519, X25519) and AES-CTR/AES-KW are recognized by name and rejected
//! with a `NotSupportedError` that names the algorithm. Key wrapping
//! (`wrapKey`/`unwrapKey`) is likewise absent.

mod algorithm;
mod buffer;
mod crypto;
mod error;
mod hash;
mod jwk;
mod key;
mod ops;
mod subtle;

use rong::{JSResult, PropertyDescriptor};

pub use crypto::Crypto;
pub use key::CryptoKey;
pub use subtle::SubtleCrypto;

/// Register `Crypto`, `SubtleCrypto` and `CryptoKey`, then install the
/// `globalThis.crypto` instance with its `subtle` member.
pub fn init(ctx: &rong::JSContext) -> JSResult<()> {
    ctx.register_class::<CryptoKey>()?;
    ctx.register_class::<SubtleCrypto>()?;
    ctx.register_class::<Crypto>()?;

    let crypto = Crypto::instance(ctx)?;
    // A single `SubtleCrypto` object per `Crypto`, so `crypto.subtle` keeps
    // its identity across accesses the way the platform does.
    crypto.define_property(
        "subtle",
        PropertyDescriptor::from_rust(ctx, subtle::SubtleCrypto::instance(ctx)?)
            .enumerable()
            .readonly(),
    )?;
    ctx.global().set("crypto", crypto)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rong_test::*;

    /// The JavaScript-level conformance suite. It lives next to the crate
    /// rather than in `tests/unit`, so it is loaded as an inline source.
    const CRYPTO_SUITE: &str = include_str!("../tests/crypto.js");

    #[test]
    fn crypto_conformance_suite() {
        async_run!(|ctx: JSContext| async move {
            rong_console::init(&ctx)?;
            rong_encoding::init(&ctx)?;
            init(&ctx)?;

            let passed = UnitJSRunner::load_source(&ctx, CRYPTO_SUITE)
                .await?
                .run()
                .await?;
            assert!(passed);

            Ok(())
        });
    }

    #[test]
    fn init_installs_the_crypto_global_once() {
        run(|ctx| {
            init(ctx)?;
            // A second init must be a no-op rather than a duplicate-class error.
            init(ctx)?;

            let shape: Vec<String> = ctx.eval(Source::from_bytes(
                r#"[
                    String(crypto instanceof Crypto),
                    String(crypto.subtle instanceof SubtleCrypto),
                    String(crypto.subtle === crypto.subtle),
                    typeof crypto.getRandomValues,
                    typeof crypto.randomUUID,
                    typeof crypto.subtle.digest,
                    typeof Crypto,
                    typeof SubtleCrypto,
                    typeof CryptoKey,
                ]"#,
            ))?;
            assert_eq!(
                shape,
                vec![
                    "true", "true", "true", "function", "function", "function", "function",
                    "function", "function",
                ]
            );
            Ok(())
        });
    }

    #[test]
    fn get_random_values_fills_only_the_view() {
        run(|ctx| {
            init(ctx)?;
            let result: Vec<i32> = ctx.eval(Source::from_bytes(
                r#"
                const backing = new Uint8Array(6);
                const view = new Uint8Array(backing.buffer, 2, 2);
                const returned = crypto.getRandomValues(view);
                [
                  returned === view ? 1 : 0,
                  backing[0], backing[1], backing[4], backing[5],
                ]
                "#,
            ))?;
            assert_eq!(result, vec![1, 0, 0, 0, 0]);
            Ok(())
        });
    }

    #[test]
    fn get_random_values_reports_spec_error_names() {
        run(|ctx| {
            init(ctx)?;
            let names: Vec<String> = ctx.eval(Source::from_bytes(
                r#"
                function nameOf(fn) {
                  try { fn(); return "no error"; } catch (error) { return error.name; }
                }
                [
                  nameOf(() => crypto.getRandomValues(new Uint8Array(65537))),
                  nameOf(() => crypto.getRandomValues(new Float32Array(4))),
                  nameOf(() => crypto.getRandomValues(new Float64Array(4))),
                  nameOf(() => crypto.getRandomValues({})),
                  nameOf(() => crypto.getRandomValues(new Uint8Array(65536))),
                ]
                "#,
            ))?;
            assert_eq!(
                names,
                vec![
                    "QuotaExceededError",
                    "TypeMismatchError",
                    "TypeMismatchError",
                    "TypeMismatchError",
                    "no error",
                ]
            );
            Ok(())
        });
    }
}
