# Rong JavaScriptCore Backend

This crate provides the JavaScriptCore (JSC) backend for RongJS.

- Crate: `rong_jscore`
- Purpose: Integrates WebKit's JavaScriptCore engine with RongJS
- Usage: Enable the `jscore` feature on `rong`
- Backend: macOS and iOS use the system `JavaScriptCore.framework` by default;
  all other targets link a source-built WebKit/JSCOnly artifact. Force the
  source backend on macOS/iOS too with `jscore-source` on `rong` (or `source`
  on this crate).
- Interruption: source/JSCOnly builds always enable execution preemption;
  system-framework builds opt in with `jscore-interrupt` on `rong` (or
  `interrupt-spi` on this crate).
- Source artifact: downloaded and cached from the pinned artifact manifest, or
  supplied via `RONG_JSC_ROOT`.
  See [`sys/README.md`](sys/README.md) for the full setup, including bytecode
  support.

## License

Licensed under either of:
- MIT License (see `../LICENSE-MIT`)
- Apache License 2.0 (see `../LICENSE-APACHE`)
