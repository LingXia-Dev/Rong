# Runtime and host setup

Use this reference when configuring a Rust binary that embeds Rong directly.

## Select one engine

Disable default features and select exactly one engine in the host crate:

```toml
[dependencies]
rong = { version = "0.5", default-features = false, features = ["quickjs"] }
```

Engine features:

- `quickjs`: bundled QuickJS engine with native hard interruption.
- `jscore`: Apple system JavaScriptCore on macOS/iOS; other supported targets
  resolve a source/JSCOnly backend.
- `jscore-source`: force the source/JSCOnly backend, including mandatory hard
  interruption support.
- `jscore-interrupt`: opt into the private execution-time-limit SPI when using
  Apple's system framework.
- `arkjs`: HarmonyOS/OpenHarmony ArkJS; interruption is cooperative-only.

Select `tls-aws-lc` for the usual desktop host or `tls-ring` for HarmonyOS when
network-capable Rong services are compiled. Do not enable multiple engine or TLS
backends accidentally through transitive feature sets.

## Create a raw runtime

Use a raw runtime when all JavaScript work belongs to the current thread:

```rust
use rong::{JSEngine, RongJS, Source};

let runtime = RongJS::runtime();
let ctx = runtime.context();
let value: i32 = ctx.eval(Source::from_bytes("20 + 22"))?;
```

Keep the runtime and every context created from it on the owning thread. Share
only explicitly thread-safe handles such as `InterruptHandle` across threads.
Use a worker pool instead of wrapping runtime/context values in ad-hoc locking.

## Install an explicit module surface

Compile the required `rong_modules` features with the same engine family as the
host, then initialize the smallest dependency-closed set:

```toml
[dependencies]
rong_modules = { version = "0.5", default-features = false, features = [
  "quickjs",
  "console",
  "timer",
] }
```

For a JSC source build, select `jscore-source` on `rong` and `jscore` on
`rong_modules`; source selection and `jscore-interrupt` are engine-level
features on `rong`. Forward the same TLS backend through `rong_modules` when
network-capable modules are enabled.

```rust
rong_modules::init(&ctx, ["console", "timer"])?;
```

Use `rong_modules::resolve_modules` to inspect the dependency closure before
initialization. Repeated registry initialization skips modules already installed
through the registry in that context. Use `init_all` only when the host policy
intentionally exposes every compiled module.

## Configure the host executor only when needed

Rong services and worker pools use the process-global `RongExecutor`, creating a
default executor on first use. Install a custom executor before that first use
only when the host needs specific thread counts or names:

```rust
let executor = rong::RongExecutor::builder()
    .threads(2)
    .thread_name("my-rong-host")
    .build()?;
executor.install_global()?;
```

Do not call blocking bridge APIs from inside an active Tokio runtime or Rong
worker thread. Prefer `.await` APIs in asynchronous hosts.

## Preserve lifecycle ownership

- Drop contexts before their runtime when controlling teardown manually.
- Attach context-owned host work through context services/tasks so shutdown can
  cancel and drain it.
- Keep capability and module policy in the host; JavaScript should not be able
  to widen its own surface.
- Use worker pools for long-lived concurrent execution and explicit shutdown.

## Verify host configurations

Run a no-engine library check and the selected engine's focused tests. For
target-specific engines, compile or test on that target rather than inferring
support from a desktop build.
