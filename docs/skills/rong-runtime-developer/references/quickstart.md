# Rong Runtime Quick Start

Rong is a JavaScript runtime for Rust with a unified API over QuickJS,
JavaScriptCore, and JSVM. For JavaScript script authors, the practical entry
point is `rong_cli`.

## Run a Script

```bash
cargo run -p rong_cli -- path/to/script.js arg1 arg2
```

Arguments after the script name are available as:

```javascript
console.log(Rong.argv); // full argv
console.log(Rong.args); // user args
```

## Compile and Run Bytecode

```bash
cargo run -p rong_cli -- compile path/to/script.js path/to/script.rong
cargo run -p rong_cli -- path/to/script.rong arg1 arg2
```

## Engine Selection

Desktop CLI defaults to QuickJS with `tls-aws-lc`:

```bash
cargo run -p rong_cli
```

Use JavaScriptCore explicitly:

```bash
cargo run -p rong_cli --no-default-features --features jscore,tls-aws-lc
```

For source-built JavaScriptCore artifacts, use the repository's pinned artifact
setup from `javascriptcore/sys/webkit-artifacts.tsv`. A normal user script
should not need to build WebKit.

## API Style

- Web-standard APIs are global where possible: `fetch`, `URL`,
  `AbortController`, streams, workers, Blob/File, encoding, events, and console.
- Rong-specific host APIs are on `Rong`: file I/O, command execution,
  environment and args, compression, sleep, Redis, S3, and similar APIs.
- Prefer async I/O and streaming for large data.
