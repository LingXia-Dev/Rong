# rong_redis

Async Redis client. Exposed as `Rong.RedisClient`.

## JS APIs

- `Rong.RedisClient` — Redis client class
  - `new Rong.RedisClient(url)` — create a client with an explicit Redis URL
  - `connect()` — explicitly connect (optional, commands auto-connect)
  - `close()` — close the connection
  - `connected` — whether a connection is currently held
- **String operations**
  - `set(key, value)` — set a key
  - `get(key)` — get a key (returns `null` if missing)
  - `del(key)` — delete a key
  - `exists(key)` — check if a key exists
  - `expire(key, seconds)` — set TTL
  - `ttl(key)` — get remaining TTL
- **Numeric operations**
  - `incr(key)` / `decr(key)` — increment / decrement
- **Hash operations**
  - `hset(key, field, value)` / `hget(key, field)` — single field
  - `hmset(key, [field, value, ...])` / `hmget(key, [field, ...])` — multiple fields
  - `hincrby(key, field, n)` / `hincrbyfloat(key, field, n)` — increment field
- **Set operations**
  - `sadd(key, member)` / `srem(key, member)` — add / remove
  - `sismember(key, member)` — check membership
  - `smembers(key)` — get all members
  - `srandmember(key)` / `spop(key)` — random member / pop
- **List operations**
  - `lpush(key, value)` / `rpush(key, value)` — push
  - `lpop(key)` / `rpop(key)` — pop
  - `lrange(key, start, stop)` — get range
  - `llen(key)` — get length
- **Pub/Sub**
  - `publish(channel, message)` — publish a message
  - `subscribe(channel, { signal? }?)` — returns a `RedisSubscription` async iterator
  - `RedisSubscription.close()` — explicitly close a subscription
  - `for await...of` with `break` closes the underlying subscription via iterator `return()`
- **Raw commands**
  - `send(command, args)` — execute any Redis command

## Namespaced Injected Clients

When a `RedisClient` is created from Rust with a fixed or dynamic namespace, the
following JS API restriction applies:

- `send()` is **disabled** — throws `TypeError`. Raw commands could bypass the namespace prefix, breaking isolation. Use the typed methods (`set`, `get`, `hset`, etc.) instead, which automatically apply the prefix.

## Rust API

- `RedisClient::new(url)` — create an unnamespaced host-configured client.
- `.with_namespace(prefix)` — apply a fixed namespace.
- `.with_namespace_resolver(resolver)` — resolve a trusted namespace for each command; a subscription retains the namespace resolved when it is created.
- `client.into_js_object(ctx)` — register the required hidden classes and convert the client into an injectable JavaScript object.

Both fixed and dynamic namespaces fail closed: empty prefixes are rejected and
raw `send()` is disabled. Dynamic resolvers run before connection or network
I/O for each operation.

Prefer `.with_namespace(prefix)` for a stable scope; it has no resolver or
context lookup. Use `.with_namespace_resolver(...)` when one reusable Redis
connection serves changing host-managed scopes.

Keep dynamic namespace state in Rust-owned `JSContext::set_state`/`get_state`
rather than a JavaScript global. Inject the result into `ctx.host_namespace()`
when the desired API is `Rong.redis`; neither the namespace state nor a
top-level `redis` property needs to be exposed.
