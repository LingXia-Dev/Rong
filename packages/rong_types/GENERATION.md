# Type generation

`@rongjs/rong` types come from two sources, by design.

## Generated (canonical from Rust)

For modules whose JS surface is expressed by `#[js_class]`,
`#[derive(FromJSObject|IntoJSObject)]`, and `js_api!`,
`src/<module>.ts` **is** the output of `rong-typegen` (in the `rong` repo). Do not
edit these by hand — change the Rust source and regenerate:

```
cargo run -p rong_typegen              # regenerate canonical outputs
cargo run -p rong_typegen -- --check   # CI drift-guard
```

Currently generated-canonical: **url, storage, sqlite, s3, fs** — modules whose
public surface is fully expressed by the Rust binding declarations and matched
by the previous hand-authored types.

`typegen.json` is the source of truth for output policy. A module declared as
`canonical` is written and checked in `src/<module>.ts`; a module declared as
`curated` remains hand-authored because its public TypeScript contract cannot
yet be expressed without losing fidelity. The check also
fails for configured modules that disappear, newly discovered modules missing
from the policy, and stale managed output files. The generated marker is only a
content marker — it never decides whether a published file is canonical.

Source discovery starts at each crate's `src/lib.rs` and follows Rust `mod`
declarations, so unreferenced `.rs` files are not part of the generated API.
`#[cfg(test)]` modules/items are excluded. Other target `cfg`s are intentionally
treated as a target-agnostic union because the npm type package covers every
supported host platform. Parse, read, unresolved-module, and orphan
`#[js_method]` errors are fatal rather than warnings.

TypeScript-only aliases with no one-to-one backing Rust type (such as
`SQLiteParam`) are declared as `type Name = "...";` entries inside `js_api!`,
beside the runtime bindings that consume them. They are emitted as part of the
canonical generated file rather than copied from a package prelude.
Method precision that the Rust type is too coarse for uses co-located
`#[js_method(ts_return = "…", ts_params = "…")]` hatches.
Free functions and namespace values use the equivalent options in a shared
`js_api!` declaration. That declaration generates runtime registration
and the mergeable `RongNamespace` TypeScript augmentation from one source.

## Independent downstream runtimes

`rong-typegen` is published with Rong so a downstream runtime can generate its
own type package without depending on `@rongjs/rong`. Paths are relative to the
config file:

```json
{
  "source": "../../crates/lingxia-logic/src",
  "name": "logic",
  "out": "src/generated/logic.ts",
  "preludes": ["src/logic-prelude.ts"],
  "global_objects": { "lx": "Lx" },
  "profiles": {
    "logic-web": "src/logic-globals.d.ts"
  }
}
```

Run `rong-typegen --config path/to/typegen.json`, adding `--check` in CI. The
CLI version should match the `rong` version used by the downstream runtime, so
upgrading Rong also upgrades the extractor and its runtime profiles. The
versioned `logic-web` profile is an ambient declaration set for the standard
globals Rong actually installs in a Logic runtime (`fetch`, Request/Response,
URL, encoding, events, abort, buffers, streams, timers, and console). It does
not include browser-only DOM APIs and creates no npm dependency on Rong's own
type package.

Downstream host APIs use the same declaration macro with their own target and
exact TypeScript interface name, for example:

```rust
rong::js_api! {
    fn register_logic(ctx) {
        namespace Lx = lx::namespace(ctx);
        fn getDeviceInfo = device::get_device_info;
    }
}
```

## Hand-authored (intentionally not generated)

Two categories stay authored because generation would lose fidelity:

- **Web-standard globals** (`abort`, `worker`, `buffer`, `event`, `console`,
  `stream`, `encoding`, `exception`): these are DOM/WHATWG types that `lib.dom`
  already declares authoritatively. The hand-written files either defer to
  `lib.dom` entirely (`stream`, `encoding`) or add DOM shapes the generator
  can't produce — `extends EventTarget`/`extends Error`, event-handler
  properties, static methods, string-literal unions (`DOMExceptionName`), and
  `interface` over `declare class` to avoid DOM clashes. Generating them would
  emit a *weaker* type that shadows the standard one, so they stay authored.
- **Curated APIs beyond the raw classes** (`http`, `redis`, `command`): the
  public surface is deliberately shaped and exceeds what the `#[js_class]`
  registrations express — e.g. `http` follows the fetch spec, `redis`
  `RedisSubscription extends AsyncIterableIterator<RedisMessage>` (so `for await`
  type-checks) and returns a recursive `RedisReply` union, and `command` exposes
  a `Rong.spawn(...)` free-function API with rich option/result types. Generating
  these would emit raw classes (`ChildProcess`, `RedisSubscription` without the
  iterator protocol) and lose that precision.
- **Not-yet-migrated free functions** (`timer`, `cron`, `assert`,
  `compression`, `sse`, `error`): these still use manual registration and stay
  curated until they move to `js_api!`. FS is the first complete module:
  its classes, object shapes, constants, and all `Rong.*` functions are emitted
  into `src/fs.ts`; `global.ts` only provides the shared namespace bootstrap.

Curated modules do not produce committed shadow declarations. A generated
snapshot that is not compared structurally with the published file is not a
drift guard and must not be treated as one.
