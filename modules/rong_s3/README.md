# rong_s3

S3-compatible object storage client. Exposed as `Rong.S3Client`.

## JS APIs

- `Rong.S3Client` — S3 client class
  - `new Rong.S3Client(options?)` — create a client
    - Options: `accessKeyId`, `secretAccessKey`, `bucket`, `region`, `endpoint`, `sessionToken`, `acl`, `virtualHostedStyle`
  - `file(path, options?)` — lazy `S3File` reference (no network request)
  - `write(path, data, options?)` — upload data, returns bytes written
  - `delete(path)` / `unlink(path)` — delete an object
  - `exists(path)` — check if an object exists
  - `size(path)` — get object size in bytes
  - `stat(path)` — get object metadata (`etag`, `lastModified`, `size`, `type`)
  - `presign(path, options?)` — generate a presigned URL
  - `list(options?)` — list objects (`prefix`, `maxKeys`, `startAfter`)
- `S3File` — lazy reference to an S3 object (via `client.file()`)
  - `text()` / `json()` / `bytes()` / `arrayBuffer()` — read
  - `write(data, options?)` — write
  - `slice(start, end)` — partial read reference
  - `exists()` / `stat()` / `delete()` / `unlink()` — metadata & delete
  - `presign(options?)` — presigned URL
  - `name` / `size` — getters

## Host-injected clients

Every `S3Client` created from Rust has locked configuration, whether or not it
uses a namespace prefix. JavaScript cannot override `accessKeyId`,
`secretAccessKey`, `sessionToken`, `region`, `endpoint`, `bucket`, `acl`, or
`virtualHostedStyle` in method options. This prevents an injected client from
escaping the host-owned credentials and endpoint. Clients constructed from
JavaScript retain the documented per-method configuration overrides.

Allowed option fields per method:

| Method | Allowed options |
|--------|----------------|
| `file(path, options?)` | *(none)* |
| `write(path, data, options?)` | `type` |
| `presign(path, options?)` | `expiresIn`, `method` |
| `list(options?)` | `prefix`, `maxKeys`, `startAfter` |

## Rust API

- `S3Client::new(config)` — create an unnamespaced host-configured client with locked configuration.
- `.with_namespace(prefix)` — apply a prefix that is transparently prepended to object keys and stripped from list results.
- `client.into_js_object(ctx)` — register the required hidden classes and convert the client into an injectable JavaScript object.
- `S3Config` — configuration struct with public fields (`access_key_id`, `secret_access_key`, `bucket`, `region`, `endpoint`, etc.)

Use `ctx.host_namespace().set("s3", client.into_js_object(ctx)?)` to expose the
managed instance as `Rong.s3` without creating a separate top-level global.
`S3Client` stores configuration rather than a live connection, so creating a
cheap wrapper per fixed namespace is preferred over a dynamic resolver on every
S3 operation.
