# rong_test_device

Bridges standard Rust integration tests (`tests/*.rs`) to run on HarmonyOS
devices **without rewriting test content**.

## Why this crate exists

Rong's integration tests live in `tests/*.rs` and run via `cargo test` on the
host machine (JSCore tests run directly on macOS since iOS/macOS share the same
runtime). HarmonyOS is different: unlike Android where you can `adb push` a
binary and execute it, HarmonyOS enforces strict permission policies —
arbitrary ELF execution is not allowed and `fork()` is unavailable. Tests must
be compiled into a `.so`, loaded by a native app, and executed on-device.

This crate bridges that gap: the same test functions that `cargo test` runs on
desktop are automatically compiled and registered for on-device execution, with
no duplication.

## How it works

`build.rs` reads each `tests/*.rs` file at compile time and performs minimal
source transformation:

1. **Strips** `#[test]` / `#[tokio::test]` attributes (they're cargo-test
   specific).
2. **Replaces** `use rong_test::*` with `use crate::prelude::*` (binds to the
   JSVM engine instead of the desktop default).
3. **Makes** test functions `pub` so they're callable from the runner.
4. **Generates** a registry mapping `"file.function_name"` → function pointer.

Files that don't import `rong_test` (e.g. `rong.rs` which tests the high-level
`Rong` worker API) are automatically skipped.

## Adding a new test

Write a standard integration test in `tests/new_feature.rs`:

```rust
use rong_test::*;

#[test]
fn new_feature_works() {
    run(|ctx| {
        let result: String = ctx.eval(Source::from_bytes("'hello'"))?;
        assert_eq!(result, "hello");
        Ok(())
    })
}
```

Next `cargo build -p rong_test_device` (or `./testing/harmony/dev.sh test`) automatically
includes it in the on-device test suite. Zero extra work.
