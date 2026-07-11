# Runtime & Bundling Contract

**Audience**: embedders, bundler/tooling authors.

## Key Decisions

- The runtime executes **one entry script** (or a `.rong` bytecode artifact).
- The runtime does **not** implement an ES module loader or Node.js CommonJS. There is no runtime contract for `import`, `import()`, or `require`.
- Native/platform APIs live under `globalThis.Rong` (and optionally Web-style globals like `fetch`, `URL`, `console`, timers).

## Runtime Contract

- `globalThis.Rong` is always present.
- The embedder decides which modules to install:
  `rong_modules::init(&ctx, names)` installs a validated, dependency-closed set
  for one context, while `rong_modules::init_all(&ctx)` explicitly installs
  everything compiled into the binary. Unknown or uncompiled names fail before
  the context is modified.
- Web-style globals (e.g. `fetch`, `URL`, `ReadableStream`, `AbortController`, `console`) are installed by their respective modules.

Module dependencies are part of the effective installed surface. Embedders can
inspect that closure with `rong_modules::resolve_modules` before creating a
context policy.

For how native modules register into the runtime, see [Module Development](./module_development.md).

## Bundling Contract (JS)

Bundles must be a plain script (IIFE / `async` IIFE) and must **not** depend on:

- Runtime `import` / `export` resolution
- Runtime code-splitting (`import()`)
- `require`, `module`, `exports`
- `node_modules` / filesystem module resolution

Bundles may rely on:

- `globalThis.Rong`
- Any globals installed by the embedder/module set

## `.rong` Bytecode (Optional)

```bash
rong compile <input.js> <output.rong>
```

- Engine-specific (QuickJS vs JavaScriptCore); portability across engines is not guaranteed.
- Prefer JS bundles for portability; use `.rong` as a startup optimization.

## FAQ

**Do we implement `require`?**
No. Tooling bundles code into a single script. If a product wants a `require`-like API, it should be a userland shim, not part of the runtime contract.
