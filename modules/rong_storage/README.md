# rong_storage

Key-value storage backed by a local database file.

## JS APIs

- `new Storage(path, options?)` — construct a storage instance directly
  - Options: `maxKeySize`, `maxValueSize`, `maxDataSize`
- `Storage` instance methods:
  - `set(key, value)` — store a key-value pair
  - `get(key)` — retrieve a value by key
  - `has(key)` — `true` when the exact key exists; does not load the value
  - `delete(key)` — remove a key
  - `clear()` — remove all entries
  - `list(prefix?)` — list keys, optionally filtered by prefix
  - `info()` — get storage info (`currentSize`, `limitSize`, `keyCount`)

## Rust API

- `Storage::new(path, options)` — create a pre-configured instance from Rust, useful for environments that inject instances via a platform namespace instead of exposing the JS constructor.
