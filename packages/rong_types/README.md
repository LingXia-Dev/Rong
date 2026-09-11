# @rongjs/rong

TypeScript type definitions for the Rong JavaScript runtime (globals injected by Rust).

## Scope

- Provides `.d.ts`/TS sources for the runtime surface so editors/TS can typecheck Rong scripts.
- Not a runtime polyfill: it does not provide implementations, only types.

## Runtime Export Map (high level)

- `Rong` namespace: file system, storage, runtime metadata, command APIs, timer helpers, compression helpers, and host constructors such as `RedisClient`, `S3Client`, `SQLite`, and `SSE`
- Globals added by Rong modules include `fetch`, `assert`, `atob`, `btoa`, `Worker`, `setTimeout`, `clearTimeout`, `setInterval`, and `clearInterval`
- Additional Web-standard globals such as `Request`, `Response`, `Headers`, `FormData`, `URL`, `ReadableStream`, `WritableStream`, `Blob`, `File`, `AbortController`, `crypto`, and `DOMException` are also provided when the corresponding runtime modules are enabled

The type package relies on TypeScript’s DOM libs for shared Web API base types.

## Installation

```bash
npm install @rongjs/rong
```

## Usage (typechecking only)

Add to your `tsconfig.json`:

```json
{
  "compilerOptions": {
    "types": ["@rongjs/rong"]
  }
}
```

Notes:

- This enables global typings; you should not `import` runtime modules like `'http'` (those are Rong globals, not Node modules).
- Ensure your `tsconfig.json` `lib` includes `"DOM"` if you want DOM globals (e.g. `URL`, `ReadableStream`) to be typed.
- `Worker` uses the DOM global type name. The package exports `RongWorker`/`RongWorkerMessageEvent`/`RongWorkerErrorEvent` for the precise Rong subset when you want stricter annotations.
- Rong’s runtime `Storage` constructor intentionally is not redeclared globally in the type package, because the DOM lib already owns the global `Storage` name. Use the exported `Storage`/`StorageConstructor` types as local annotations when needed.
- The package only supports the root export `@rongjs/rong`; `src/*` and `dist/*` are not public import paths.

## Accuracy notes (common gotchas)

- Storage is not `localStorage`-compatible. The standard runtime exposes `new Storage(path, options?)`; embedders may also inject a preconfigured `storage` instance, but that is not part of the default runtime surface.
- Directory listing is `Rong.readDir(...)` (async iterator), not `Rong.readdir(...)`.
- HTTP is the global `fetch(...)` API. There is no `http` namespace and no `download` JS API.

## Development

```bash
# Install dependencies
npm install

# Build
npm run build

# Watch mode
npm run watch
```
