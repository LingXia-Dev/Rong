# Testing

Rong is a multi-engine project. Most tests need an engine feature enabled.
By default, `rong` uses QuickJS. To switch to JavaScriptCore, always use
`--no-default-features --features jscore` to avoid enabling both engines.

## Cargo tests

### Running all tests

```bash
# QuickJS (default on `rong`)
cargo test

# JavaScriptCore
cargo test --no-default-features --features jscore

# JavaScriptCore with source-built WebKit provider (non-Apple targets)
RONG_JSC_WEBKIT_ROOT=/path/to/webkit-or-bun-build \
cargo test --no-default-features --features jscore-provider-webkit
```

### Testing a specific module

To test a single module, use the `-p` (package) flag:

```bash
# Test rong_http module with QuickJS
cargo test -p rong_http --features quickjs

# Test rong_timer module with JavaScriptCore
cargo test -p rong_timer --features jscore

# Test rong_fs module with QuickJS
cargo test -p rong_fs --features quickjs
```

**Available modules**:
- `rong_http` - HTTP client/server
- `rong_timer` - setTimeout/setInterval
- `rong_fs` - File system operations
- `rong_console` - Console logging
- `rong_buffer` - Binary data handling
- `rong_encoding` - Text encoding/decoding
- `rong_event` - Event emitter
- `rong_abort` - AbortController
- `rong_url` - URL parsing
- `rong_path` - Path manipulation
- `rong_stream` - Stream APIs
- `rong_process` - Process information
- `rong_child_process` - Child process management
- `rong_storage` - Storage APIs
- `rong_assert` - Assertion utilities
- `rong_exception` - Exception handling
- `rong_navigator` - Navigator APIs

### Testing multiple modules

```bash
# Test all workspace packages
cargo test --workspace

# Test specific modules
cargo test -p rong_http -p rong_timer --features quickjs
```

### Running specific test cases

```bash
# Run a specific test function in a module
cargo test -p rong_http test_fetch --features quickjs

# Run all tests matching a pattern
cargo test -p rong_timer timeout --features quickjs

# Show test output
cargo test -p rong_http --features quickjs -- --nocapture
```

## Module test runner

The repository also provides a small test runner script to execute a single module test suite
against a specific engine:

```bash
# Test rong_http with QuickJS
./test.sh -e quickjs -t rong_http

# Test rong_http with JavaScriptCore
./test.sh -e jscore -t rong_http

# Test rong_timer with QuickJS
./test.sh -e quickjs -t rong_timer
```

This script is useful for:
- Quick module testing during development
- CI/CD integration
- Testing across different engines

## WebKit provider environment

When `jscore-provider-webkit` is enabled, `javascriptcore/sys` reads:

- `RONG_JSC_WEBKIT_ROOT` (preferred; auto-detects include/lib layout)
- `RONG_JSC_WEBKIT_INCLUDE_DIR` (optional override, required if root auto-detect fails)
- `RONG_JSC_WEBKIT_LIB_DIR` (optional override, required if root auto-detect fails)
- `RONG_JSC_WEBKIT_LIB_NAME` (optional, default: `JavaScriptCore`)
- `RONG_JSC_WEBKIT_LINK_KIND` (optional, default: `dylib`; supports `dylib`, `static`, `framework`)
- `RONG_JSC_WEBKIT_EXTRA_LIBS` (optional, comma-separated)

## WebKit submodule workflow

Use the in-repo WebKit source provider:

```bash
# Init to pinned commit
./scripts/webkit_submodule.sh init

# Build JavaScriptCore provider artifacts from WebKit source
./scripts/build_webkit_provider.sh --release

# Bump to latest main (updates submodule pointer)
./scripts/webkit_submodule.sh bump
```

macOS note:
- Building WebKit provider via `build-jsc` requires full Xcode (`xcodebuild`), not only Command Line Tools.

Then run provider checks:

```bash
./scripts/check_jscore_webkit.sh

# Or run via test.sh after provider env is available
bash test.sh -e jscore-provider-webkit -c
```

One-shot flow:

```bash
./scripts/e2e_webkit_provider.sh
```

Parity smoke (same core tests on both providers):

```bash
./scripts/parity_jscore_provider.sh
```
