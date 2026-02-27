# Rong JavaScriptCore Backend

This crate provides the JavaScriptCore (JSC) backend for RongJS.

- Crate: `rong_jscore`
- Purpose: Integrates WebKit's JavaScriptCore engine with RongJS
- Usage: Enable the `jscore` feature on `rong`

## Providers

- Default provider: system JavaScriptCore on Apple targets (`rong_jscore_sys`)
- Optional provider: source-built WebKit/JSC via `provider-webkit`

To use the WebKit provider from the workspace root, enable `jscore-provider-webkit`
and set:

- `RONG_JSC_WEBKIT_ROOT` (preferred)

Optional overrides:

- `RONG_JSC_WEBKIT_INCLUDE_DIR`
- `RONG_JSC_WEBKIT_LIB_DIR`
- `RONG_JSC_WEBKIT_LINK_KIND` (`dylib`, `static`, `framework`)

## License

Licensed under either of:
- MIT License (see `../LICENSE-MIT`)
- Apache License 2.0 (see `../LICENSE-APACHE`)
