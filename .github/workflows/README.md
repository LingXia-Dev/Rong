# GitHub Workflows

## `ci.yml` (tests)

- **Trigger:** push to `master`, pull_request, and manual `workflow_dispatch`
- **Concurrency:** PR and branch runs cancel older in-progress runs for the same PR/ref, so the Actions page shows the latest relevant CI instead of stale queued attempts.
- **Scope:** a lightweight `scope` job classifies changed files. `docs/*`,
  `packages/*`, Markdown, and GitHub metadata changes do not run the Rust/JSC
  matrix. `docs/api`, `docs/skills`, `packages/rong_types`, `packages/skill`, and npm release
  script changes run only the npm package validation job. Manual
  `workflow_dispatch` runs all scopes.
- **Runs:** for Rust/source changes, `cargo fmt` runs once, then separate host
  `check`, `clippy`, and `test` jobs:
  - `quickjs` on Windows, Linux, and macOS
  - `jscore` on macOS using the system `JavaScriptCore.framework`
  - `jscore-source-*` on currently pinned macOS Intel, Linux, and Windows targets, gated by pinned prebuilt artifact rows in `javascriptcore/sys/webkit-artifacts.tsv`
- **npm packaging:** builds the Rong type package and validates `docs/skills` +
  `docs/api` can generate self-contained installable skills through
  `packages/skill/bin/pack.mjs`.
- **Source backend behavior:** `jscore-source-*` is the production-style prebuilt consumer path. It downloads and caches the pinned artifact through `rong_jscore_sys/build.rs`; if no row exists for a supported target, CI fails instead of silently skipping.
- **Steps:** `cargo fmt --check` plus `cargo make check-engine`, `cargo make clippy-engine`, and `cargo make test-engine`
- **No standalone Windows JSC workflow:** Windows source support is covered by
  `build-jsc-artifacts.yml` for producing artifacts and `CI`'s
  `jscore-source-x86_64-pc-windows-msvc` job for consuming them.

## `build-jsc-artifacts.yml` (JSC source prebuilds)

- **Trigger:** manual (`workflow_dispatch`)
- **Runs:** builds WebKit/JSCOnly artifacts for supported macOS Intel, macOS arm64, Linux, and Windows targets and uploads tarballs to one GitHub Release.
- **Outputs:** per-target release assets plus TSV rows for `javascriptcore/sys/webkit-artifacts.tsv`.
- **Purpose:** produce the prebuilt source artifacts consumed by normal `jscore-source` builds. This keeps regular CI and local builds from compiling WebKit and keeps disk usage bounded to the downloaded artifact cache.
- **Update flow:** run the workflow with one release tag and either a WebKit tag/SHA or a branch that the workflow resolves to a fixed commit, review the emitted TSV rows, paste them into `javascriptcore/sys/webkit-artifacts.tsv`, then run `CI` to verify prebuilt consumption from this repository's release assets.

## `harmony-self-hosted.yml` (Harmony self-hosted)

- **Trigger:** manual (`workflow_dispatch`)
- **Runs:** ArkJS/OHOS `check`, `clippy`, and the Rust-side Harmony smoke-library build on a self-hosted runner with `OHOS_NDK_HOME`
- **Requirements:** runner labels `self-hosted` and `harmony`; intended for future local-runner coverage, not GitHub-hosted CI

## `release.yml` (manual package publish)

- **Trigger:** manual (`workflow_dispatch`)
- **Input:** `package_scope` controls the package family:
  - `all`: publish selected Rust crates and repo-maintained `@rongjs/*` npm packages
  - `rust`: publish selected Rust crates only
  - `npm`: publish repo-maintained `@rongjs/*` npm packages only
- **Input:** `rust_selection` is passed to `scripts/publish.sh`; examples:
  - `--crate rong_timer`
  - `--crate rong_jscore_sys --crate rong_jscore`
  - `--group engines`
  - `--changed-since v0.4.0`
- **Input:** `create_tags` creates package-level tags such as
  `rong_timer-v0.4.1` and `npm-rongjs-rong-v0.4.1`
- **Runs:** publishes the selected package set; it does not create repo-level
  `vX.Y.Z` tags or GitHub Releases
- **Requirements:** run from `master`; package versions and `CHANGELOG.md` must
  already be updated when relevant

## Secrets

- `CARGO_REGISTRY_TOKEN` (required for publish)
- `GITHUB_TOKEN` (default Actions token; used when `create_tags=true`)

npm publishing uses Trusted Publishing through GitHub Actions OIDC. Configure the
trusted publisher for each repo-maintained npm package in npm package settings;
do not configure token-based npm credentials for this workflow.

## Local testing

```bash
cargo make ci-verify
ENGINE=jscore cargo make ci-verify
RONG_JSC_SOURCE=1 ENGINE=jscore cargo make ci-verify
cargo make ci-verify-all
```

## Local hooks

```bash
git config --local core.hooksPath .githooks
./.githooks/pre-commit
./.githooks/pre-push
```

Local hooks are layered:

- `pre-commit`: `cargo fmt --all -- --check`
- `pre-push`: `cargo make pre-commit` (`fmt` + `check` + `clippy` with the default `quickjs` engine)

For local release steps, see `scripts/README.md`.
