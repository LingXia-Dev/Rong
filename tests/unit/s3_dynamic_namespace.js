const s3 = Rong.dynamicS3;
const emptyNamespaceS3 = Rong.emptyNamespaceS3;
const resolverErrorS3 = Rong.resolverErrorS3;

describe("S3 dynamic namespace prefix", () => {
  const KEY = `dynamic-ns-${Date.now()}.txt`;

  afterEach(async () => {
    Rong.__setTestS3Namespace("scope-a");
    await s3.delete(KEY);
    Rong.__setTestS3Namespace("scope-b");
    await s3.delete(KEY);
  });

  test("resolves the namespace independently for every client operation", async () => {
    Rong.__setTestS3Namespace("scope-a");
    await s3.write(KEY, "value-a");

    Rong.__setTestS3Namespace("scope-b");
    await s3.write(KEY, "value-b");
    assert.equal(await s3.file(KEY).text(), "value-b");

    Rong.__setTestS3Namespace("scope-a");
    assert.equal(await s3.file(KEY).text(), "value-a");
  });

  test("re-resolves the namespace when a lazy S3File is used", async () => {
    const file = s3.file(KEY);

    Rong.__setTestS3Namespace("scope-a");
    await file.write("file-a");

    Rong.__setTestS3Namespace("scope-b");
    await file.write("file-b");
    assert.equal(await file.text(), "file-b");

    const slice = file.slice(5, 6);
    assert.equal(await slice.text(), "b");

    Rong.__setTestS3Namespace("scope-a");
    assert.equal(await file.text(), "file-a");
    assert.equal(await slice.text(), "a");
  });

  test("uses one resolved namespace throughout list", async () => {
    Rong.__setTestS3Namespace("scope-a");
    await s3.write(KEY, "listed-a");

    Rong.__setTestS3Namespace("scope-b");
    await s3.write(KEY, "listed-b");

    const result = await s3.list({ prefix: "dynamic-ns-" });
    assert(result.contents.some((entry) => entry.key === KEY));
    assert(result.contents.every((entry) => !entry.key.startsWith("scope-")));
  });

  test("rejects an empty namespace instead of using raw object keys", async () => {
    let threw = false;
    try {
      await emptyNamespaceS3.exists(KEY);
    } catch (error) {
      threw = true;
      assert.equal(error.name, "TypeError");
      assert(error.message.includes("must not be empty"), error.message);
    }
    assert(threw, "an empty namespace should be rejected");

    const file = emptyNamespaceS3.file(KEY);
    threw = false;
    try {
      await file.exists();
    } catch (error) {
      threw = true;
      assert.equal(error.name, "TypeError");
      assert(error.message.includes("must not be empty"), error.message);
    }
    assert(threw, "a lazy file should reject an empty namespace when used");
  });

  test("propagates resolver errors before bucket construction or network I/O", async () => {
    let threw = false;
    try {
      await resolverErrorS3.exists(KEY);
    } catch (error) {
      threw = true;
      assert.equal(error.name, "TypeError");
      assert(error.message.includes("namespace unavailable"), error.message);
    }
    assert(threw, "the namespace resolver should run first");

    threw = false;
    try {
      await resolverErrorS3.list();
    } catch (error) {
      threw = true;
      assert.equal(error.name, "TypeError");
      assert(error.message.includes("namespace unavailable"), error.message);
    }
    assert(threw, "list should resolve the namespace before constructing a bucket");
  });
});
