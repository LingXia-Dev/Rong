# rong_crypto

The [Web Cryptography API](https://w3c.github.io/webcrypto/) for RongJS:
`globalThis.crypto`, `crypto.subtle` and `CryptoKey`.

```rust
let ctx = /* JSContext */;
rong_crypto::init(&ctx)?;
```

```javascript
const key = await crypto.subtle.generateKey(
  { name: "HMAC", hash: "SHA-256" },
  true,
  ["sign", "verify"],
);
const signature = await crypto.subtle.sign("HMAC", key, new TextEncoder().encode("hi"));
```

Supported: SHA-1/256/384/512 digests, HMAC sign/verify, AES-GCM and AES-CBC
encrypt/decrypt, PBKDF2 and HKDF derivation, `raw` and `jwk` (`kty: "oct"`)
key import/export, `getRandomValues` and `randomUUID`.

Asymmetric algorithms (RSA, ECDSA, ECDH, Ed25519, X25519) and AES-CTR/AES-KW
are recognized and rejected with `NotSupportedError`.

Backed by the RustCrypto crates (`sha1`, `sha2`, `hmac`, `aes`, `aes-gcm`,
`cbc`, `pbkdf2`, `hkdf`) and `getrandom`.
