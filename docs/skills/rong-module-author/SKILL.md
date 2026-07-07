---
name: rong-module-author
description: >-
  Author and modify RongJS (Rong) modules - expose Rust functions and classes to
  JavaScript, convert values across the Rust<->JS boundary, raise JS errors from
  Rust, and wire a new `rong_<name>` module crate into the runtime. Use when
  writing or editing Rong module code: `#[js_export]` / `#[js_class]` /
  `#[js_method]`, `JSFunc::new` registration, `FromJSObj` / `IntoJSObj` types,
  `JSResult`/`HostError` error handling, a module `init(ctx)` function, or
  registering a class with `register_class` / `register_hidden_class`.
license: MIT OR Apache-2.0
metadata:
  version: 0.1.0
  project: RongJS
  source: https://github.com/LingXia-Dev/Rong
---

# Authoring RongJS modules

Use the smallest reference needed:

- `references/functions.md`: free functions, `JSFunc`, registration.
- `references/classes.md`: classes, methods, object shapes, `#[js_const_enum]`.
- `references/type-conversion.md`: Rust<->JS mapping and dynamic values.
- `references/errors.md`: `JSResult`, `HostError`, preserving JS-thrown values.
- `references/module-structure.md`: new crate wiring and tests.

## Core Rules

- Every module exposes `pub fn init(ctx: &JSContext) -> JSResult<()>`.
- Use `JSResult<T>` for fallible APIs: `Ok` returns/resolves, `Err` throws/rejects.
- Use `async fn` for I/O; it becomes a JS `Promise`.
- Public option/result objects should be named Rust structs with `FromJSObj` /
  `IntoJSObj`, not hand-parsed or hand-built `JSObject`.
- Use `#[ts_type = "..."]` only when generated TypeScript needs precision Rust
  cannot infer; use `#[ts_skip]` only for internal derived parser structs.
- Numeric constant objects such as `Rong.SeekMode` should use `#[js_const_enum]`,
  not TS enum preludes or hand-built JS objects.
- Preserve JS-thrown values with `RongJSError::from_thrown_value(value)`. Detect
  thrown values with `value.is_exception()`, not `is_error()`.

## Registration Sketch

```rust
use rong::*;

pub fn init(ctx: &JSContext) -> JSResult<()> {
    ctx.register_hidden_class::<Thing>()?;

    let f = JSFunc::new(ctx, do_work)?.name("doWork")?;
    ctx.host_namespace().set("doWork", f)?;
    Ok(())
}
```

Use `register_class::<T>()` only when the constructor should be globally visible.

## Checklist

1. Read a nearby module (`rong_url`, `rong_fs`, `rong_http`) and match patterns.
2. Choose free functions, classes, or both.
3. Model public API shapes in Rust so `rong_typegen` can generate declarations.
4. Wire `init(ctx)` and, for new crates, `rong_modules` feature plumbing.
5. Add engine-backed tests.
6. Run `cargo run -p rong_typegen`, `cargo run -p rong_typegen -- --check`, and
   `cargo test -p rong_<name> --features quickjs`.
