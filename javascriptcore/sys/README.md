# rong_jscore_sys

Low-level JavaScriptCore FFI bindings for RongJS.

This crate exposes the raw JavaScriptCore bindings used by the RongJS backend.
Most users should depend on `rong` or `rong_jscore` instead of using this crate
directly.

## Backends

On **macOS and iOS**, the default backend links the system `JavaScriptCore.framework`.
On every other target — and on macOS/iOS when the `source` feature is enabled —
the crate links a source-built WebKit/JSCOnly artifact. (Other Apple targets such
as tvOS/watchOS have no system JSC here, so they use the source backend too.)

### How `build.rs` finds the source artifact

For a source build, `build.rs` resolves the artifact in this order:

1. **`RONG_JSC_ROOT`** — the single manual override (see *Manual configuration*).
2. **Per-target cache** — `~/.cache/rong/webkit/<target>/` (overridable with
   `RONG_JSC_CACHE_DIR`), if already populated.
3. **Download a prebuilt artifact** — the common, cross-platform path once
   artifacts are published. If a row for the target is pinned in
   `webkit-artifacts.tsv`, the build downloads it (a quick `curl` + `tar`,
   sha256-verified) into the cache. Disable with `RONG_JSC_DOWNLOAD=0`; point at
   a mirror with `RONG_JSC_ARTIFACT_BASE_URL`.

So once a release publishes prebuilt artifacts, a normal `cargo build` needs no
env var on any supported platform. Until then (or offline), either build an
artifact yourself (see *Building an artifact*) or point `RONG_JSC_ROOT` at one.

### Manual configuration

Point the crate at an existing artifact with a single env var:

```sh
RONG_JSC_ROOT=/path/to/jsc-artifact
```

Target-specific variants are also accepted, for example
`RONG_JSC_ROOT_AARCH64_UNKNOWN_LINUX_GNU` (the suffix is the target triple
upper-cased with `-`/`.` replaced by `_`).

`RONG_JSC_ROOT` may point at an install-style tree with `include/` and `lib/`,
or directly at a WebKit build tree such as `WebKitBuild/JSCOnly/Release`;
`build.rs` probes the usual subdirectories. The include directory must contain
`JavaScriptCore/JavaScript.h`.

When the artifact is a `JavaScriptCore.framework` produced on Apple, the crate
links it as a framework and stamps the framework install name to the extracted
artifact path, so downstream binaries do not need to add their own rpath.
Otherwise it links the static `JavaScriptCore`, `WTF`, and `bmalloc` archives
plus the usual ICU/C++ runtime libraries for the target OS.

## Building an artifact

There is no system JavaScriptCore off Apple, so non-Apple targets (and Apple
with the `source` feature) need a JSCOnly artifact built from upstream WebKit:
`https://github.com/WebKit/WebKit`.

The repo ships a helper that does this end to end:

```sh
# Shallow-clones upstream WebKit, builds JSCOnly Release, and installs
# include/ + lib/ into the per-target cache that build.rs auto-detects.
./scripts/build_jsc_artifact.sh
```

By default it installs into `~/.cache/rong/webkit/<target>/{include,lib}`, so a
later `RONG_JSC_SOURCE=1 cargo build` (or `./test.sh -e jscore`) needs no env
var. Useful flags:

- `--check` - validate the host tools/environment before cloning WebKit.

- `--out <dir>` — install elsewhere; then set `RONG_JSC_ROOT=<dir>`.
- `--package <file.tar.gz>` — also emit a distributable tarball (this is what CI
  uploads to GitHub releases and pins in `webkit-artifacts.tsv`).
- `--webkit-ref <tag|branch|sha>` — pin the WebKit revision (default: a known-good tag).

Windows/MSVC source artifacts use clang-cl, vcpkg static ICU, and the same
released-artifact smoke path as the other prebuilt targets.

To build by hand instead, use WebKit's own tooling and point `RONG_JSC_ROOT` at
the build tree:

```sh
git clone --depth=1 https://github.com/WebKit/WebKit
cd WebKit && Tools/Scripts/build-jsc --jsc-only --release
RONG_JSC_ROOT="$PWD/WebKitBuild/JSCOnly/Release" cargo build ...
```

The helper validates the installed artifact and runs a bytecode smoke test for
native builds. The build fails if required headers, libraries, or bytecode
private headers are missing.

## Updating prebuilt artifacts

Normal source builds should use prebuilt source artifacts. They should not
compile WebKit in regular CI or on developer machines unless someone is
intentionally producing a new artifact. The update flow is:

1. Pick and pin a WebKit revision.
   Use a tag or exact commit SHA, not `main`, for artifacts that will be
   published and consumed by others.

2. Run the manual GitHub workflow:

   ```text
   .github/workflows/build-jsc-artifacts.yml
   ```

   Inputs:

   - `webkit_ref`: the pinned WebKit tag/branch/SHA to build.
   - `release_tag`: the GitHub Release tag that will hold the tarballs.
   - `target`: `all`, or one supported target such as
     `aarch64-apple-darwin`.

   The workflow builds the supported macOS Intel, macOS arm64, Linux, and Windows targets, uploads
   `rong-webkit-<target>.tar.gz` assets to the release, and emits TSV rows in
   its summary and `webkit-artifacts-additions` artifact.

3. Review the emitted rows and paste them into
   `javascriptcore/sys/webkit-artifacts.tsv`.

   Each row is:

   ```text
   <target-triple> <release-tag> <filename> <sha256>
   ```

   The checksum is what `build.rs` verifies before extracting the artifact into
   the per-target cache.

4. Run the normal `CI` workflow.

   Once rows exist, the corresponding `jscore-source-test` jobs stop skipping.
   They exercise the production path: `build.rs` downloads the pinned prebuilt
   artifact, verifies the SHA-256, extracts it into `RONG_JSC_CACHE_DIR`, and
   runs `ENGINE=jscore RONG_JSC_SOURCE=1 RONG_JSC_REQUIRE_BYTECODE=1 cargo make
   ci-verify`.

5. Merge only after both sides pass:

   - `build-jsc-artifacts.yml` produced valid release assets and TSV rows.
   - `ci.yml` consumed those rows and passed `jscore-source-test` for every
     pinned target.

If an artifact must be disabled temporarily, remove or comment out its row in
`webkit-artifacts.tsv`. The corresponding CI job will explicitly skip and tell
maintainers to rebuild/pin an artifact.

## Bytecode

The source backend can compile and run JSC bytecode
(`JSContext::compile_to_bytecode` / `run_bytecode`). That goes through a small
C++ bridge which calls JSC internals and therefore needs the JSC **private
headers**. A bytecode-capable artifact must include
`<artifact>/include/JavaScriptCore/private/`,
`<artifact>/include/WTF/`, and `<artifact>/include/bmalloc/`.

If the artifact lacks those private headers, `compile_to_bytecode` reports
bytecode as unsupported. Release artifact builds set `RONG_JSC_REQUIRE_BYTECODE=1`,
which turns that condition into a build failure and keeps published source
artifacts bytecode-capable.

The system `JavaScriptCore.framework` backend (the macOS/iOS default, without the
`source` feature) never supports bytecode — the public C API exposes no
bytecode (de)serialization.
