// JavaScript-level conformance for the `crypto` global.
//
// Vectors are cited inline; every fixed expectation comes from a published
// RFC or NIST document.

const encoder = new TextEncoder();

function hex(buffer) {
  const bytes = new Uint8Array(buffer);
  let out = "";
  for (const byte of bytes) {
    out += byte.toString(16).padStart(2, "0");
  }
  return out;
}

function bytes(hexString) {
  const out = new Uint8Array(hexString.length / 2);
  for (let i = 0; i < out.length; i += 1) {
    out[i] = parseInt(hexString.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

async function rejects(promise) {
  try {
    await promise;
  } catch (error) {
    return error;
  }
  throw new Error("expected the promise to reject, but it resolved");
}

describe("crypto.getRandomValues", () => {
  test("fills the array in place and returns the same object", () => {
    const array = new Uint8Array(32);
    const returned = crypto.getRandomValues(array);
    expect(returned).toBe(array);
    expect(array.some((byte) => byte !== 0)).toBe(true);
  });

  test("writes only inside a subarray view", () => {
    const backing = new Uint8Array(8);
    const view = new Uint8Array(backing.buffer, 2, 4);
    crypto.getRandomValues(view);
    expect(backing[0]).toBe(0);
    expect(backing[1]).toBe(0);
    expect(backing[6]).toBe(0);
    expect(backing[7]).toBe(0);
  });

  test("accepts every integer typed array", () => {
    for (const Ctor of [Int8Array, Uint8Array, Uint8ClampedArray, Int16Array, Uint16Array, Int32Array, Uint32Array]) {
      const array = new Ctor(4);
      expect(crypto.getRandomValues(array)).toBe(array);
    }
  });

  test("rejects more than 65536 bytes with QuotaExceededError", () => {
    let thrown = null;
    try {
      crypto.getRandomValues(new Uint8Array(65537));
    } catch (error) {
      thrown = error;
    }
    expect(thrown).toBeTruthy();
    expect(thrown.name).toBe("QuotaExceededError");
  });

  test("allows exactly 65536 bytes", () => {
    const array = new Uint8Array(65536);
    expect(crypto.getRandomValues(array)).toBe(array);
  });

  test("counts bytes, not elements, against the quota", () => {
    let thrown = null;
    try {
      // 32769 * 2 bytes is over the limit even though the element count is not.
      crypto.getRandomValues(new Uint16Array(32769));
    } catch (error) {
      thrown = error;
    }
    expect(thrown.name).toBe("QuotaExceededError");
  });

  test("rejects float typed arrays with TypeMismatchError", () => {
    for (const Ctor of [Float32Array, Float64Array]) {
      let thrown = null;
      try {
        crypto.getRandomValues(new Ctor(4));
      } catch (error) {
        thrown = error;
      }
      expect(thrown.name).toBe("TypeMismatchError");
    }
  });
});

describe("crypto.randomUUID", () => {
  test("is a lowercase hyphenated RFC 4122 v4 UUID", () => {
    const uuid = crypto.randomUUID();
    expect(uuid.length).toBe(36);
    expect(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(uuid)).toBe(true);
  });

  test("sets the version and variant nibbles", () => {
    for (let i = 0; i < 32; i += 1) {
      const uuid = crypto.randomUUID();
      expect(uuid[14]).toBe("4");
      expect("89ab".includes(uuid[19])).toBe(true);
    }
  });

  test("does not repeat", () => {
    const seen = new Set();
    for (let i = 0; i < 128; i += 1) {
      seen.add(crypto.randomUUID());
    }
    expect(seen.size).toBe(128);
  });
});

describe("crypto.subtle.digest", () => {
  // FIPS 180-4 / NIST example vectors for the one-block message "abc".
  test("SHA-1 of 'abc'", async () => {
    const digest = await crypto.subtle.digest("SHA-1", encoder.encode("abc"));
    expect(hex(digest)).toBe("a9993e364706816aba3e25717850c26c9cd0d89d");
  });

  test("SHA-256 of 'abc'", async () => {
    const digest = await crypto.subtle.digest("SHA-256", encoder.encode("abc"));
    expect(hex(digest)).toBe(
      "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    );
  });

  test("SHA-384 of 'abc'", async () => {
    const digest = await crypto.subtle.digest({ name: "SHA-384" }, encoder.encode("abc"));
    expect(hex(digest)).toBe(
      "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed" +
        "8086072ba1e7cc2358baeca134c825a7",
    );
  });

  test("SHA-512 of 'abc'", async () => {
    const digest = await crypto.subtle.digest("SHA-512", encoder.encode("abc"));
    expect(hex(digest)).toBe(
      "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a" +
        "2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f",
    );
  });

  test("SHA-256 of the empty message", async () => {
    const digest = await crypto.subtle.digest("SHA-256", new Uint8Array(0));
    expect(hex(digest)).toBe(
      "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    );
  });

  test("accepts an ArrayBuffer as well as a view", async () => {
    const view = encoder.encode("abc");
    const digest = await crypto.subtle.digest("SHA-256", view.buffer);
    expect(hex(digest)).toBe(
      "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    );
  });

  test("accepts a DataView, including a sliced window", async () => {
    const expected =
      "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    const whole = encoder.encode("abc");
    expect(hex(await crypto.subtle.digest("SHA-256", new DataView(whole.buffer)))).toBe(expected);

    const backing = new Uint8Array([0, 0, 97, 98, 99, 0, 0]);
    const window = new DataView(backing.buffer, 2, 3);
    expect(hex(await crypto.subtle.digest("SHA-256", window))).toBe(expected);
  });

  test("resolves with an ArrayBuffer", async () => {
    const digest = await crypto.subtle.digest("SHA-256", new Uint8Array(1));
    expect(digest instanceof ArrayBuffer).toBe(true);
    expect(digest.byteLength).toBe(32);
  });

  test("matches the algorithm name case-insensitively", async () => {
    const digest = await crypto.subtle.digest("sha-256", encoder.encode("abc"));
    expect(hex(digest)).toBe(
      "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    );
  });

  test("rejects an unknown algorithm with NotSupportedError", async () => {
    const error = await rejects(crypto.subtle.digest("SHA-3", new Uint8Array(1)));
    expect(error.name).toBe("NotSupportedError");
  });
});

describe("crypto.subtle HMAC", () => {
  // RFC 4231 test case 2: key "Jefe", data "what do ya want for nothing?".
  const rfc4231Key = encoder.encode("Jefe");
  const rfc4231Data = encoder.encode("what do ya want for nothing?");

  async function importHmac(usages, hash = "SHA-256") {
    return crypto.subtle.importKey("raw", rfc4231Key, { name: "HMAC", hash }, true, usages);
  }

  test("HMAC-SHA-256 matches RFC 4231 test case 2", async () => {
    const key = await importHmac(["sign"]);
    const signature = await crypto.subtle.sign("HMAC", key, rfc4231Data);
    expect(hex(signature)).toBe(
      "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843",
    );
  });

  test("HMAC-SHA-512 matches RFC 4231 test case 2", async () => {
    const key = await importHmac(["sign"], "SHA-512");
    const signature = await crypto.subtle.sign("HMAC", key, rfc4231Data);
    expect(hex(signature)).toBe(
      "164b7a7bfcf819e2e395fbe73b56e0a387bd64222e831fd610270cd7ea250554" +
        "9758bf75c05a994a6d034f65f8f0e6fdcaeab1a34d4a6b4b636e070a38bce737",
    );
  });

  test("sign/verify round-trips", async () => {
    const key = await importHmac(["sign", "verify"]);
    const signature = await crypto.subtle.sign("HMAC", key, rfc4231Data);
    expect(await crypto.subtle.verify("HMAC", key, signature, rfc4231Data)).toBe(true);
  });

  test("a tampered signature does not verify", async () => {
    const key = await importHmac(["sign", "verify"]);
    const signature = new Uint8Array(await crypto.subtle.sign("HMAC", key, rfc4231Data));
    signature[0] ^= 0x01;
    expect(await crypto.subtle.verify("HMAC", key, signature, rfc4231Data)).toBe(false);
  });

  test("a truncated signature does not verify", async () => {
    const key = await importHmac(["sign", "verify"]);
    const signature = new Uint8Array(await crypto.subtle.sign("HMAC", key, rfc4231Data));
    expect(await crypto.subtle.verify("HMAC", key, signature.slice(0, 16), rfc4231Data)).toBe(false);
  });

  test("tampered data does not verify", async () => {
    const key = await importHmac(["sign", "verify"]);
    const signature = await crypto.subtle.sign("HMAC", key, rfc4231Data);
    expect(await crypto.subtle.verify("HMAC", key, signature, encoder.encode("other"))).toBe(false);
  });

  test("generateKey produces a usable key of the hash block length", async () => {
    const key = await crypto.subtle.generateKey({ name: "HMAC", hash: "SHA-256" }, true, [
      "sign",
      "verify",
    ]);
    expect(key.type).toBe("secret");
    expect(key.algorithm.name).toBe("HMAC");
    expect(key.algorithm.hash.name).toBe("SHA-256");
    expect(key.algorithm.length).toBe(512);
    const raw = await crypto.subtle.exportKey("raw", key);
    expect(raw.byteLength).toBe(64);
  });

  test("importKey honors HmacImportParams.length", async () => {
    const raw = new Uint8Array(32);
    const key = await crypto.subtle.importKey(
      "raw",
      raw,
      { name: "HMAC", hash: "SHA-256", length: 256 },
      true,
      ["sign"],
    );
    expect(key.algorithm.length).toBe(256);
    expect((await crypto.subtle.exportKey("raw", key)).byteLength).toBe(32);
  });

  test("importKey rejects HMAC length 0 and a truncated length with DataError", async () => {
    const raw = new Uint8Array(32);
    const zero = await rejects(
      crypto.subtle.importKey("raw", raw, { name: "HMAC", hash: "SHA-256", length: 0 }, true, [
        "sign",
      ]),
    );
    expect(zero.name).toBe("DataError");
    const truncated = await rejects(
      crypto.subtle.importKey("raw", raw, { name: "HMAC", hash: "SHA-256", length: 128 }, true, [
        "sign",
      ]),
    );
    expect(truncated.name).toBe("DataError");
  });
});

describe("crypto.subtle AES-GCM", () => {
  async function importAes(rawKey, usages = ["encrypt", "decrypt"]) {
    return crypto.subtle.importKey("raw", rawKey, { name: "AES-GCM" }, true, usages);
  }

  test("matches NIST GCM test case 13 (256-bit key, empty plaintext)", async () => {
    // NIST "gcmEncryptExtIV256" sample: all-zero key and IV, empty plaintext.
    const key = await importAes(new Uint8Array(32), ["encrypt"]);
    const ciphertext = await crypto.subtle.encrypt(
      { name: "AES-GCM", iv: new Uint8Array(12) },
      key,
      new Uint8Array(0),
    );
    expect(hex(ciphertext)).toBe("530f8afbc74536b9a963b4f1c4cb738b");
  });

  test("matches NIST GCM test case 14 (256-bit key, one zero block)", async () => {
    const key = await importAes(new Uint8Array(32), ["encrypt"]);
    const ciphertext = await crypto.subtle.encrypt(
      { name: "AES-GCM", iv: new Uint8Array(12) },
      key,
      new Uint8Array(16),
    );
    expect(hex(ciphertext)).toBe(
      "cea7403d4d606b6e074ec5d3baf39d18d0d1c8a799996bf0265b98b5d48ab919",
    );
  });

  test("round-trips a message", async () => {
    const key = await crypto.subtle.generateKey({ name: "AES-GCM", length: 256 }, true, [
      "encrypt",
      "decrypt",
    ]);
    const iv = crypto.getRandomValues(new Uint8Array(12));
    const message = encoder.encode("the quick brown fox");
    const ciphertext = await crypto.subtle.encrypt({ name: "AES-GCM", iv }, key, message);
    // 128-bit tag is appended to the ciphertext.
    expect(ciphertext.byteLength).toBe(message.length + 16);
    const plaintext = await crypto.subtle.decrypt({ name: "AES-GCM", iv }, key, ciphertext);
    expect(new Uint8Array(plaintext)).toEqual(message);
  });

  test("round-trips with additional authenticated data", async () => {
    const key = await crypto.subtle.generateKey({ name: "AES-GCM", length: 128 }, true, [
      "encrypt",
      "decrypt",
    ]);
    const iv = crypto.getRandomValues(new Uint8Array(12));
    const additionalData = encoder.encode("header");
    const message = encoder.encode("body");
    const ciphertext = await crypto.subtle.encrypt(
      { name: "AES-GCM", iv, additionalData },
      key,
      message,
    );
    const plaintext = await crypto.subtle.decrypt(
      { name: "AES-GCM", iv, additionalData },
      key,
      ciphertext,
    );
    expect(new Uint8Array(plaintext)).toEqual(message);

    const wrongAad = await rejects(
      crypto.subtle.decrypt(
        { name: "AES-GCM", iv, additionalData: encoder.encode("other") },
        key,
        ciphertext,
      ),
    );
    expect(wrongAad.name).toBe("OperationError");
  });

  test("the wrong key fails to decrypt", async () => {
    const key = await importAes(bytes("00112233445566778899aabbccddeeff"));
    const otherKey = await importAes(bytes("ffeeddccbbaa99887766554433221100"));
    const iv = new Uint8Array(12).fill(7);
    const ciphertext = await crypto.subtle.encrypt(
      { name: "AES-GCM", iv },
      key,
      encoder.encode("secret"),
    );
    const error = await rejects(
      crypto.subtle.decrypt({ name: "AES-GCM", iv }, otherKey, ciphertext),
    );
    expect(error.name).toBe("OperationError");
  });

  test("the wrong iv fails to decrypt", async () => {
    const key = await importAes(bytes("00112233445566778899aabbccddeeff"));
    const iv = new Uint8Array(12).fill(7);
    const ciphertext = await crypto.subtle.encrypt(
      { name: "AES-GCM", iv },
      key,
      encoder.encode("secret"),
    );
    const error = await rejects(
      crypto.subtle.decrypt({ name: "AES-GCM", iv: new Uint8Array(12).fill(8) }, key, ciphertext),
    );
    expect(error.name).toBe("OperationError");
  });

  test("an iv that is not 12 bytes is rejected", async () => {
    const key = await importAes(bytes("00112233445566778899aabbccddeeff"));
    const error = await rejects(
      crypto.subtle.encrypt({ name: "AES-GCM", iv: new Uint8Array(16) }, key, new Uint8Array(1)),
    );
    expect(error.name).toBe("NotSupportedError");
  });

  test("a truncated tagLength is not supported", async () => {
    const key = await importAes(bytes("00112233445566778899aabbccddeeff"));
    const error = await rejects(
      crypto.subtle.encrypt(
        { name: "AES-GCM", iv: new Uint8Array(12), tagLength: 96 },
        key,
        new Uint8Array(1),
      ),
    );
    expect(error.name).toBe("NotSupportedError");
  });
});

describe("crypto.subtle AES-CBC", () => {
  test("matches NIST SP 800-38A F.2.1 (AES-128-CBC, first block)", async () => {
    const key = await crypto.subtle.importKey(
      "raw",
      bytes("2b7e151628aed2a6abf7158809cf4f3c"),
      { name: "AES-CBC" },
      false,
      ["encrypt"],
    );
    const ciphertext = await crypto.subtle.encrypt(
      { name: "AES-CBC", iv: bytes("000102030405060708090a0b0c0d0e0f") },
      key,
      bytes("6bc1bee22e409f96e93d7e117393172a"),
    );
    // The first block is the SP 800-38A vector; the second is the PKCS#7
    // padding block the Web Cryptography API mandates.
    expect(hex(ciphertext).slice(0, 32)).toBe("7649abac8119b246cee98e9b12e9197d");
    expect(ciphertext.byteLength).toBe(32);
  });

  test("round-trips a message", async () => {
    const key = await crypto.subtle.generateKey({ name: "AES-CBC", length: 256 }, true, [
      "encrypt",
      "decrypt",
    ]);
    const iv = crypto.getRandomValues(new Uint8Array(16));
    const message = encoder.encode("cipher block chaining");
    const ciphertext = await crypto.subtle.encrypt({ name: "AES-CBC", iv }, key, message);
    const plaintext = await crypto.subtle.decrypt({ name: "AES-CBC", iv }, key, ciphertext);
    expect(new Uint8Array(plaintext)).toEqual(message);
  });

  test("the wrong iv corrupts the first block", async () => {
    const key = await crypto.subtle.generateKey({ name: "AES-CBC", length: 128 }, true, [
      "encrypt",
      "decrypt",
    ]);
    const iv = new Uint8Array(16).fill(1);
    const message = encoder.encode("0123456789abcdef0123456789abcdef");
    const ciphertext = await crypto.subtle.encrypt({ name: "AES-CBC", iv }, key, message);
    const plaintext = new Uint8Array(
      await crypto.subtle.decrypt({ name: "AES-CBC", iv: new Uint8Array(16).fill(2) }, key, ciphertext),
    );
    expect(plaintext.slice(0, 16)).not.toEqual(message.slice(0, 16));
    expect(plaintext.slice(16)).toEqual(message.slice(16));
  });
});

describe("crypto.subtle key usage enforcement", () => {
  test("a sign-only key cannot verify", async () => {
    const key = await crypto.subtle.importKey(
      "raw",
      encoder.encode("key"),
      { name: "HMAC", hash: "SHA-256" },
      false,
      ["sign"],
    );
    const signature = await crypto.subtle.sign("HMAC", key, encoder.encode("m"));
    const error = await rejects(
      crypto.subtle.verify("HMAC", key, signature, encoder.encode("m")),
    );
    expect(error.name).toBe("InvalidAccessError");
  });

  test("a verify-only key cannot sign", async () => {
    const key = await crypto.subtle.importKey(
      "raw",
      encoder.encode("key"),
      { name: "HMAC", hash: "SHA-256" },
      false,
      ["verify"],
    );
    const error = await rejects(crypto.subtle.sign("HMAC", key, encoder.encode("m")));
    expect(error.name).toBe("InvalidAccessError");
  });

  test("an encrypt-only key cannot decrypt", async () => {
    const key = await crypto.subtle.generateKey({ name: "AES-GCM", length: 128 }, true, [
      "encrypt",
    ]);
    const iv = new Uint8Array(12);
    const ciphertext = await crypto.subtle.encrypt({ name: "AES-GCM", iv }, key, new Uint8Array(4));
    const error = await rejects(crypto.subtle.decrypt({ name: "AES-GCM", iv }, key, ciphertext));
    expect(error.name).toBe("InvalidAccessError");
  });

  test("an AES-CBC key cannot be used for AES-GCM", async () => {
    const key = await crypto.subtle.generateKey({ name: "AES-CBC", length: 128 }, true, [
      "encrypt",
      "decrypt",
    ]);
    const error = await rejects(
      crypto.subtle.encrypt({ name: "AES-GCM", iv: new Uint8Array(12) }, key, new Uint8Array(4)),
    );
    expect(error.name).toBe("InvalidAccessError");
  });

  test("an HMAC key cannot be imported for encryption", async () => {
    const error = await rejects(
      crypto.subtle.importKey(
        "raw",
        encoder.encode("key"),
        { name: "HMAC", hash: "SHA-256" },
        false,
        ["encrypt"],
      ),
    );
    expect(error.name).toBe("SyntaxError");
  });

  test("an empty key usage list is rejected", async () => {
    const error = await rejects(
      crypto.subtle.importKey(
        "raw",
        encoder.encode("key"),
        { name: "HMAC", hash: "SHA-256" },
        false,
        [],
      ),
    );
    expect(error.name).toBe("SyntaxError");
  });

  test("a non-extractable key cannot be exported", async () => {
    const key = await crypto.subtle.generateKey({ name: "AES-GCM", length: 128 }, false, [
      "encrypt",
      "decrypt",
    ]);
    expect(key.extractable).toBe(false);
    const error = await rejects(crypto.subtle.exportKey("raw", key));
    expect(error.name).toBe("InvalidAccessError");
  });
});

describe("crypto.subtle unsupported algorithms", () => {
  const asymmetric = ["RSASSA-PKCS1-v1_5", "RSA-PSS", "RSA-OAEP", "ECDSA", "ECDH", "Ed25519", "X25519"];

  test("importKey rejects asymmetric algorithms with NotSupportedError", async () => {
    for (const name of asymmetric) {
      const error = await rejects(
        crypto.subtle.importKey("raw", new Uint8Array(32), { name }, false, ["sign"]),
      );
      expect(error.name).toBe("NotSupportedError");
      expect(error.message).toContain(name);
    }
  });

  test("sign rejects asymmetric algorithms with NotSupportedError", async () => {
    const key = await crypto.subtle.importKey(
      "raw",
      encoder.encode("key"),
      { name: "HMAC", hash: "SHA-256" },
      false,
      ["sign"],
    );
    for (const name of ["ECDSA", "Ed25519", "RSASSA-PKCS1-v1_5"]) {
      const error = await rejects(crypto.subtle.sign({ name, hash: "SHA-256" }, key, new Uint8Array(1)));
      expect(error.name).toBe("NotSupportedError");
      expect(error.message).toContain(name);
    }
  });

  test("AES-CTR and AES-KW are recognized but not supported", async () => {
    for (const name of ["AES-CTR", "AES-KW"]) {
      const error = await rejects(
        crypto.subtle.importKey("raw", new Uint8Array(16), { name }, false, ["encrypt"]),
      );
      expect(error.name).toBe("NotSupportedError");
      expect(error.message).toContain(name);
    }
  });

  test("a completely unknown name is a NotSupportedError", async () => {
    const error = await rejects(
      crypto.subtle.importKey("raw", new Uint8Array(16), { name: "ROT13" }, false, ["encrypt"]),
    );
    expect(error.name).toBe("NotSupportedError");
    expect(error.message).toContain("ROT13");
  });

  test("an unsupported key format is a NotSupportedError", async () => {
    const error = await rejects(
      crypto.subtle.importKey("pkcs8", new Uint8Array(16), { name: "AES-GCM" }, false, ["encrypt"]),
    );
    expect(error.name).toBe("NotSupportedError");
  });

  test("encrypt of HMAC or SHA-256 is NotSupportedError, not InvalidAccessError", async () => {
    const hmac = await crypto.subtle.importKey(
      "raw",
      encoder.encode("key"),
      { name: "HMAC", hash: "SHA-256" },
      false,
      ["sign"],
    );
    const aes = await crypto.subtle.generateKey({ name: "AES-GCM", length: 128 }, false, [
      "encrypt",
    ]);
    const hmacError = await rejects(crypto.subtle.encrypt("HMAC", hmac, new Uint8Array(1)));
    expect(hmacError.name).toBe("NotSupportedError");
    const hashError = await rejects(crypto.subtle.encrypt("SHA-256", aes, new Uint8Array(1)));
    expect(hashError.name).toBe("NotSupportedError");
  });
});

describe("crypto.subtle key import and export", () => {
  test("raw import/export round-trips the material", async () => {
    const raw = bytes("000102030405060708090a0b0c0d0e0f");
    const key = await crypto.subtle.importKey("raw", raw, { name: "AES-GCM" }, true, ["encrypt"]);
    expect(key.algorithm.length).toBe(128);
    expect(key.usages).toEqual(["encrypt"]);
    const exported = await crypto.subtle.exportKey("raw", key);
    expect(new Uint8Array(exported)).toEqual(raw);
  });

  test("an AES key of the wrong length is a DataError", async () => {
    const error = await rejects(
      crypto.subtle.importKey("raw", new Uint8Array(20), { name: "AES-GCM" }, true, ["encrypt"]),
    );
    expect(error.name).toBe("DataError");
  });

  test("jwk export produces an oct key that can be re-imported", async () => {
    const key = await crypto.subtle.generateKey({ name: "HMAC", hash: "SHA-256" }, true, [
      "sign",
      "verify",
    ]);
    const jwk = await crypto.subtle.exportKey("jwk", key);
    expect(jwk.kty).toBe("oct");
    expect(jwk.alg).toBe("HS256");
    expect(jwk.ext).toBe(true);
    expect(jwk.key_ops).toEqual(["sign", "verify"]);

    const reimported = await crypto.subtle.importKey(
      "jwk",
      jwk,
      { name: "HMAC", hash: "SHA-256" },
      true,
      ["sign"],
    );
    const message = encoder.encode("same key, same tag");
    const a = await crypto.subtle.sign("HMAC", key, message);
    const b = await crypto.subtle.sign("HMAC", reimported, message);
    expect(hex(a)).toBe(hex(b));
  });

  test("jwk export of an AES key carries the RFC 7518 alg", async () => {
    const key = await crypto.subtle.generateKey({ name: "AES-GCM", length: 256 }, true, [
      "encrypt",
      "decrypt",
    ]);
    const jwk = await crypto.subtle.exportKey("jwk", key);
    expect(jwk.alg).toBe("A256GCM");
  });

  test("a jwk whose alg contradicts the algorithm is a DataError", async () => {
    const error = await rejects(
      crypto.subtle.importKey(
        "jwk",
        { kty: "oct", k: "AAAAAAAAAAAAAAAAAAAAAA", alg: "HS512" },
        { name: "HMAC", hash: "SHA-256" },
        true,
        ["sign"],
      ),
    );
    expect(error.name).toBe("DataError");
  });

  test("a jwk with the wrong kty is a DataError", async () => {
    const error = await rejects(
      crypto.subtle.importKey("jwk", { kty: "RSA" }, { name: "HMAC", hash: "SHA-256" }, true, [
        "sign",
      ]),
    );
    expect(error.name).toBe("DataError");
  });

  test("a jwk whose use contradicts the algorithm is a DataError", async () => {
    const hmac = await rejects(
      crypto.subtle.importKey(
        "jwk",
        { kty: "oct", k: "AAAAAAAAAAAAAAAAAAAAAA", use: "enc" },
        { name: "HMAC", hash: "SHA-256" },
        true,
        ["sign"],
      ),
    );
    expect(hmac.name).toBe("DataError");
    const aes = await rejects(
      crypto.subtle.importKey(
        "jwk",
        { kty: "oct", k: "AAAAAAAAAAAAAAAAAAAAAA", use: "sig" },
        { name: "AES-GCM" },
        true,
        ["encrypt"],
      ),
    );
    expect(aes.name).toBe("DataError");
  });

  test("a jwk whose alg or ext is the wrong JS type is a DataError", async () => {
    const alg = await rejects(
      crypto.subtle.importKey(
        "jwk",
        { kty: "oct", k: "AAAAAAAAAAAAAAAAAAAAAA", alg: 256 },
        { name: "HMAC", hash: "SHA-256" },
        true,
        ["sign"],
      ),
    );
    expect(alg.name).toBe("DataError");
    const ext = await rejects(
      crypto.subtle.importKey(
        "jwk",
        { kty: "oct", k: "AAAAAAAAAAAAAAAAAAAAAA", ext: "false" },
        { name: "HMAC", hash: "SHA-256" },
        true,
        ["sign"],
      ),
    );
    expect(ext.name).toBe("DataError");
  });

  test("a matching jwk use is accepted", async () => {
    const key = await crypto.subtle.importKey(
      "jwk",
      { kty: "oct", k: "AAAAAAAAAAAAAAAAAAAAAA", use: "sig" },
      { name: "HMAC", hash: "SHA-256" },
      true,
      ["sign"],
    );
    expect(key.algorithm.name).toBe("HMAC");
  });
});

describe("crypto.subtle derivation", () => {
  test("PBKDF2-HMAC-SHA-1 matches RFC 6070 test case 2", async () => {
    // RFC 6070: P = "password", S = "salt", c = 2, dkLen = 20.
    const key = await crypto.subtle.importKey(
      "raw",
      encoder.encode("password"),
      "PBKDF2",
      false,
      ["deriveBits"],
    );
    const derived = await crypto.subtle.deriveBits(
      { name: "PBKDF2", salt: encoder.encode("salt"), iterations: 2, hash: "SHA-1" },
      key,
      160,
    );
    expect(hex(derived)).toBe("ea6c014dc72d6f8ccd1ed92ace1d41f0d8de8957");
  });

  test("PBKDF2-HMAC-SHA-256 matches the RFC 7914 section 11 vector", async () => {
    // RFC 7914 section 11: P = "passwd", S = "salt", c = 1, dkLen = 64.
    const key = await crypto.subtle.importKey(
      "raw",
      encoder.encode("passwd"),
      { name: "PBKDF2" },
      false,
      ["deriveBits"],
    );
    const derived = await crypto.subtle.deriveBits(
      { name: "PBKDF2", salt: encoder.encode("salt"), iterations: 1, hash: "SHA-256" },
      key,
      512,
    );
    expect(hex(derived)).toBe(
      "55ac046e56e3089fec1691c22544b605f94185216dde0465e68b9d57c20dacbc" +
        "49ca9cccf179b645991664b39d77ef317c71b845b1e30bd509112041d3a19783",
    );
  });

  test("HKDF-SHA-256 matches RFC 5869 test case 1", async () => {
    const key = await crypto.subtle.importKey(
      "raw",
      bytes("0b".repeat(22)),
      "HKDF",
      false,
      ["deriveBits"],
    );
    const derived = await crypto.subtle.deriveBits(
      {
        name: "HKDF",
        salt: bytes("000102030405060708090a0b0c"),
        info: bytes("f0f1f2f3f4f5f6f7f8f9"),
        hash: "SHA-256",
      },
      key,
      336,
    );
    expect(hex(derived)).toBe(
      "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf" +
        "34007208d5b887185865",
    );
  });

  test("deriveKey produces a working AES-GCM key", async () => {
    const base = await crypto.subtle.importKey(
      "raw",
      encoder.encode("correct horse battery staple"),
      "PBKDF2",
      false,
      ["deriveKey"],
    );
    const key = await crypto.subtle.deriveKey(
      { name: "PBKDF2", salt: encoder.encode("salt"), iterations: 100, hash: "SHA-256" },
      base,
      { name: "AES-GCM", length: 256 },
      true,
      ["encrypt", "decrypt"],
    );
    expect(key.algorithm.name).toBe("AES-GCM");
    expect(key.algorithm.length).toBe(256);

    const iv = crypto.getRandomValues(new Uint8Array(12));
    const message = encoder.encode("derived");
    const ciphertext = await crypto.subtle.encrypt({ name: "AES-GCM", iv }, key, message);
    const plaintext = await crypto.subtle.decrypt({ name: "AES-GCM", iv }, key, ciphertext);
    expect(new Uint8Array(plaintext)).toEqual(message);
  });

  test("deriveKey is deterministic for the same inputs", async () => {
    async function derive() {
      const base = await crypto.subtle.importKey("raw", encoder.encode("pw"), "HKDF", false, [
        "deriveKey",
      ]);
      const key = await crypto.subtle.deriveKey(
        { name: "HKDF", salt: encoder.encode("s"), info: encoder.encode("i"), hash: "SHA-256" },
        base,
        { name: "HMAC", hash: "SHA-256", length: 256 },
        true,
        ["sign"],
      );
      return crypto.subtle.exportKey("raw", key);
    }
    expect(hex(await derive())).toBe(hex(await derive()));
  });

  test("a PBKDF2 key must be imported non-extractable", async () => {
    const error = await rejects(
      crypto.subtle.importKey("raw", encoder.encode("pw"), "PBKDF2", true, ["deriveBits"]),
    );
    expect(error.name).toBe("SyntaxError");
  });

  test("a PBKDF2 key cannot be exported", async () => {
    const key = await crypto.subtle.importKey("raw", encoder.encode("pw"), "PBKDF2", false, [
      "deriveBits",
    ]);
    const error = await rejects(crypto.subtle.exportKey("raw", key));
    expect(error.name).toBe("InvalidAccessError");
  });

  test("a deriveBits-only key cannot deriveKey", async () => {
    const base = await crypto.subtle.importKey("raw", encoder.encode("pw"), "PBKDF2", false, [
      "deriveBits",
    ]);
    const error = await rejects(
      crypto.subtle.deriveKey(
        { name: "PBKDF2", salt: encoder.encode("s"), iterations: 1, hash: "SHA-256" },
        base,
        { name: "AES-GCM", length: 128 },
        true,
        ["encrypt"],
      ),
    );
    expect(error.name).toBe("InvalidAccessError");
  });

  test("a length that is not a multiple of 8 is an OperationError", async () => {
    const key = await crypto.subtle.importKey("raw", encoder.encode("pw"), "PBKDF2", false, [
      "deriveBits",
    ]);
    const error = await rejects(
      crypto.subtle.deriveBits(
        { name: "PBKDF2", salt: encoder.encode("s"), iterations: 1, hash: "SHA-256" },
        key,
        12,
      ),
    );
    expect(error.name).toBe("OperationError");
  });

  test("deriveBits with length 0 returns an empty ArrayBuffer", async () => {
    const key = await crypto.subtle.importKey("raw", encoder.encode("pw"), "PBKDF2", false, [
      "deriveBits",
    ]);
    const derived = await crypto.subtle.deriveBits(
      { name: "PBKDF2", salt: encoder.encode("s"), iterations: 1, hash: "SHA-256" },
      key,
      0,
    );
    expect(derived instanceof ArrayBuffer).toBe(true);
    expect(derived.byteLength).toBe(0);
  });

  test("deriveBits length 0 still rejects iterations 0", async () => {
    const key = await crypto.subtle.importKey("raw", encoder.encode("pw"), "PBKDF2", false, [
      "deriveBits",
    ]);
    const error = await rejects(
      crypto.subtle.deriveBits(
        { name: "PBKDF2", salt: encoder.encode("s"), iterations: 0, hash: "SHA-256" },
        key,
        0,
      ),
    );
    expect(error.name).toBe("OperationError");
  });

  test("HKDF requires info", async () => {
    const key = await crypto.subtle.importKey("raw", encoder.encode("ikm"), "HKDF", false, [
      "deriveBits",
    ]);
    const error = await rejects(
      crypto.subtle.deriveBits({ name: "HKDF", salt: encoder.encode("s"), hash: "SHA-256" }, key, 256),
    );
    expect(error.name).toBe("TypeError");
  });

  test("HKDF accepts an empty info", async () => {
    const key = await crypto.subtle.importKey("raw", bytes("0b".repeat(22)), "HKDF", false, [
      "deriveBits",
    ]);
    const derived = await crypto.subtle.deriveBits(
      { name: "HKDF", salt: new Uint8Array(0), info: new Uint8Array(0), hash: "SHA-256" },
      key,
      336,
    );
    expect(hex(derived)).toBe(
      "8da4e775a563c18f715f802a063c5a31b8a11f5c5ee1879ec3454e5f3c738d2d" +
        "9d201395faa4b61a96c8",
    );
  });
});

describe("crypto interface shape", () => {
  test("crypto.subtle keeps its identity", () => {
    expect(crypto.subtle).toBe(crypto.subtle);
  });

  test("crypto is a Crypto and crypto.subtle a SubtleCrypto", () => {
    expect(crypto instanceof Crypto).toBe(true);
    expect(crypto.subtle instanceof SubtleCrypto).toBe(true);
  });

  test("the interfaces are not constructible", () => {
    for (const Ctor of [Crypto, SubtleCrypto, CryptoKey]) {
      let thrown = null;
      try {
        new Ctor();
      } catch (error) {
        thrown = error;
      }
      expect(thrown).toBeTruthy();
    }
  });

  test("a generated key is a CryptoKey", async () => {
    const key = await crypto.subtle.generateKey({ name: "AES-GCM", length: 128 }, true, [
      "encrypt",
      "decrypt",
    ]);
    expect(key instanceof CryptoKey).toBe(true);
  });

  test("CryptoKey.algorithm and usages keep their identity", async () => {
    const key = await crypto.subtle.generateKey({ name: "HMAC", hash: "SHA-256" }, true, [
      "sign",
      "verify",
    ]);
    expect(key.algorithm).toBe(key.algorithm);
    expect(key.usages).toBe(key.usages);
    expect(Object.isFrozen(key.usages)).toBe(true);
  });
});
