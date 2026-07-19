# Contributing

This project supports multiple JavaScript engines and a growing module set. Keep local checks and CI aligned by using the shared `cargo make` tasks.

## Development Setup

Requirements:

- Rust stable toolchain
- `cargo-make`
- Node.js 20+ for npm package and agent skill validation
- On macOS, `llvm` is required for bindgen-based crates

Install helper tools:

```bash
cargo install cargo-make
brew install llvm
```

If `llvm` is installed by Homebrew, export `LIBCLANG_PATH` when needed:

```bash
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
```

## Local Verification

Fast local gate:

```bash
cargo make pre-commit
```

CI-equivalent verification for the default engine (`quickjs`):

```bash
cargo make ci-verify
```

Run the same verification for JavaScriptCore:

```bash
ENGINE=jscore cargo make ci-verify
```

Run the default CI engine set locally:

```bash
cargo make ci-verify-all
```

Validate npm packages when changing `packages/rong_types`, `packages/skill`,
`packages/test`, `docs/api`, or `docs/skills`:

```bash
npm --prefix packages/rong_types install --no-package-lock
npm --prefix packages/rong_types run build
npm --prefix packages/skill run check
npm --prefix packages/test run check
```

What these tasks do:

- `pre-commit`: `cargo fmt --check` + `cargo check` + `cargo clippy`
- `ci-verify`: `pre-commit` checks plus `bash test.sh -e <engine>`
- `ci-verify-all`: runs `ci-verify` sequentially for `quickjs` and `jscore`
- `npm --prefix packages/skill run check`: syntax-checks the skill CLI/packer and verifies
  `docs/skills` plus `docs/api` can be packed into self-contained skills
- `npm --prefix packages/test run check`: syntax-checks and exercises the
  engine-neutral JavaScript test framework without Rong or Node runtime APIs

The shared tasks exclude `rong_arkjs`, `rong_arkjs_sys`, and the device-only
`rong_test_device` crate from the default host CI gate. ArkJS/OHOS is validated
through separate Harmony-focused checks instead of the default host matrix. The
tasks also check `lib/bin/test` targets instead of `--all-targets` to avoid
example-only regressions from blocking normal work.

## Git Hooks

Hook layering:

- `pre-commit`: format-only, keeps each commit fast
- `pre-push`: runs `cargo make pre-commit`
- CI: runs the full verification flow

Install the local hooks:

```bash
git config --local core.hooksPath .githooks
```

Run them manually:

```bash
./.githooks/pre-commit
./.githooks/pre-push
```

## Test Matrix

- `quickjs`: default local engine; CI runs it on Windows, Linux, and macOS
- `jscore`: secondary CI engine; CI runs it on macOS with the system
  `JavaScriptCore.framework`
- `jscore-source-*`: source-backend consumer jobs run on macOS, Linux, and
  Windows when pinned WebKit/JSCOnly artifacts are listed in
  [`javascriptcore/sys/webkit-artifacts.tsv`](javascriptcore/sys/webkit-artifacts.tsv)
  (see [`javascriptcore/sys/README.md`](javascriptcore/sys/README.md))
- `arkjs`: verified separately through Harmony/OHOS checks; not part of the default host verification gate

For ad hoc engine tests, the lower-level runner remains available:

```bash
bash test.sh -e quickjs
bash test.sh -e jscore
```

For HarmonyOS device-side verification, use:

```bash
./testing/harmony/dev.sh test
```

For future self-hosted Harmony CI coverage, provision a local runner with
`OHOS_NDK_HOME` and the `harmony` runner label, then use the
`Harmony CI (Self-Hosted)` workflow.

## CI Scope

The main `CI` workflow starts with a lightweight `scope` job:

- Docs, Markdown, and GitHub metadata changes do not run the Rust/JSC host
  matrix unless they touch workflow behavior.
- Changes under `docs/api`, `docs/skills`, `packages/rong_types`,
  `packages/skill`, or npm release scripts run the npm package validation job.
  `packages/test` changes also run the Rust host-engine matrix because its
  runtime is embedded by `UnitJSRunner`.
- Rust/source changes run format, host engine checks, clippy, tests, and the
  `jscore-source-*` prebuilt-consumer jobs.
- Manual `workflow_dispatch` runs all scopes.

There is no standalone `JSC Windows (source)` workflow. Windows source-backend
coverage is split between `Build JSC artifacts` for producing prebuilt JSC
tarballs and the normal `CI` workflow's `jscore-source-x86_64-pc-windows-msvc`
job for consuming the pinned artifact.

## Agent Skills And npm Packages

- Source skill documentation lives under [`docs/skills`](docs/skills).
- Shared runtime/API reference material lives under [`docs/api`](docs/api).
- The npm package in [`packages/skill`](packages/skill) packs those docs into installable,
  self-contained skills. Treat generated `packages/skill/assets` output as build output,
  not source documentation.
- The repo-maintained npm packages are published under the `@rongjs` scope:
  `@rongjs/rong` from [`packages/rong_types`](packages/rong_types) and
  `@rongjs/rong-skill` from [`packages/skill`](packages/skill), and
  `@rongjs/test` from [`packages/test`](packages/test).

## Release Flow

Preferred flow:

1. Prepare a normal PR with the package version bump(s) and matching
   `CHANGELOG.md` update.
2. Merge that PR into `master`.
3. Run `Publish Packages` in GitHub Actions from `master`.
4. Choose the package family and Rust selection that match the packages being
   released.

The publish workflow can publish Rust crates, npm packages, or both. It creates
package-level tags only when `create_tags=true`; product-level tags such as
`v0.4.1` are explicit maintainer decisions and are not created by CI.

For local release details, see [`scripts/README.md`](scripts/README.md).
For the full maintainer checklist, see [`docs/releasing.md`](docs/releasing.md).

`./scripts/bump_version.sh` bumps selected Rust crates and/or repo-maintained npm
packages. `./scripts/publish.sh` publishes selected Rust crates in dependency
order. `./scripts/publish_npm.sh` publishes all repo-maintained `@rongjs/*` npm
packages.

## Notes For Contributors

- Keep CI and local commands aligned. Prefer updating `Makefile.toml` instead of adding one-off commands to workflows.
- When adding or removing published crates, update `scripts/publish.sh`.
- When adding or removing repo-maintained npm packages, update
  `scripts/publish_npm.sh`, the release workflow, and the npm package CI scope.
- When changing pinned JSC source artifacts, run `Build JSC artifacts`, review
  the emitted TSV rows, update `javascriptcore/sys/webkit-artifacts.tsv`, then
  rely on normal `CI` `jscore-source-*` jobs to verify consumption.
- If a check is engine-specific, be explicit about which engine it applies to.
