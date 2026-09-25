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
- Garbage collection: JSC collects on its own schedule, from timers it arms
  on the VM thread's run loop. With the system framework, Rong's workers
  service that run loop (public CoreFoundation only), so JSC reclaims retired
  contexts and idle heaps. Source/JSCOnly builds run those timers on WebKit's
  own run loop, which Rong cannot drive: there, garbage that only a full
  collection frees, such as a dropped context, waits for allocation to
  trigger one. An embedder running a JSC runtime on its own thread, outside
  Rong's workers, must run that thread's CFRunLoop for the same effect.
- Source artifact: downloaded and cached from the pinned artifact manifest, or
  supplied via `RONG_JSC_ROOT`.
  See [`sys/README.md`](sys/README.md) for the full setup, including bytecode
  support.

## License

Licensed under either of:
- MIT License (see `../LICENSE-MIT`)
- Apache License 2.0 (see `../LICENSE-APACHE`)
