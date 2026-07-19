// S3Client / S3File API tests
// Uses TEST_S3_* globals set by the Rust test harness (local s3s mock server).

const S3_ENDPOINT = globalThis.TEST_S3_ENDPOINT ?? "http://127.0.0.1:9000";
const S3_ACCESS_KEY = globalThis.TEST_S3_ACCESS_KEY;
const S3_SECRET_KEY = globalThis.TEST_S3_SECRET_KEY;
const S3_BUCKET = globalThis.TEST_S3_BUCKET ?? "test-bucket";

function makeClient(overrides) {
  return new Rong.S3Client({
    accessKeyId: S3_ACCESS_KEY,
    secretAccessKey: S3_SECRET_KEY,
    bucket: S3_BUCKET,
    endpoint: S3_ENDPOINT,
    ...overrides,
  });
}

// ─── Unit tests (construction, lazy refs, error handling) ──────────

describe("S3Client", () => {
  // ─── Construction ──────────────────────────────────────────────

  test("hides S3File from global scope", () => {
    assert.equal(typeof S3File, "undefined");
    assert.equal(globalThis.S3File, undefined);

    const client = makeClient();
    const file = client.file("hello.txt");
    let failed = false;
    try {
      new file.constructor();
    } catch (e) {
      failed = true;
    }
    assert(failed, "S3File should not be constructible via instance.constructor");
  });

  test("does not expose S3Client on globalThis", () => {
    assert.equal(typeof S3Client, "undefined");
    assert.equal(globalThis.S3Client, undefined);
    assert.equal(typeof Rong.S3Client, "function");
  });

  test("constructor with explicit options", () => {
    const client = makeClient();
    assert(client, "S3Client instance created");
  });

  test("constructor with no arguments", () => {
    const client = new Rong.S3Client();
    assert(client, "S3Client created without args");
  });

  test("constructor with custom region", () => {
    const client = makeClient({ region: "us-west-2" });
    assert(client, "S3Client with custom region");
  });

  // ─── file() — lazy reference ──────────────────────────────────

  test("file() returns lazy S3File with correct name", () => {
    const client = makeClient();
    const f = client.file("hello.txt");
    assert(f, "file() returned an object");
    assert.equal(f.name, "hello.txt");
  });

  test("file() with options override", () => {
    const client = makeClient();
    const f = client.file("data.bin", { bucket: "other-bucket" });
    assert(f, "file() with options returned an object");
    assert.equal(f.name, "data.bin");
  });

  test("S3File.size is always NaN", () => {
    const client = makeClient();
    const f = client.file("any.bin");
    assert(Number.isNaN(f.size), "size is NaN for remote files");
  });

  // ─── slice() — no network ─────────────────────────────────────

  test("slice() returns a new S3File with range", () => {
    const client = makeClient();
    const f = client.file("large.bin");
    const partial = f.slice(0, 1024);
    assert(partial, "slice returned an object");
    assert.equal(partial.name, "large.bin");
    assert(Number.isNaN(partial.size), "sliced file size is still NaN");
  });

  // ─── presign() — no data transfer ─────────────────────────────

  test("presign() GET returns a signed URL string", async () => {
    const client = makeClient();
    const url = await client.presign("test-key.txt");
    assert(typeof url === "string", "presign returns a string");
    assert(url.includes("test-key.txt"), "URL contains the key");
    assert(
      url.includes("X-Amz-Signature") || url.includes("Signature"),
      "URL is signed",
    );
  });

  test("presign() PUT with custom expiry", async () => {
    const client = makeClient();
    const url = await client.presign("upload.bin", {
      expiresIn: 3600,
      method: "PUT",
    });
    assert(typeof url === "string", "PUT presign returns a string");
  });

  test("presign() DELETE method", async () => {
    const client = makeClient();
    const url = await client.presign("to-delete.txt", { method: "DELETE" });
    assert(typeof url === "string", "DELETE presign returns a string");
  });

  test("S3File.presign() works on file reference", async () => {
    const client = makeClient();
    const f = client.file("report.pdf");
    const url = await f.presign({ expiresIn: 7200 });
    assert(url.includes("report.pdf"), "file presign URL contains key");
  });

  test("S3File.presign() PUT on file reference", async () => {
    const client = makeClient();
    const f = client.file("upload.bin");
    const url = await f.presign({ method: "PUT" });
    assert(typeof url === "string", "file PUT presign returns string");
  });

  // ─── Error handling ────────────────────────────────────────────

  test("error on missing credentials for network ops", async () => {
    const client = new Rong.S3Client({
      bucket: "some-bucket",
      endpoint: S3_ENDPOINT,
    });
    try {
      await client.exists("test.txt");
      assert(false, "should have thrown");
    } catch (e) {
      assert(
        e.message.includes("credentials") || e.message.includes("Credentials"),
        "error mentions credentials: " + e.message,
      );
    }
  });

  test("error on missing bucket", async () => {
    const client = new Rong.S3Client({
      accessKeyId: S3_ACCESS_KEY,
      secretAccessKey: S3_SECRET_KEY,
      endpoint: S3_ENDPOINT,
    });
    try {
      await client.exists("test.txt");
      assert(false, "should have thrown");
    } catch (e) {
      assert(
        e.message.includes("bucket") || e.message.includes("Bucket"),
        "error mentions bucket: " + e.message,
      );
    }
  });

  test("S3File constructor is not exposed globally", () => {
    assert.equal(typeof S3File, "undefined");
    assert.equal(globalThis.S3File, undefined);
  });

  test("presign() rejects invalid method", async () => {
    const client = makeClient();
    try {
      await client.presign("key", { method: "PATCH" });
      assert(false, "should have thrown");
    } catch (e) {
      assert(e.message.includes("PATCH"), "error mentions bad method");
    }
  });

  test("presign() rejects a negative expiration", async () => {
    const client = makeClient();
    let error;
    try {
      await client.presign("key", { expiresIn: -1 });
    } catch (caught) {
      error = caught;
    }
    assert(error instanceof RangeError, "negative expiresIn should throw RangeError");
  });

  test("S3File.presign() rejects a negative expiration", async () => {
    const file = makeClient().file("key");
    let error;
    try {
      await file.presign({ expiresIn: -1 });
    } catch (caught) {
      error = caught;
    }
    assert(error instanceof RangeError, "negative expiresIn should throw RangeError");
  });

  test("exists() propagates transport errors", async () => {
    const client = makeClient({ endpoint: "http://127.0.0.1:1" });
    let error;
    try {
      await client.exists("key");
    } catch (caught) {
      error = caught;
    }
    assert(error && error.name === "S3Error", "transport error should be preserved");
  });

  test("S3File.exists() propagates transport errors", async () => {
    const file = makeClient({ endpoint: "http://127.0.0.1:1" }).file("key");
    let error;
    try {
      await file.exists();
    } catch (caught) {
      error = caught;
    }
    assert(error && error.name === "S3Error", "transport error should be preserved");
  });

  test("S3 errors have name 'S3Error'", async () => {
    const client = makeClient();
    try {
      // read a non-existent key — should throw S3Error
      await client.file("does-not-exist-" + Date.now()).text();
      assert(false, "should have thrown");
    } catch (e) {
      assert.equal(e.name, "S3Error");
    }
  });

  test("write() with invalid data type throws TypeError", async () => {
    const client = makeClient();
    try {
      await client.write("key.txt", 12345);
      assert(false, "should have thrown");
    } catch (e) {
      assert.equal(e.name, "TypeError");
    }
  });
});

// ─── Live tests (against local s3s mock server) ──────────────────

describe("S3 operations", () => {
  const client = makeClient();
  const TEST_KEY = `rong-test-${Date.now()}.txt`;
  const TEST_JSON_KEY = `rong-test-${Date.now()}.json`;
  const TEST_BIN_KEY = `rong-test-${Date.now()}.bin`;
  const TEST_CONTENT = "Hello from RongJS S3 test!";
  const TEST_JSON = { name: "rong", version: "0.3.0", timestamp: Date.now() };

  // ─── write + read ──────────────────────────────────────────────

  test("client.write() returns bytes written", async () => {
    const n = await client.write(TEST_KEY, TEST_CONTENT);
    assert.equal(n, TEST_CONTENT.length);
  });

  test("file.text() reads back written content", async () => {
    const file = client.file(TEST_KEY);
    const text = await file.text();
    assert.equal(text, TEST_CONTENT);
  });

  test("write() with content type + json()", async () => {
    await client.write(TEST_JSON_KEY, JSON.stringify(TEST_JSON), {
      type: "application/json",
    });
    const file = client.file(TEST_JSON_KEY);
    const data = await file.json();
    assert.equal(data.name, "rong");
    assert.equal(data.version, "0.3.0");
  });

  test("write() ArrayBuffer + bytes() roundtrip", async () => {
    const src = new Uint8Array([0x00, 0x01, 0x02, 0xff, 0xfe]);
    await client.write(TEST_BIN_KEY, src.buffer);
    const file = client.file(TEST_BIN_KEY);
    const buf = await file.bytes();
    const out = new Uint8Array(buf);
    assert.equal(out.length, 5);
    assert.equal(out[0], 0x00);
    assert.equal(out[3], 0xff);
    assert.equal(out[4], 0xfe);
  });

  test("write() Uint8Array", async () => {
    const src = new Uint8Array([10, 20, 30]);
    const n = await client.write(TEST_BIN_KEY, src);
    assert.equal(n, 3);
  });

  // ─── exists ────────────────────────────────────────────────────

  test("client.exists() returns true for existing key", async () => {
    const exists = await client.exists(TEST_KEY);
    assert.equal(exists, true);
  });

  test("client.exists() returns false for missing key", async () => {
    const exists = await client.exists("nonexistent-key-" + Date.now());
    assert.equal(exists, false);
  });

  test("file.exists() returns true for existing key", async () => {
    const file = client.file(TEST_KEY);
    const exists = await file.exists();
    assert.equal(exists, true);
  });

  test("file.exists() returns false for missing key", async () => {
    const file = client.file("nonexistent-file-" + Date.now());
    const exists = await file.exists();
    assert.equal(exists, false);
  });

  // ─── size ──────────────────────────────────────────────────────

  test("client.size() returns byte count", async () => {
    const sz = await client.size(TEST_KEY);
    assert.equal(sz, TEST_CONTENT.length);
  });

  // ─── stat ──────────────────────────────────────────────────────

  test("client.stat() returns metadata object", async () => {
    const st = await client.stat(TEST_KEY);
    assert(typeof st.size === "number", "stat.size is a number");
    assert(st.size > 0, "stat.size > 0");
    assert(typeof st.type === "string", "stat.type is a string");
  });

  test("file.stat() returns metadata object", async () => {
    const file = client.file(TEST_KEY);
    const st = await file.stat();
    assert(typeof st.size === "number", "stat.size is a number");
    assert(st.size > 0, "stat.size > 0");
  });

  // ─── bytes / arrayBuffer ───────────────────────────────────────

  test("file.bytes() returns ArrayBuffer", async () => {
    const file = client.file(TEST_KEY);
    const buf = await file.bytes();
    assert(buf !== null && buf !== undefined, "bytes() returned data");
    assert(buf.byteLength > 0, "has byte length");
  });

  test("file.arrayBuffer() is alias for bytes()", async () => {
    const file = client.file(TEST_KEY);
    const buf = await file.arrayBuffer();
    assert(buf !== null && buf !== undefined, "arrayBuffer() returned data");
    assert.equal(buf.byteLength, TEST_CONTENT.length);
  });

  // ─── slice ─────────────────────────────────────────────────────

  test("slice() returns partial content", async () => {
    const file = client.file(TEST_KEY);
    const partial = file.slice(0, 5);
    const text = await partial.text();
    assert.equal(text, TEST_CONTENT.slice(0, 5));
  });

  test("slice() with only start", async () => {
    const file = client.file(TEST_KEY);
    const partial = file.slice(6);
    const text = await partial.text();
    assert.equal(text, TEST_CONTENT.slice(6));
  });

  test("slice() with end before start returns empty content", async () => {
    const file = client.file(TEST_KEY);
    const partial = file.slice(5, 2);
    assert.equal(await partial.text(), "");
  });

  // ─── list ──────────────────────────────────────────────────────

  test("list() returns objects", async () => {
    const result = await client.list({ prefix: "rong-test-" });
    assert(result.contents, "has contents array");
    assert(result.contents.length > 0, "found test objects");
    const found = result.contents.find((o) => o.key === TEST_KEY);
    assert(found, "found our test key in listing");
    assert(found.size > 0, "listed object has size");
    assert(typeof found.lastModified === "string", "has lastModified");
  });

  test("list() with maxKeys limits results + isTruncated", async () => {
    const result = await client.list({ prefix: "rong-test-", maxKeys: 1 });
    assert.equal(result.contents.length, 1);
    assert.equal(result.isTruncated, true);
  });

  test("list() rejects a negative maxKeys", async () => {
    let error;
    try {
      await client.list({ prefix: "rong-test-", maxKeys: -1 });
    } catch (caught) {
      error = caught;
    }
    assert(error instanceof RangeError, "negative maxKeys should throw RangeError");
  });

  test("list() with startAfter paginates", async () => {
    const all = await client.list({ prefix: "rong-test-" });
    assert(all.contents.length >= 2, "need at least 2 objects for pagination test");
    const firstKey = all.contents[0].key;
    const page2 = await client.list({ prefix: "rong-test-", startAfter: firstKey });
    const keys2 = page2.contents.map((o) => o.key);
    assert(!keys2.includes(firstKey), "startAfter excludes first key");
  });

  // ─── presign (live) ────────────────────────────────────────────

  test("client.presign() generates valid URL", async () => {
    const url = await client.presign(TEST_KEY);
    assert(typeof url === "string", "presign returns string");
    assert(url.includes(TEST_KEY), "URL contains key");
  });

  // ─── file.write ────────────────────────────────────────────────

  test("file.write() on file reference", async () => {
    const file = client.file(TEST_KEY);
    const n = await file.write("updated content");
    assert.equal(n, "updated content".length);
    const text = await file.text();
    assert.equal(text, "updated content");
  });

  // ─── delete / unlink ──────────────────────────────────────────

  test("client.delete() removes the object", async () => {
    await client.delete(TEST_KEY);
    const exists = await client.exists(TEST_KEY);
    assert.equal(exists, false);
  });

  test("client.unlink() alias works", async () => {
    const key = `rong-unlink-test-${Date.now()}.txt`;
    await client.write(key, "temp");
    await client.unlink(key);
    const exists = await client.exists(key);
    assert.equal(exists, false);
  });

  test("file.delete() removes the object", async () => {
    const key = `rong-file-delete-${Date.now()}.txt`;
    await client.write(key, "to-delete");
    const file = client.file(key);
    await file.delete();
    const exists = await client.exists(key);
    assert.equal(exists, false);
  });

  test("file.unlink() alias works", async () => {
    const key = `rong-file-unlink-${Date.now()}.txt`;
    await client.write(key, "to-unlink");
    const file = client.file(key);
    await file.unlink();
    const exists = await client.exists(key);
    assert.equal(exists, false);
  });

  // ─── Cleanup ───────────────────────────────────────────────────

  test("cleanup", async () => {
    for (const k of [TEST_JSON_KEY, TEST_BIN_KEY]) {
      try { await client.delete(k); } catch (_) {}
    }
  });
});
