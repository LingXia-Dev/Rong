const STORAGE_DB_PATH =
  globalThis.TEST_STORAGE_DB_PATH ??
  `${
    typeof WORKSPACE_ROOT !== "undefined" ? WORKSPACE_ROOT : "."
  }/target/test-tmp/test_storage.db`;

describe("Storage API", () => {
  let storage;

  beforeEach(async () => {
    if (!storage) {
      storage = new Storage(STORAGE_DB_PATH);
    }
    await storage.clear();
  });

  test("should allow direct class construction with path", async () => {
    const direct = new Storage(`${STORAGE_DB_PATH}.direct`);
    await direct.clear();
    await direct.set("direct_key", "direct_value");
    assert.equal(await direct.get("direct_key"), "direct_value");
  });

  test("should honor custom limit options", async () => {
    const strict = new Storage(`${STORAGE_DB_PATH}.strict`, {
      maxKeySize: 4,
    });
    await strict.clear();
    await strict.set("tiny", "ok");

    let errorThrown = false;
    try {
      await strict.set("too_long_key", "nope");
    } catch (err) {
      errorThrown = true;
      assert(
        err.message.includes("Key size exceeds"),
        "Error should mention key size limit",
      );
    }
    assert(errorThrown, "Setting oversized key should throw");
  });

  test("should allow immediate info() call without table creation error", async () => {
    let errorThrown = false;
    try {
      const info = await storage.info();
      assert(typeof info.currentSize === "number");
      assert(typeof info.limitSize === "number");
      assert(typeof info.keyCount === "number");
    } catch (e) {
      errorThrown = true;
      console.error("Immediate info() call failed:", e.message);
    }
    assert(
      !errorThrown,
      "info() should work immediately without table creation errors",
    );
  });

  test("should set and get string values", async () => {
    await storage.set("test_string", "hello world");
    const value = await storage.get("test_string");
    assert.equal(value, "hello world");
  });

  test("should handle comprehensive number types", async () => {
    // Test i32 range
    await storage.set("i32_min", -2147483648); // i32::MIN
    await storage.set("i32_max", 2147483647); // i32::MAX
    await storage.set("i32_regular", 42);

    // Test u32 range
    await storage.set("u32_max", 4294967295); // u32::MAX
    await storage.set("u32_regular", 100);

    // Test floating point
    await storage.set("float_pi", 3.14159);
    await storage.set("float_negative", -2.718);

    // Test large integers (should become BigInt)
    await storage.set("i64_large", 9007199254740992n); // 2^53, beyond JS safe integer
    await storage.set("u64_moderate", 9007199254740993n); // Just beyond JS safe integer for u64
    await storage.set("i64_min", -9223372036854775808n); // i64::MIN

    // Verify all values
    assert.equal(await storage.get("i32_min"), -2147483648);
    assert.equal(await storage.get("i32_max"), 2147483647);
    assert.equal(await storage.get("i32_regular"), 42);
    assert.equal(await storage.get("u32_max"), 4294967295);
    assert.equal(await storage.get("u32_regular"), 100);
    assert.equal(await storage.get("float_pi"), 3.14159);
    assert.equal(await storage.get("float_negative"), -2.718);
    assert.equal(await storage.get("i64_large"), 9007199254740992n);
    assert.equal(await storage.get("u64_moderate"), 9007199254740993n);
    assert.equal(await storage.get("i64_min"), -9223372036854775808n);
  });

  test("should preserve BigInt type and magnitude", async () => {
    const small = 42n;
    const huge = 1n << 100n;
    await storage.set("small_bigint", small);
    await storage.set("huge_bigint", huge);

    const storedSmall = await storage.get("small_bigint");
    const storedHuge = await storage.get("huge_bigint");
    assert.equal(typeof storedSmall, "bigint");
    assert.equal(storedSmall, small);
    assert.equal(typeof storedHuge, "bigint");
    assert.equal(storedHuge, huge);
  });

  test("should preserve non-finite numbers", async () => {
    await storage.set("nan", NaN);
    await storage.set("positive_infinity", Infinity);
    await storage.set("negative_infinity", -Infinity);

    assert(Number.isNaN(await storage.get("nan")));
    assert.equal(await storage.get("positive_infinity"), Infinity);
    assert.equal(await storage.get("negative_infinity"), -Infinity);
  });

  test("should set and get boolean values", async () => {
    await storage.set("test_true", true);
    await storage.set("test_false", false);

    assert.equal(await storage.get("test_true"), true);
    assert.equal(await storage.get("test_false"), false);
  });

  test("should handle null values", async () => {
    await storage.set("test_null", null);
    const value = await storage.get("test_null");
    assert.equal(value, null);
  });

  test("should handle object values", async () => {
    const testObj = {
      name: "test",
      value: 42,
      nested: {
        array: [1, 2, 3],
        bool: true,
        float: 3.14,
      },
      nullValue: null,
    };

    await storage.set("test_object", testObj);
    const retrieved = await storage.get("test_object");

    // Verify object structure
    assert.equal(retrieved.name, "test");
    assert.equal(retrieved.value, 42);
    assert.equal(retrieved.nested.bool, true);
    assert.equal(retrieved.nested.float, 3.14);
    assert.equal(retrieved.nested.array.length, 3);
    assert.equal(retrieved.nested.array[0], 1);
    assert.equal(retrieved.nested.array[1], 2);
    assert.equal(retrieved.nested.array[2], 3);
    assert.equal(retrieved.nullValue, null);
  });

  test("should handle array values", async () => {
    const testArray = [
      1,
      "hello",
      true,
      null,
      { key: "value" },
      [1, 2, 3],
      3.14159,
    ];

    await storage.set("test_array", testArray);
    const retrieved = await storage.get("test_array");

    assert.equal(retrieved.length, 7);
    assert.equal(retrieved[0], 1);
    assert.equal(retrieved[1], "hello");
    assert.equal(retrieved[2], true);
    assert.equal(retrieved[3], null);
    assert.equal(retrieved[4].key, "value");
    assert.equal(retrieved[5].length, 3);
    assert.equal(retrieved[5][0], 1);
    assert.equal(retrieved[6], 3.14159);
  });

  test("should return undefined for non-existent keys", async () => {
    const value = await storage.get("non_existent_key");
    assert.equal(value, undefined);
  });

  test("should delete values", async () => {
    await storage.set("test_delete", "to be deleted");
    assert.equal(await storage.get("test_delete"), "to be deleted");

    await storage.delete("test_delete");
    assert.equal(await storage.get("test_delete"), undefined);
  });

  test("should list all keys with for...of", async () => {
    await storage.set("key1", "value1");
    await storage.set("key2", "value2");
    await storage.set("key3", "value3");

    const iterator = await storage.list();
    const keys = [];
    for (const key of iterator) {
      keys.push(key);
    }

    assert.equal(keys.length, 3);
    assert(keys.includes("key1"));
    assert(keys.includes("key2"));
    assert(keys.includes("key3"));
  });

  test("should list keys with prefix", async () => {
    await storage.set("user:1", "alice");
    await storage.set("user:2", "bob");
    await storage.set("config:theme", "dark");

    const iterator = await storage.list("user:");
    const userKeys = Array.from(iterator);
    assert.equal(userKeys.length, 2);
    assert(userKeys.includes("user:1"));
    assert(userKeys.includes("user:2"));
    assert(!userKeys.includes("config:theme"));
  });

  test("should clear all data", async () => {
    await storage.set("key1", "value1");
    await storage.set("key2", { test: true });

    const keysBefore = Array.from(await storage.list());
    assert.equal(keysBefore.length, 2);

    await storage.clear();

    const keysAfter = Array.from(await storage.list());
    assert.equal(keysAfter.length, 0);
  });

  test("should provide storage info", async () => {
    // Clear storage first to get accurate counts
    await storage.clear();

    // Add some test data
    await storage.set("test1", { some: "data", with: ["nested", "content"] });
    await storage.set("test2", "simple string");
    await storage.set("test3", 42);

    const info = await storage.info();
    assert(typeof info.currentSize === "number");
    assert(typeof info.limitSize === "number");
    assert(typeof info.keyCount === "number");
    assert(info.currentSize > 0);
    assert(info.limitSize > 0);
    assert.equal(info.keyCount, 3, "Should have 3 keys");
  });

  test("should reject undefined values", async () => {
    let errorThrown = false;
    try {
      await storage.set("test_undefined", undefined);
    } catch (e) {
      errorThrown = true;
      assert(e.message.includes("Cannot store undefined values"));
    }
    assert(errorThrown, "Should throw error for undefined values");
  });

  test("should handle Date values", async () => {
    // Test storing and retrieving Date objects
    const testDate = new Date("2023-12-25T10:30:00.000Z");
    await storage.set("test_date", testDate);

    const retrieved = await storage.get("test_date");
    assert(retrieved instanceof Date, "Retrieved value should be a Date");
    assert.equal(
      retrieved.getTime(),
      testDate.getTime(),
      "Date timestamps should match",
    );
    assert.equal(
      retrieved.toISOString(),
      "2023-12-25T10:30:00.000Z",
      "Date should preserve exact value",
    );

    // Test with current date
    const now = new Date();
    await storage.set("test_date_now", now);
    const retrievedNow = await storage.get("test_date_now");
    assert(
      retrievedNow instanceof Date,
      "Retrieved current date should be a Date",
    );
    assert.equal(
      retrievedNow.getTime(),
      now.getTime(),
      "Current date timestamps should match",
    );

    // Test with epoch timestamp
    const epochDate = new Date(1640995200000); // 2022-01-01 00:00:00 UTC
    await storage.set("test_epoch_date", epochDate);
    const retrievedEpoch = await storage.get("test_epoch_date");
    assert(
      retrievedEpoch instanceof Date,
      "Retrieved epoch date should be a Date",
    );
    assert.equal(
      retrievedEpoch.getTime(),
      1640995200000,
      "Epoch date should preserve timestamp",
    );
  });
});
