// S3 namespace prefix tests.
// The Rust harness injects a pre-configured `Rong.s3` with namespace prefix "app1/".
// JS never calls `new Rong.S3Client` here — it uses the injected instance directly.

const s3 = Rong.s3;

describe("S3 namespace prefix", () => {
  const KEY = `ns-test-${Date.now()}.txt`;
  const CONTENT = "namespaced content";

  beforeEach(async () => {
    await s3.write(KEY, CONTENT);
  });

  afterEach(async () => {
    if (await s3.exists(KEY)) {
      await s3.delete(KEY);
    }
  });

  it("write and read through namespaced client", async () => {
    const n = await s3.write(KEY, CONTENT);
    assert.equal(n, CONTENT.length);

    const file = s3.file(KEY);
    const text = await file.text();
    assert.equal(text, CONTENT);
  });

  it("file() keeps the namespace transparent", () => {
    const file = s3.file(KEY);
    assert.equal(file.name, KEY);
  });

  it("file() rejects config override on namespaced client", () => {
    let threw = false;
    try {
      s3.file(KEY, { bucket: "other-bucket" });
    } catch (e) {
      threw = true;
      assert.equal(e.name, "TypeError");
      assert(e.message.includes("Cannot override S3 config field"), e.message);
    }
    assert(threw, "file() config override should be blocked");
  });

  it("rejects unknown options on a namespaced client", () => {
    let threw = false;
    try {
      s3.file(KEY, { typo: true });
    } catch (e) {
      threw = true;
      assert.equal(e.name, "TypeError");
      assert(e.message.includes("Option 'typo' is not allowed"), e.message);
    }
    assert(threw, "unknown namespaced options should be rejected");
  });

  it("exists returns true for namespaced key", async () => {
    assert.equal(await s3.exists(KEY), true);
  });

  it("size returns correct byte count", async () => {
    assert.equal(await s3.size(KEY), CONTENT.length);
  });

  it("stat returns metadata", async () => {
    const st = await s3.stat(KEY);
    assert(st.size > 0, "stat.size > 0");
  });

  it("list with prefix returns namespaced keys (prefix stripped)", async () => {
    const result = await s3.list({ prefix: "ns-test-" });
    assert(result.contents.length > 0, "found objects");
    // Keys should NOT contain the namespace prefix — it's transparent
    const found = result.contents.find((o) => o.key === KEY);
    assert(found, "found key without namespace prefix");
  });

  it("write() allows content type but rejects config override", async () => {
    const n = await s3.write(KEY, CONTENT, { type: "text/plain" });
    assert.equal(n, CONTENT.length);

    let threw = false;
    try {
      await s3.write(KEY, CONTENT, { bucket: "other-bucket" });
    } catch (e) {
      threw = true;
      assert.equal(e.name, "TypeError");
      assert(e.message.includes("Cannot override S3 config field"), e.message);
    }
    assert(threw, "write() config override should be blocked");
  });

  it("presign() rejects config override", async () => {
    const url = await s3.presign(KEY, { method: "GET", expiresIn: 60 });
    assert(typeof url === "string", "presign still works with safe options");

    let threw = false;
    try {
      await s3.presign(KEY, { endpoint: "http://evil.example.com" });
    } catch (e) {
      threw = true;
      assert.equal(e.name, "TypeError");
      assert(e.message.includes("Cannot override S3 config field"), e.message);
    }
    assert(threw, "presign() config override should be blocked");
  });

  it("list() rejects config override", async () => {
    let threw = false;
    try {
      await s3.list({ bucket: "other-bucket" });
    } catch (e) {
      threw = true;
      assert.equal(e.name, "TypeError");
      assert(e.message.includes("Cannot override S3 config field"), e.message);
    }
    assert(threw, "list() config override should be blocked");
  });

  it("file operations are isolated from non-prefixed keys", async () => {
    // Write directly via Rong.S3Client (no namespace prefix) to verify isolation
    const raw = new Rong.S3Client({
      accessKeyId: TEST_S3_ACCESS_KEY,
      secretAccessKey: TEST_S3_SECRET_KEY,
      bucket: TEST_S3_BUCKET,
      endpoint: TEST_S3_ENDPOINT,
    });

    // The namespaced client wrote to "app1/<KEY>", so a raw client
    // should NOT find it under just "<KEY>"
    const rawExists = await raw.exists(KEY);
    assert.equal(rawExists, false, "raw client should not see namespaced key");

    // But it should exist under the full prefixed path
    const prefixedExists = await raw.exists("app1/" + KEY);
    assert.equal(prefixedExists, true, "raw client sees full prefixed key");
  });

  it("delete removes namespaced key", async () => {
    await s3.delete(KEY);
    assert.equal(await s3.exists(KEY), false);
  });
});
