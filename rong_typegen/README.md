# rong-typegen

`rong-typegen` generates TypeScript declarations from Rong binding metadata in
Rust source. It supports Rong's own type package and independent downstream
runtimes; generated downstream types do not import `@rongjs/rong`.

Use the same `rong-typegen` version as the downstream runtime's `rong` version:

```sh
cargo install rong_typegen --version "$RONG_VERSION" --locked
rong-typegen --config packages/types/typegen.json
rong-typegen --config packages/types/typegen.json --check
```

Paths in the JSON configuration are relative to the configuration file:

```json
{
  "source": "../../crates/logic/src",
  "name": "logic",
  "out": "src/generated/logic.ts",
  "preludes": ["src/logic-prelude.ts"],
  "global_objects": { "lx": "Lx" },
  "profiles": { "logic-web": "src/logic-globals.d.ts" }
}
```

The `logic-web` profile supplies the DOM-free standard APIs installed by a Rong
Logic runtime, including `fetch`, URL, encoding, events, abort, buffers,
streams, timers, and console.
