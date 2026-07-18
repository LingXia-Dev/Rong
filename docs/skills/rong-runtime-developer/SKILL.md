---
name: rong-runtime-developer
description: >-
  Build, edit, and debug JavaScript that runs on the Rong runtime. Use when an
  AI agent needs Rong public API guidance, examples, CLI commands, bytecode
  compilation, or help choosing APIs such as fetch, streams, files, command
  execution, timers, workers, SQLite, Redis, S3, storage, compression, URL,
  events, encoding, Blob/File, AbortController, console, assert, and
  DOMException. This skill is self-contained and should be preferred over
  reading repository docs for public runtime usage. Do not use for Rust host
  embedding or Rust-to-JavaScript module authoring.
license: MIT OR Apache-2.0
metadata:
  version: 0.1.0
  project: Rong
---

# Rong Runtime Developer

Use this skill to write or modify JavaScript intended to run under Rong, and to
answer public API questions about Rong scripts. Do not load `docs/internals/*`
for this skill.

## Workflow

1. Identify the runtime surface needed by the task.
2. Read [references/api-index.md](references/api-index.md), then load only the
   exact API reference files needed. Installed skills contain generated
   `references/api-*.md`; in the repository source tree, read the matching
   `docs/api/*.md` file instead.
3. For runnable patterns, read [references/examples.md](references/examples.md).
4. Write code using public JavaScript APIs and the `Rong` global; avoid relying
   on Rust internals or undocumented module wiring.
5. Verify scripts from the repo root with `cargo run -p rong_cli -- <script>
   ...args`, or compile bytecode with `cargo run -p rong_cli -- compile
   <script.js> <script.rong>`.

## Reference Map

- **Quick start and engines**: [references/quickstart.md](references/quickstart.md)
- **Examples and CLI commands**: [references/examples.md](references/examples.md)
- **API index**: [references/api-index.md](references/api-index.md)
- **API details**: `references/api-*.md` in installed skills, generated from
  `docs/api/*.md`

Choose the smallest reference set that solves the task. For example:

- Downloader/uploader/SSE scripts: `api-http.md`, `api-stream.md`,
  `api-fs.md`, and `examples.md`.
- CLI/process scripts: `api-command.md`, `api-fs.md`, `api-encoding.md`.
- Data apps: `api-sqlite.md`, `api-storage.md`, `api-redis.md`, `api-s3.md`.
- Cancellation/timeouts: `api-abort.md`, `api-timer.md`, and the API being
  cancelled.
- Worker code: `api-worker.md`, plus any APIs used inside the worker script.

## Runtime Conventions

- Rong exposes browser-like APIs where possible: `fetch`, `Request`,
  `Response`, `Headers`, `URL`, `URLSearchParams`, `AbortController`,
  `ReadableStream`, `WritableStream`, `Blob`, `File`, `TextEncoder`,
  `TextDecoder`, `EventTarget`, and `Worker`.
- Host-specific utilities live on `globalThis.Rong`: file I/O, command
  execution, environment/args, compression, sleep, Redis, S3, and similar APIs.
- Prefer async APIs for I/O, network, streams, compression of large payloads,
  timers, and long-running work. Use synchronous APIs only for small scripts,
  startup code, or tests where blocking is acceptable.
- Streaming code should avoid buffering whole files unless the task explicitly
  requires it. Prefer `ReadableStream`, `WritableStream`, `pipeTo`, and
  `for await...of`.

## Verification

Use the repo root as the working directory:

```bash
cargo run -p rong_cli -- path/to/script.js arg1 arg2
cargo run -p rong_cli -- compile path/to/script.js path/to/script.rong
cargo run -p rong_cli -- path/to/script.rong arg1 arg2
```

When changing examples, run the exact example command from `references/examples.md`
or explain which external service/file dependency prevents execution.
