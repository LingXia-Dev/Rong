# Testing

Rong is a multi-engine project. Most tests need an engine feature enabled.
By default, `rong` uses QuickJS. To switch to JavaScriptCore, always use
`--no-default-features --features jscore` to avoid enabling both engines.

JavaScript suites under `tests/unit` use `@rongjs/test`. See the
[JavaScript testing guide](../guides/javascript-testing.md) for its public API,
async execution model, matchers, reports, and embedding contract.

## Cargo tests

### Running all tests

```bash
# QuickJS (default on `rong`)
cargo test

# JavaScriptCore
cargo test --no-default-features --features jscore
```

On Apple, `jscore` uses the system `JavaScriptCore.framework`. To exercise the
source-built (JSCOnly) backend instead, use the `jscore-source` feature:

```bash
cargo test --no-default-features --features jscore-source
```

The source backend always enables JSC execution preemption. Apple
system-framework tests need `jscore-interrupt` to exercise the private-SPI path.

`./test.sh -e jscore` lets `build.rs` download a pinned source artifact or use
one configured through `RONG_JSC_ROOT`. Set `RONG_JSC_SOURCE=1` to force the
source backend in `test.sh` on Apple too. See
[`javascriptcore/sys/README.md`](../../javascriptcore/sys/README.md) for artifact setup.

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
- `rong_http` - HTTP client (fetch)
- `rong_timer` - setTimeout/setInterval
- `rong_fs` - File system operations
- `rong_console` - Console logging
- `rong_buffer` - Binary data handling
- `rong_encoding` - Text encoding/decoding
- `rong_event` - Event emitter
- `rong_abort` - AbortController
- `rong_url` - URL parsing
- `rong_stream` - Stream APIs
- `rong_command` - Shell and subprocesses
- `rong_storage` - Storage APIs
- `rong_assert` - Assertion utilities
- `rong_exception` - Exception handling
- `rong_redis` - Redis client
- `rong_sqlite` - SQLite database
- `rong_s3` - S3 object storage

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
