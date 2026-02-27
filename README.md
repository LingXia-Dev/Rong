# Rong (融) - Multi-Engine JavaScript Runtime for Rust

[![Rust](https://img.shields.io/badge/rust-1.90+-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-APACHE)

**Rong** (融, meaning "fusion" in Chinese) is a specialized JavaScript runtime for Rust designed for **LingXia App** and **microservices** in constrained environments. It provides a lightweight, secure JavaScript execution environment optimized for specific use cases rather than general-purpose Node.js replacement.

## 🌟 Why "Rong" (融)?

The name "Rong" (融) embodies the core philosophy of this project:

- **Fusion**: Seamlessly merges JavaScript engines with Rust native code
- **Harmony**: Creates harmonious integration between different runtime environments
- **Unity**: Unifies multiple JavaScript engines under a single, elegant API
- **Flow**: Enables smooth data flow between JavaScript and Rust worlds

In Chinese culture, "融" represents natural harmony and coexistence - perfectly capturing how Rong brings together diverse technologies into a unified whole.

## Features

### Multi-Engine Support
- **QuickJS** - Lightweight and fast
- **JavaScriptCore** - WebKit's production-ready engine
- **ArkJS** - HarmonyOS JavaScript engine 🚧 **In Development**

### Developer Experience
- **Zero-cost abstractions** - Minimal runtime overhead
- **Type-safe bindings** - Rust's type system ensures memory safety
- **Async/await support** - Full Promise and async iterator integration
- **Rich module ecosystem** - Built-in modules for common tasks

### Architecture
- **Unified API** - Same code works across all engines
- **Memory efficient** - Careful resource management
- **Thread-safe** - Safe concurrent access patterns
- **Extensible** - Easy to add custom modules and bindings

```
┌─────────────────────────────────────────────────────────────┐
│                        Rong Core                            │
├─────────────────────────────────────────────────────────────┤
│ Unified API │ Type System │ Memory Management │ Async/Await │
├─────────────────────────────────────────────────────────────┤
│    QuickJS   │      JavaScriptCore     │      ArkJS         │
├─────────────────────────────────────────────────────────────┤
│              Built-in Modules & Extensions                  │
│   Timer │ HTTP │ FS │ Console │  Path │ URL │ ...           │
└─────────────────────────────────────────────────────────────┘
```


## 🚀 Quick Start

```rust
use rong::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a JavaScript runtime and context
    let rt = RongJS::runtime();
    let ctx = rt.context();

    // Execute JavaScript code
    let result: i32 = ctx.eval(Source::from_bytes(b"2 + 3"))?;
    println!("Result: {}", result); // Output: Result: 5

    Ok(())
}
```

For more examples including class bindings, async functions, and advanced features, see the [Module Development Guide](docs/module_development.md).

### Engine Selection

QuickJS is the default engine:

```bash
cargo run -p rong_cli
```

Switch to JavaScriptCore explicitly:

```bash
cargo run -p rong_cli --no-default-features --features jscore,tls-aws-lc
```

Use JavaScriptCore with source-built WebKit provider (for non-Apple targets):

```bash
RONG_JSC_WEBKIT_ROOT=/path/to/webkit-or-bun-build \
cargo run -p rong_cli --no-default-features --features jscore-provider-webkit,tls-aws-lc
```

With repository submodule workflow:

```bash
./scripts/webkit_submodule.sh init
./scripts/build_webkit_provider.sh --release
./scripts/check_jscore_webkit.sh
bash test.sh -e jscore-provider-webkit -c
```

One-shot end-to-end workflow:

```bash
./scripts/e2e_webkit_provider.sh
./scripts/parity_jscore_provider.sh
```

If auto-detection is not enough, override with:
- `RONG_JSC_WEBKIT_INCLUDE_DIR`
- `RONG_JSC_WEBKIT_LIB_DIR`
- `RONG_JSC_WEBKIT_LINK_KIND` (`dylib`/`static`/`framework`)

`quickjs` and `jscore` are mutually exclusive. If both are enabled, build fails fast.
For TLS backend selection, use `tls-aws-lc` (default) or `tls-ring`.

## 📦 Built-in Modules

Rong comes with a rich set of built-in modules:

- **rong_timer** - setTimeout, setInterval, async timers
- **rong_http** - HTTP client/server, fetch API
- **rong_fs** - File system operations
- **rong_console** - Console logging and debugging
- **rong_url** - URL parsing and manipulation
- **rong_buffer** - Binary data handling
- **rong_event** - Event emitter and handling
- **rong_abort** - AbortController and signals
- **rong_encoding** - Text encoding/decoding
- **rong_assert** - Assertion utilities
- **rong_storage** - Storage APIs
- **rong_stream** - Stream APIs
- **rong_path** - Path manipulation
- **rong_process** - Process information and environment
- **rong_child_process** - Child process management
- **rong_exception** - Exception handling
- **rong_navigator** - Navigator APIs

## 📚 Documentation

Comprehensive guides for working with Rong:

- **[Module Development Guide](docs/module_development.md)** - Learn how to create Rust-driven JavaScript APIs and classes
- **[Value System Guide](docs/value_system.md)** - Understand type conversion between Rust and JavaScript
- **[Error Handling Guide](docs/error_handling.md)** - Best practices for error handling
- **[Testing Guide](docs/testing.md)** - How to run and write tests
- **[Engine & Provider Strategy](docs/engine_provider_strategy.md)** - Roadmap for multi-engine support, JSC providers, and Harmony validation

## 📄 License

This project is licensed under either the MIT License or the Apache License 2.0, at your option.
See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE) for details.

## 🙏 Acknowledgments

- QuickJS team for the lightweight JavaScript engine
- WebKit team for JavaScriptCore
- HarmonyOS team for ArkJS
- Rust community for excellent async ecosystem

---

**Rong (融)** - *Fusing JavaScript engines with Rust, creating harmony in diversity.*
