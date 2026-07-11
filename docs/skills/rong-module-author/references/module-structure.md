# Creating and wiring a new module crate

A Rong module is a crate `modules/rong_<name>` exposing
`pub fn init(ctx: &JSContext) -> JSResult<()>`. To ship it in the runtime you
register it in `rong_modules` behind a feature flag.

## 1. Crate layout

```
modules/rong_<name>/
|-- Cargo.toml
|-- README.md
`-- src/
    |-- lib.rs        # pub fn init(ctx) + module wiring
    `-- <feature>.rs  # your classes/functions
```

## 2. `Cargo.toml` - forward engine features to `rong`

Every module forwards the JS-engine and TLS features to the main `rong` crate so
the engine choice flows through the whole graph:

```toml
[package]
name = "rong_<name>"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
homepage.workspace = true
rust-version.workspace = true
readme = "README.md"

[dependencies]
rong = { workspace = true }
# ...your own deps...

[features]
default = []
quickjs    = ["rong/quickjs"]
jscore     = ["rong/jscore"]
arkjs      = ["rong/arkjs"]
tls-aws-lc = ["rong/tls-aws-lc"]
tls-ring   = ["rong/tls-ring"]

[dev-dependencies]
rong_test    = { path = "../../testing/rong_test" }
rong_assert  = { workspace = true }
rong_console = { workspace = true }
```

## 3. `src/lib.rs` - the `init` entry point

```rust
//! # <Name> Module
mod thing;
pub use thing::Thing;

use rong::*;

pub fn init(ctx: &JSContext) -> JSResult<()> {
    ctx.register_class::<Thing>()?;
    // ...or register functions on ctx.host_namespace()...
    Ok(())
}
```

## 4. Wire it into the workspace and `rong_modules`

1. **Workspace** (`/Cargo.toml`):
   - add `"modules/rong_<name>"` to `[workspace] members`;
   - add to `[workspace.dependencies]`:
     `rong_<name> = { path = "modules/rong_<name>", default-features = false, version = "<crate version>" }`.

   Published crates use independent versions, so choose the module's own crate
   version rather than assuming a workspace-wide version.

2. **`rong_modules/Cargo.toml`**:
   - dependency: `rong_<name> = { workspace = true, optional = true }`
   - per-module feature: `<name> = ["rong_<name>"]`
   - add `"<name>"` to the `all = [ ... ]` feature list
   - add engine passthrough to each engine feature: `"rong_<name>?/quickjs"`, `"rong_<name>?/jscore"`, `"rong_<name>?/arkjs"`

3. **`rong_modules/src/lib.rs`** - call your `init` under the feature gate:

```rust
#[cfg(feature = "<name>")]
rong_<name>::init(ctx)?;
```

4. **`packages/rong_types/typegen.json`** - if the crate contains `js_api!`,
   `#[js_class]`, `#[js_union]`, `FromJSObject`/`IntoJSObject`, or
   `#[js_numeric_enum]` items, classify its generated output explicitly:
   - use `canonical` when the generated declaration is the published
     `src/<name>.ts`;
   - use `curated` when `src/<name>.ts` remains hand-authored because generation
     would lose public TypeScript fidelity.

   Typegen fails for an unclassified JS-facing module, so this decision cannot
   be skipped accidentally.

## 5. Tests

Modules test against an engine via `rong_test`. Sync tests use `run(|ctx| ...)`;
async tests use `async_run!(|ctx: JSContext| async move { ... })`. A common
pattern runs a JS unit script through `UnitJSRunner`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rong_test::*;

    #[test]
    fn test_thing() {
        async_run!(|ctx: JSContext| async move {
            init(&ctx)?;
            rong_assert::init(&ctx)?;
            rong_console::init(&ctx)?;

            let passed = UnitJSRunner::load_script(&ctx, "thing.js")
                .await?
                .run()
                .await?;
            assert!(passed);
            Ok(())
        });
    }
}
```

Run them against a concrete engine (one engine at a time, no default features):

```bash
cargo test -p rong_<name> --no-default-features --features quickjs
cargo test -p rong_<name> --no-default-features --features jscore
```

The engines (`quickjs`, `jscore`, `arkjs`) are mutually exclusive - always pass
exactly one with `--no-default-features`.

## Conventions

Read a neighbouring module before inventing patterns - `rong_url` (small,
classes), `rong_fs` (functions + async I/O), `rong_http` (union-type params,
larger surface). Match their error codes, naming, and test style.
