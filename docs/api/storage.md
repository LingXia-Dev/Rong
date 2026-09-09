# Storage

Persistent key-value storage.

## Open

```javascript
const store = new Storage("./data.db");
const strict = new Storage("./strict.db", {
  maxKeySize: 128,
  maxValueSize: 1024 * 1024,
});
```

## Operations

```javascript
// Write (values are JSON-serialized)
await store.set("user", { name: "Alice", age: 30 });

// Read
const user = await store.get("user");
// { name: "Alice", age: 30 }

// Presence (exact key, does not load the value)
await store.has("user"); // true
await store.has("use"); // false

// Delete
await store.delete("user");

// Clear all
await store.clear();
```

## List Keys

```javascript
// List all keys
for (const key of await store.list()) {
  console.log(key);
}

// Filter by prefix
for (const key of await store.list("user:")) {
  console.log(key);
}
```

## Info

```javascript
const info = await store.info();
```

## Notes

- The standard Rong runtime exposes a global `Storage` constructor.
- `Rong.Storage` and `Rong.storage.open(...)` are not part of the default runtime API.
- Embedders may choose to inject a preconfigured global `storage` instance, but that is host-specific.
