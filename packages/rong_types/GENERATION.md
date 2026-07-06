# Type generation

`@rongjs/rong` types come from two sources, by design.

## Generated (canonical from Rust)

For modules whose JS surface is entirely `#[js_class]` / `#[derive(FromJSObj|IntoJSObj)]`,
`src/<module>.ts` **is** the output of `rong-typegen` (in the `rong` repo). Do not
edit these by hand — change the Rust source and regenerate:

```
cargo run -p rong_typegen              # regenerate into packages/rong_types/generated
cargo run -p rong_typegen -- --check   # CI drift-guard
```

Currently generated-canonical: **url, storage, sqlite, s3, fs** — modules whose
public surface is fully expressed by `#[js_class]` and matched by the manual
types (with hatches/preludes for the coarse spots).

A `src/<module>.ts` is treated as canonical when it begins with the generated
marker comment. `rong-typegen` writes and `--check`-verifies canonical modules
in place under `src/`; every other module is written to `generated/<module>.ts`
as a reference only. So a canonical published file can never silently drift from
the Rust source — CI regenerates and diffs it.

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

`generated/*.ts` holds the generated output for **every** module (including the
hand-authored ones) and is CI-checked. For hand-authored modules it is a
drift reference: diffing it against `src/` surfaces cases where the authored
types have fallen out of step with the actual registered Rust classes (e.g. the
runtime registers `Worker`/`Response`, which authored files once called
`RongWorker`/`FetchResponse`).
