/**
 * Crypto module type definitions
 * Corresponds to: modules/rong_crypto
 *
 * `globalThis.crypto` is a Web Crypto subset: SHA-1/256/384/512 digest, HMAC,
 * AES-GCM (96-bit IV, 128-bit tag), AES-CBC, PBKDF2, HKDF, and raw/oct JWK
 * import/export. Asymmetric algorithms and AES-CTR/AES-KW are recognized and
 * rejected with `NotSupportedError`.
 *
 * Ambient `Crypto` / `SubtleCrypto` / `CryptoKey` declarations live in the
 * `logic-web` typegen profile. DOM `lib` already covers the same globals for
 * `@rongjs/rong` consumers.
 */

export {};
