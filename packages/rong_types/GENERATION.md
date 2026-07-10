# Type generation

`@rongjs/rong` types come from two sources, by design.

## Generated (canonical from Rust)

For modules whose JS surface is entirely `#[js_class]` / `#[derive(FromJSObj|IntoJSObj)]`,
`src/<module>.ts` **is** the output of `rong-typegen` (in the `rong` repo). Do not
edit these by hand — change the Rust source and regenerate:

```
cargo run -p rong_typegen              # regenerate canonical + reference outputs
cargo run -p rong_typegen -- --check   # CI drift-guard
```

Currently generated-canonical: **url, storage, sqlite, s3, fs** — modules whose
public surface is fully expressed by `#[js_class]` and matched by the manual
types (with hatches/preludes for the coarse spots).

`typegen.json` is the source of truth for output policy. A module declared as
`canonical` is written and checked in `src/<module>.ts`; a module declared as
`reference` is written and checked in `generated/<module>.ts`. The check also
fails for configured modules that disappear, newly discovered modules missing
from the policy, and stale managed output files. The generated marker is only a
content marker — it never decides whether a published file is canonical.

Source discovery starts at each crate's `src/lib.rs` and follows Rust `mod`
declarations, so unreferenced `.rs` files are not part of the generated API.
`#[cfg(test)]` modules/items are excluded. Other target `cfg`s are intentionally
treated as a target-agnostic union because the npm type package covers every
supported host platform. Parse, read, unresolved-module, and orphan
`#[js_method]` errors are fatal rather than warnings.

Irreducible TS with no Rust origin (unions like `SQLiteParam`, result shapes) is
authored once in `preludes/<module>.ts` and prepended to the generated file.
Method precision that the Rust type is too coarse for uses co-located
`#[js_method(ts_return = "…", ts_args = "…")]` hatches.

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
- **Free-function namespace** (`global.ts`) and pure-function modules
  (`timer`, `cron`, `assert`, `compression`, `sse`, `error`): `Rong.mkdir`,
  `fetch`, `sleep`, etc. are registered manually in Rust (`ctx.global().set`),
  so there is no annotation to extract.

## The `generated/` reference

`generated/*.ts` holds CI-checked output for modules classified as `reference`
in `typegen.json`. Canonical modules live only in `src/`. For hand-authored
modules the generated file is a review aid: diffing it against `src/` surfaces
cases where the authored types have fallen out of step with the registered Rust
classes (e.g. the runtime registers `Worker`/`Response`, which authored files
once called `RongWorker`/`FetchResponse`).
