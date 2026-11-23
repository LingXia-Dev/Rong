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

## Registration and generated types

Declare host-namespace functions once with `js_api!`. The macro generates
the runtime registration function, and `rong_typegen` reads the same declaration
to emit the named mergeable TypeScript interface. The `namespace` declaration
pairs that interface with the runtime object, so downstream runtimes can target
their own namespace rather than `Rong`.

```rust
rong::js_api! {
    fn register_functions(ctx) {
        namespace RongNamespace = ctx.host_namespace();
        fn readFile = read_file;
        fn isAbsolute = is_absolute;
    }
}

pub fn init(ctx: &JSContext) -> JSResult<()> {
    register_functions(ctx)
}
```

- Rust signatures are inferred automatically; use `ts_params` or `ts_return` in
  the declaration only when a dynamic Rust boundary type loses TS precision.
- Use `class Name = RustType` for a constructor installed on the target object;
  registration and `readonly Name: typeof Name` are generated together.
- Use `const Name: "TsType" = expression` for a readonly runtime namespace
  value whose exact TypeScript type cannot be inferred from the expression.
- Use `type Name = "..."` for a TypeScript-only alias that has no precise Rust
  representation. Keep it beside the functions that reference it.
- Use `cfg = "unix"` for a platform-gated runtime registration. Typegen keeps
  it in the target-agnostic npm declaration union.
- Direct `JSFunc::new(...).set(...)` remains available for intentionally dynamic
  registration, but it cannot participate in automatic namespace generation.

## Closures with state

A host function is shared: script may call it again while it runs (a callback
that uses the same API). `JSFunc::new`, `ClassSetup::method` and `new_func`
therefore take an `Fn`; a closure that changes a capture directly does not
compile. Keep such state in a `Cell` or `RefCell`, and end a `RefCell` borrow
before calling into script.

```rust
let calls = Cell::new(0u32);
let seen = RefCell::new(Vec::<String>::new());
JSFunc::new(ctx, move |name: String, callback: JSFunc| -> JSResult<u32> {
    calls.set(calls.get() + 1);
    seen.borrow_mut().push(name);       // released at the end of the statement
    callback.call::<_, ()>(None, ())?;  // may call this function again
    Ok(calls.get())
})?;
```

`JSFunc::new_once` takes an `FnOnce`; a second call throws.

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
- Object-shaped returns: derive `IntoJSObject` (see `classes.md`).

For public APIs, do not hand-build result objects with `JSObject::new(...).set(...)`
unless the shape is intentionally dynamic. Use a named `IntoJSObject` struct so the
same Rust shape also drives generated TypeScript.
