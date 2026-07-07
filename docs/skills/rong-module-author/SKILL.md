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

Rong (RongJS) is an embeddable, multi-engine JavaScript runtime (QuickJS,
JavaScriptCore, ArkJS) with a single Rust API. A **module** is a Rust crate that
exposes Rust functions and classes to JavaScript through that API. This skill
helps you write correct, idiomatic module code.

Use the reference files in this skill for depth - load the one that matches the
task:

- **[references/functions.md](references/functions.md)** - sync/async functions, `JSFunc`, optional params, registration.
- **[references/classes.md](references/classes.md)** - `#[js_export]`/`#[js_class]`/`#[js_method]`, constructors, getters/setters, static & mutable methods, `FromJSObj`/`IntoJSObj`, union-type params.
- **[references/type-conversion.md](references/type-conversion.md)** - the Rust<->JS type mapping, `JSValue`/`JSObject`/`JSArray` ops, type-checking helpers.
- **[references/errors.md](references/errors.md)** - `JSResult`, `HostError`, error codes, preserving thrown JS values, throw vs no-throw APIs.
- **[references/module-structure.md](references/module-structure.md)** - create a new `rong_<name>` crate, the `init(ctx)` pattern, wiring into `rong_modules`, and writing tests.

## Mental model

- A module crate exposes one entry point: `pub fn init(ctx: &JSContext) -> JSResult<()>`. Inside it you register functions and classes onto the context.
- **Functions** are for standalone utilities/APIs (`Rong.cwd()`); **classes** are for stateful objects (`new Point2D(10, 20)`).
- The Rust<->JS bridge type is `JSResult<T>`. This is a hard rule everywhere:
  - `Ok(v)` -> JS normal return / `Promise.resolve(v)`
  - `Err(e)` -> JS `throw` / `Promise.reject`
- `async fn` becomes a JS `Promise` automatically; sync `fn` returns directly (use sync only for non-blocking work).

## The three macros

```rust
#[js_export]                 // mark a struct as a JS-exposable class
#[derive(Debug)]
struct Point { x: i32, y: i32 }

#[js_class(rename = "Point2D")]   // impl block -> JS class methods (rename optional)
impl Point {
    #[js_method(constructor)]      // mark methods to expose; see classes.md for getter/setter/static
    fn new(x: i32, y: i32) -> Self { Self { x, y } }

    #[js_method]
    fn distance(&self) -> f64 { ((self.x.pow(2) + self.y.pow(2)) as f64).sqrt() }
}
```

## Registering in `init`

```rust
use rong::*;

pub fn init(ctx: &JSContext) -> JSResult<()> {
    // A class with a global constructor (JS: `new Point2D(...)`)
    ctx.register_class::<Point>()?;

    // A free function on the host `Rong` namespace (JS: `Rong.cwd()`)
    let rong = ctx.host_namespace();
    let cwd_fn = JSFunc::new(ctx, cwd)?.name("cwd")?;
    rong.set("cwd", cwd_fn)?;

    Ok(())
}

fn cwd() -> JSResult<String> {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| HostError::new(rong::error::E_IO, e.to_string()).into())
}
```

Use `register_hidden_class::<T>()` for Rust-owned interop types that need a
prototype but should **not** get a global JS constructor (create instances from
Rust with `Class::lookup::<T>(ctx)?.instance(value)`).

## Errors (summary - see errors.md)

Build host errors with `HostError::new(CODE, msg)`, optionally `.with_name("TypeError")`
and `.with_data(rong::err_data!({ ... }))`, then `.into()` a `RongJSError`. To
re-throw a value that came from JS (callback/promise/abort reason), preserve it
with `RongJSError::from_thrown_value(value)` rather than wrapping it. Detect
exceptions with `value.is_exception()` - **not** `is_error()`.

## Type conversion (summary - see type-conversion.md)

Rust <-> JS conversion is automatic for primitives (`bool`, integer types, `f64`,
`String`/`&str`), `Option<T>` (<-> `null`), `Vec<T>` (<-> Array), `SystemTime` (<->
Date), and the wrapper types (`JSValue`, `JSObject`, `JSArray`, `JSFunc`,
`JSDate`, `JSSymbol`, `Promise`). For object-shaped params/returns, derive
`FromJSObj` / `IntoJSObj` with `#[rename = "jsName"]`. Use `Optional<T>` (from
`rong::function`) for optional arguments.

For public API object shapes, prefer named Rust structs with `FromJSObj` /
`IntoJSObj` over hand-parsed or hand-built `JSObject`. The type generator reads
those structs and emits the matching TypeScript interfaces; raw `JSObject`
usually forces a `ts_args`/`ts_return` hatch or a manual type fragment.

## Working checklist

1. Decide functions vs classes (or both).
2. Write the Rust with the macros; return `JSResult<T>` for anything fallible.
3. Model public option/result objects as `FromJSObj` / `IntoJSObj` structs so `rong_typegen` can emit TypeScript.
4. Implement `pub fn init(ctx: &JSContext) -> JSResult<()>` and register everything.
5. For a new module crate, wire it into `rong_modules` behind a feature flag (see module-structure.md).
6. Add a `#[cfg(test)]` test using `rong_test` (and a JS unit script via `UnitJSRunner` if useful).
7. Regenerate and check types when the JS surface changes: `cargo run -p rong_typegen` and `cargo run -p rong_typegen -- --check`.
8. Verify against an engine: `cargo test -p rong_<name> --features quickjs` (and `jscore`).

Match the conventions of existing modules under `modules/` - read a neighbor
(`rong_url`, `rong_fs`, `rong_http`) before adding new patterns.
