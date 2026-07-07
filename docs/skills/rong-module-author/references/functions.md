# Functions: exposing Rust functions to JavaScript

Functions are for standalone utilities and module APIs (e.g. `Rong.cwd()`).

## Async functions (I/O)

Most functions that perform I/O should be `async`. Rong converts them to JS
Promises automatically.

```rust
use rong::*;

/// Read a file's contents
async fn read_file(path: String) -> JSResult<String> {
    tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| HostError::new(rong::error::E_IO, format!("read failed: {e}")).into())
}
```

```javascript
const content = await Rong.readFile("data.txt"); // returns a Promise
```

## Sync functions

Synchronous functions return values directly. Use them **only** for
non-blocking work.

```rust
fn is_absolute(path: String) -> bool {
    std::path::Path::new(&path).is_absolute()
}
```

```javascript
const absolute = Rong.isAbsolute("/usr/bin"); // direct value
```

## Registration

Wrap with `JSFunc::new(ctx, fn)`, name it (for stack traces), and attach it.

```rust
pub fn init(ctx: &JSContext) -> JSResult<()> {
    let rong = ctx.host_namespace();

    let read_fn = JSFunc::new(ctx, read_file)?.name("readFile")?;
    rong.set("readFile", read_fn)?;

    let abs_fn = JSFunc::new(ctx, is_absolute)?.name("isAbsolute")?;
    rong.set("isAbsolute", abs_fn)?;

    Ok(())
}
```

- `ctx.host_namespace()` returns the host `Rong` object. To attach to the global
  object or a sub-object instead, use the relevant `JSObject` and `.set(...)`.
- `.name("...")` sets the function's `.name` (helps stack traces).

## Optional parameters

Use `Optional<T>` from `rong::function`. The inner value is `Option<T>` at `.0`.

```rust
use rong::function::Optional;

async fn read_with_encoding(path: String, encoding: Optional<String>) -> JSResult<String> {
    let enc = encoding.0.unwrap_or_else(|| "utf-8".to_string());
    // ...
    todo!()
}
```

JS callers may omit trailing optional arguments.

## Returning values

The return type is converted automatically (see `type-conversion.md`):

- `JSResult<T>` - `Ok` returns/resolves, `Err` throws/rejects.
- `T` directly for infallible sync functions (e.g. `bool`, `String`).
- `Option<T>` maps to `T` or `null`.
- Object-shaped returns: derive `IntoJSObj` (see `classes.md`).

For public APIs, do not hand-build result objects with `JSObject::new(...).set(...)`
unless the shape is intentionally dynamic. Use a named `IntoJSObj` struct so the
same Rust shape also drives generated TypeScript.
