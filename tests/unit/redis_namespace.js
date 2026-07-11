// Redis namespace prefix tests.
// The Rust harness injects a pre-configured `Rong.redis` with namespace prefix "app1:".
// JS never calls `new Rong.RedisClient` here — it uses the injected instance directly.

const redis = Rong.redis;

describe("Redis namespace prefix", () => {
  afterEach(async () => {
    await redis.del("mykey");
    await redis.del("counter");
    await redis.del("myhash");
    await redis.del("myset");
    await redis.del("mylist");
  });

  it("set and get through namespaced client", async () => {
    await redis.set("mykey", "hello");
    assert.equal(await redis.get("mykey"), "hello");
  });

  it("keys are isolated from non-prefixed access", async () => {
    await redis.set("mykey", "namespaced");

    // Create a raw client (no namespace) to verify isolation
    const raw = new Rong.RedisClient(TEST_REDIS_URL);
    try {
      // Raw client should NOT see the key under "mykey"
      assert.equal(await raw.get("mykey"), null);
      // But should see it under the full prefixed key
      assert.equal(await raw.get("app1:mykey"), "namespaced");
    } finally {
      raw.close();
    }
  });

  it("del works on namespaced key", async () => {
    await redis.set("mykey", "val");
    assert.equal(await redis.del("mykey"), 1);
    assert.equal(await redis.get("mykey"), null);
  });

  it("exists works on namespaced key", async () => {
    assert.equal(await redis.exists("mykey"), false);
    await redis.set("mykey", "val");
    assert.equal(await redis.exists("mykey"), true);
  });

  it("incr/decr work on namespaced key", async () => {
    assert.equal(await redis.incr("counter"), 1);
    assert.equal(await redis.incr("counter"), 2);
    assert.equal(await redis.decr("counter"), 1);
  });

  it("hash operations work on namespaced key", async () => {
    await redis.hset("myhash", "name", "Alice");
    assert.equal(await redis.hget("myhash", "name"), "Alice");
  });

  it("set operations work on namespaced key", async () => {
    await redis.sadd("myset", "a");
    await redis.sadd("myset", "b");
    const members = await redis.smembers("myset");
    assert.equal(members.length, 2);
  });

  it("list operations work on namespaced key", async () => {
    await redis.rpush("mylist", "first");
    await redis.rpush("mylist", "second");
    assert.equal(await redis.llen("mylist"), 2);
    const items = await redis.lrange("mylist", 0, -1);
    assert.equal(items[0], "first");
    assert.equal(items[1], "second");
  });

  it("send is disabled on namespaced injected clients", async () => {
    let threw = false;
    try {
      await redis.send("SET", ["unsafe", "value"]);
    } catch (e) {
      threw = true;
      assert.equal(e.name, "TypeError");
      assert(e.message.includes("disabled"), e.message);
    }
    assert(threw, "send should be blocked");
  });
});
