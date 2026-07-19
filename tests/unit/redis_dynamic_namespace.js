const redis = Rong.redis;
const emptyNamespaceRedis = Rong.emptyNamespaceRedis;
const resolverErrorRedis = Rong.resolverErrorRedis;

describe("Redis dynamic namespace prefix", () => {
  afterEach(async () => {
    Rong.__setTestNamespace("scope-a");
    await redis.del("shared-key");
    Rong.__setTestNamespace("scope-b");
    await redis.del("shared-key");
  });

  test("resolves the namespace independently for every operation", async () => {
    Rong.__setTestNamespace("scope-a");
    await redis.set("shared-key", "value-a");

    Rong.__setTestNamespace("scope-b");
    await redis.set("shared-key", "value-b");
    assert.equal(await redis.get("shared-key"), "value-b");

    Rong.__setTestNamespace("scope-a");
    assert.equal(await redis.get("shared-key"), "value-a");
  });

  test("disables raw commands", async () => {
    let threw = false;
    try {
      await redis.send("GET", ["shared-key"]);
    } catch (error) {
      threw = true;
      assert.equal(error.name, "TypeError");
      assert(error.message.includes("disabled"), error.message);
    }
    assert(threw, "send should be blocked");
  });

  test("rejects an empty namespace instead of falling back to raw keys", async () => {
    let threw = false;
    try {
      await emptyNamespaceRedis.get("shared-key");
    } catch (error) {
      threw = true;
      assert.equal(error.name, "TypeError");
      assert(error.message.includes("must not be empty"), error.message);
    }
    assert(threw, "an empty namespace should be rejected");
  });

  test("resolves the namespace before opening a connection", async () => {
    let threw = false;
    try {
      await resolverErrorRedis.get("shared-key");
    } catch (error) {
      threw = true;
      assert.equal(error.name, "TypeError");
      assert(error.message.includes("namespace unavailable"), error.message);
      assert(!error.message.includes("Redis URL"), error.message);
    }
    assert(threw, "the namespace resolver should run before connection setup");
  });
});
