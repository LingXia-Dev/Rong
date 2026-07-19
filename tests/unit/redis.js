const REDIS_URL = globalThis.TEST_REDIS_URL ?? "redis://127.0.0.1:6379";

async function waitForQuiet(ms = 75) {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

describe("RedisClient — connection", () => {
  let client;

  test("hides RedisSubscription from global scope", () => {
    assert.equal(typeof RedisSubscription, "undefined");
    assert.equal(globalThis.RedisSubscription, undefined);
  });

  test("does not expose RedisClient on globalThis", () => {
    assert.equal(typeof RedisClient, "undefined");
    assert.equal(globalThis.RedisClient, undefined);
    assert.equal(typeof Rong.RedisClient, "function");
  });

  beforeEach(() => {
    client = new Rong.RedisClient(REDIS_URL);
  });
  afterEach(() => client.close());

  test("requires an explicit URL", () => {
    let threw = false;
    try {
      new Rong.RedisClient();
    } catch (e) {
      threw = true;
      assert.equal(e.name, "TypeError");
      assert(e.message.includes("explicit Redis URL"));
    }
    assert(threw, "constructor should require an explicit url");
  });

  test("connect explicitly", async () => {
    await client.connect();
    assert.equal(client.connected, true);
  });

  test("close connection", async () => {
    await client.connect();
    client.close();
    assert.equal(client.connected, false);
  });

  test("reconnects after close", async () => {
    await client.set("test:rc", "val");
    client.close();
    const val = await client.get("test:rc");
    assert.equal(val, "val");
    assert.equal(client.connected, true);
  });

  test("close is idempotent", () => {
    client.close();
    client.close();
    assert.equal(client.connected, false);
  });

  test("throws a TypeError for invalid URL", async () => {
    const broken = new Rong.RedisClient("not a redis url");
    let threw = false;
    try {
      await broken.connect();
    } catch (e) {
      threw = true;
      assert.equal(e.name, "TypeError");
      assert(e.message.includes("Invalid Redis URL"));
    } finally {
      broken.close();
    }
    assert(threw, "should throw for invalid redis url");
  });
});

describe("RedisClient — strings", () => {
  let client;

  beforeEach(async () => {
    client = new Rong.RedisClient(REDIS_URL);
    await client.del("test:key");
  });
  afterEach(() => client.close());

  test("set and get", async () => {
    await client.set("test:key", "hello");
    assert.equal(await client.get("test:key"), "hello");
  });

  test("get returns null for missing key", async () => {
    assert.equal(await client.get("test:missing"), null);
  });

  test("del removes a key", async () => {
    await client.set("test:key", "v");
    assert.equal(await client.del("test:key"), 1);
    assert.equal(await client.get("test:key"), null);
  });

  test("exists checks key presence", async () => {
    assert.equal(await client.exists("test:key"), false);
    await client.set("test:key", "v");
    assert.equal(await client.exists("test:key"), true);
  });
});

describe("RedisClient — TTL", () => {
  let client;

  beforeEach(async () => {
    client = new Rong.RedisClient(REDIS_URL);
    await client.del("test:ttl");
  });
  afterEach(() => client.close());

  test("expire and ttl", async () => {
    await client.set("test:ttl", "data");
    await client.expire("test:ttl", 100);
    const ttl = await client.ttl("test:ttl");
    assert(ttl > 0 && ttl <= 100, `TTL should be 1-100, got ${ttl}`);
  });

  test("ttl -1 for no expiry", async () => {
    await client.set("test:ttl", "data");
    assert.equal(await client.ttl("test:ttl"), -1);
  });

  test("ttl -2 for nonexistent key", async () => {
    assert.equal(await client.ttl("test:missing"), -2);
  });
});

describe("RedisClient — numeric", () => {
  let client;

  beforeEach(async () => {
    client = new Rong.RedisClient(REDIS_URL);
    await client.del("test:counter");
  });
  afterEach(() => client.close());

  test("incr", async () => {
    assert.equal(await client.incr("test:counter"), 1);
    assert.equal(await client.incr("test:counter"), 2);
  });

  test("decr", async () => {
    await client.set("test:counter", "10");
    assert.equal(await client.decr("test:counter"), 9);
  });
});

describe("RedisClient — hashes", () => {
  let client;

  beforeEach(async () => {
    client = new Rong.RedisClient(REDIS_URL);
    await client.del("test:hash");
  });
  afterEach(() => client.close());

  test("hset and hget", async () => {
    await client.hset("test:hash", "name", "Alice");
    assert.equal(await client.hget("test:hash", "name"), "Alice");
  });

  test("hget null for missing field", async () => {
    await client.hset("test:hash", "a", "1");
    assert.equal(await client.hget("test:hash", "nope"), null);
  });

  test("hmset and hmget", async () => {
    await client.hmset("test:hash", ["name", "Alice", "email", "a@b.c"]);
    const r = await client.hmget("test:hash", ["name", "email"]);
    assert.equal(r[0], "Alice");
    assert.equal(r[1], "a@b.c");
  });

  test("hmget null for missing fields", async () => {
    await client.hmset("test:hash", ["name", "Alice"]);
    const r = await client.hmget("test:hash", ["name", "missing"]);
    assert.equal(r[0], "Alice");
    assert.equal(r[1], null);
  });

  test("hmset rejects odd-length field lists", async () => {
    let threw = false;
    try {
      await client.hmset("test:hash", ["name", "Alice", "orphan"]);
    } catch (e) {
      threw = true;
      assert.equal(e.name, "TypeError");
      assert(e.message.includes("even length"));
    }
    assert(threw, "hmset should reject odd-length field arrays");
  });

  test("hincrby", async () => {
    await client.hset("test:hash", "visits", "10");
    assert.equal(await client.hincrby("test:hash", "visits", 5), 15);
  });

  test("hincrbyfloat", async () => {
    await client.hset("test:hash", "score", "10.5");
    assert.equal(await client.hincrbyfloat("test:hash", "score", 1.5), 12);
  });
});

describe("RedisClient — sets", () => {
  let client;

  beforeEach(async () => {
    client = new Rong.RedisClient(REDIS_URL);
    await client.del("test:set");
  });
  afterEach(() => client.close());

  test("sadd and smembers", async () => {
    await client.sadd("test:set", "a");
    await client.sadd("test:set", "b");
    await client.sadd("test:set", "c");
    const m = await client.smembers("test:set");
    assert.equal(m.length, 3);
    assert(m.includes("a") && m.includes("b") && m.includes("c"));
  });

  test("srem", async () => {
    await client.sadd("test:set", "x");
    await client.sadd("test:set", "y");
    await client.srem("test:set", "x");
    const m = await client.smembers("test:set");
    assert.equal(m.length, 1);
    assert(m.includes("y"));
  });

  test("sismember", async () => {
    await client.sadd("test:set", "m");
    assert.equal(await client.sismember("test:set", "m"), true);
    assert.equal(await client.sismember("test:set", "x"), false);
  });

  test("srandmember", async () => {
    await client.sadd("test:set", "only");
    assert.equal(await client.srandmember("test:set"), "only");
  });

  test("spop", async () => {
    await client.sadd("test:set", "pop");
    assert.equal(await client.spop("test:set"), "pop");
    assert.equal((await client.smembers("test:set")).length, 0);
  });
});

describe("RedisClient — lists", () => {
  let client;

  beforeEach(async () => {
    client = new Rong.RedisClient(REDIS_URL);
    await client.del("test:list");
  });
  afterEach(() => client.close());

  test("lpush, rpush, lrange", async () => {
    await client.rpush("test:list", "a");
    await client.rpush("test:list", "b");
    await client.lpush("test:list", "z");
    const list = await client.lrange("test:list", 0, -1);
    assert.equal(list[0], "z");
    assert.equal(list[1], "a");
    assert.equal(list[2], "b");
  });

  test("lpop and rpop", async () => {
    await client.rpush("test:list", "1");
    await client.rpush("test:list", "2");
    await client.rpush("test:list", "3");
    assert.equal(await client.lpop("test:list"), "1");
    assert.equal(await client.rpop("test:list"), "3");
  });

  test("lpop/rpop null on empty", async () => {
    assert.equal(await client.lpop("test:list"), null);
    assert.equal(await client.rpop("test:list"), null);
  });

  test("llen", async () => {
    assert.equal(await client.llen("test:list"), 0);
    await client.rpush("test:list", "a");
    await client.rpush("test:list", "b");
    assert.equal(await client.llen("test:list"), 2);
  });
});

describe("RedisClient — raw send", () => {
  let client;

  beforeEach(async () => {
    client = new Rong.RedisClient(REDIS_URL);
    await client.del("test:key");
    await client.del("test:counter");
  });
  afterEach(() => client.close());

  test("send GET", async () => {
    await client.set("test:key", "hello");
    assert.equal(await client.send("GET", ["test:key"]), "hello");
  });

  test("send MSET", async () => {
    await client.send("MSET", ["test:key", "a", "test:counter", "b"]);
    assert.equal(await client.get("test:key"), "a");
    assert.equal(await client.get("test:counter"), "b");
  });

  test("send maps arrays, integers, and nulls", async () => {
    await client.send("MSET", ["test:key", "a", "test:counter", "b"]);

    const values = await client.send("MGET", ["test:key", "test:counter", "test:missing"]);
    assert.equal(values.length, 3);
    assert.equal(values[0], "a");
    assert.equal(values[1], "b");
    assert.equal(values[2], null);

    assert.equal(await client.send("DEL", ["test:missing"]), 0);
    assert.equal(await client.send("GET", ["test:missing"]), null);
  });

  test("send preserves large integer replies as bigint", async () => {
    await client.del("test:big-counter");
    const result = await client.send("INCRBY", ["test:big-counter", "9007199254740993"]);
    assert.equal(typeof result, "bigint");
    assert.equal(result, 9007199254740993n);
  });
});

describe("RedisClient — pub/sub", () => {
  let pub_client;
  let sub_client;
  let subscriptions;

  beforeEach(async () => {
    pub_client = new Rong.RedisClient(REDIS_URL);
    sub_client = new Rong.RedisClient(REDIS_URL);
    subscriptions = [];
  });

  afterEach(() => {
    for (const sub of subscriptions) {
      sub.close();
    }
    sub_client.close();
    pub_client.close();
  });

  async function subscribe(channel, options) {
    const sub = await sub_client.subscribe(channel, options);
    subscriptions.push(sub);
    return sub;
  }

  test("subscribe returns an async-iterable subscription", async () => {
    const sub = await subscribe("test:ch");
    assert.equal(sub.channel, "test:ch");
    assert.equal(sub[Symbol.asyncIterator](), sub);

    const pending = sub.next();
    await pub_client.publish("test:ch", "hello pub/sub");
    const received = await pending;

    assert.equal(received.done, false);
    assert.equal(received.value.message, "hello pub/sub");
    assert.equal(received.value.channel, "test:ch");
  });

  test("subscribe to multiple channels", async () => {
    const subA = await subscribe("ch:a");
    const subB = await subscribe("ch:b");

    const nextA = subA.next();
    const nextB = subB.next();
    await pub_client.publish("ch:a", "one");
    await pub_client.publish("ch:b", "two");

    const receivedA = await nextA;
    const receivedB = await nextB;
    assert.equal(receivedA.value.channel, "ch:a");
    assert.equal(receivedA.value.message, "one");
    assert.equal(receivedB.value.channel, "ch:b");
    assert.equal(receivedB.value.message, "two");
  });

  test("multiple subscriptions on the same channel each receive messages", async () => {
    const first = await subscribe("test:shared");
    const second = await subscribe("test:shared");

    const nextFirst = first.next();
    const nextSecond = second.next();
    await pub_client.publish("test:shared", "message");

    const firstMessage = await nextFirst;
    const secondMessage = await nextSecond;
    assert.equal(firstMessage.value.message, "message");
    assert.equal(secondMessage.value.message, "message");
  });

  test("close stops receiving and is idempotent", async () => {
    const sub = await subscribe("test:close");

    const first = sub.next();
    await pub_client.publish("test:close", "before");
    const received = await first;
    assert.equal(received.value.message, "before");

    sub.close();
    sub.close();
    await waitForQuiet();

    await pub_client.publish("test:close", "after");
    await waitForQuiet();

    const done = await sub.next();
    assert.equal(done.done, true);
  });

  test("break in for-await closes the subscription", async () => {
    const sub = await subscribe("test:break");
    const messages = [];

    const loop = (async () => {
      for await (const event of sub) {
        messages.push(event.message);
        break;
      }
    })();

    await pub_client.publish("test:break", "first");
    await loop;
    await waitForQuiet();

    await pub_client.publish("test:break", "second");
    await waitForQuiet();

    assert.equal(messages.length, 1);
    assert.equal(messages[0], "first");

    const done = await sub.next();
    assert.equal(done.done, true);
  });

  test("client.close closes active subscriptions", async () => {
    const sub = await subscribe("test:client-close");
    sub_client.close();
    await waitForQuiet();

    const done = await sub.next();
    assert.equal(done.done, true);
  });

  test("AbortSignal closes the subscription", async () => {
    const controller = new AbortController();
    const sub = await subscribe("test:abort", { signal: controller.signal });

    controller.abort(new Error("aborted"));
    await waitForQuiet();

    const done = await sub.next();
    assert.equal(done.done, true);
  });

  test("already-aborted signals reject subscribe", async () => {
    const controller = new AbortController();
    controller.abort(new Error("stop now"));

    let threw = false;
    try {
      await sub_client.subscribe("test:abort-now", { signal: controller.signal });
    } catch (e) {
      threw = true;
      assert(e.message.includes("stop now"));
    }
    assert(threw, "subscribe should reject for an already-aborted signal");
  });
});
