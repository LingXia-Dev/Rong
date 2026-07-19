// Storage injected instance tests.
// The Rust harness injects a pre-configured `storage` global.
// JS never calls `new Storage` — it uses the injected instance directly.

describe("Storage injected instance", () => {
  beforeEach(async () => {
    await storage.clear();
  });

  test("set and get", async () => {
    await storage.set("key", "value");
    assert.equal(await storage.get("key"), "value");
  });

  test("stores different value types", async () => {
    await storage.set("str", "hello");
    await storage.set("num", 42);
    await storage.set("bool", true);
    await storage.set("obj", { name: "test" });

    assert.equal(await storage.get("str"), "hello");
    assert.equal(await storage.get("num"), 42);
    assert.equal(await storage.get("bool"), true);
    const obj = await storage.get("obj");
    assert.equal(obj.name, "test");
  });

  test("delete removes key", async () => {
    await storage.set("key", "val");
    await storage.delete("key");
    assert.equal(await storage.get("key"), undefined);
  });

  test("clear removes all keys", async () => {
    await storage.set("a", "1");
    await storage.set("b", "2");
    await storage.clear();
    const info = await storage.info();
    assert.equal(info.keyCount, 0);
  });

  test("list returns keys", async () => {
    await storage.set("user:alice", "a");
    await storage.set("user:bob", "b");
    await storage.set("config", "c");

    const allKeys = [];
    for (const k of await storage.list()) {
      allKeys.push(k);
    }
    assert.equal(allKeys.length, 3);

    const userKeys = [];
    for (const k of await storage.list("user:")) {
      userKeys.push(k);
    }
    assert.equal(userKeys.length, 2);
  });

  test("info returns storage metadata", async () => {
    await storage.set("key", "value");
    const info = await storage.info();
    assert(info.currentSize > 0, "currentSize > 0");
    assert(info.limitSize > 0, "limitSize > 0");
    assert.equal(info.keyCount, 1);
  });

  test("respects custom data size limit from Rust config", async () => {
    // The Rust harness set maxDataSize to 10MB.
    const info = await storage.info();
    assert.equal(info.limitSize, 10 * 1024 * 1024);
  });
});
