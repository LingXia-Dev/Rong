# Publishing Scripts (Maintainer)

Recommended path: maintain package versions and `CHANGELOG.md` manually, then
use the GitHub Actions `Publish Packages` workflow to execute the selected
package publish.

Release flow summary: see [`docs/releasing.md`](../docs/releasing.md).

## Local verification

```bash
cargo make pre-commit
cargo make ci-verify
ENGINE=jscore cargo make ci-verify
cargo make ci-verify-all
```

- `cargo make pre-commit`: fast local gate (`cargo fmt --check` + `cargo check` + `cargo clippy`)
- `cargo make ci-verify`: CI-equivalent gate for one engine, including `bash test.sh -e <engine>`
- `cargo make ci-verify-all`: runs `ci-verify` sequentially for all default CI engines

Optional local hook setup:

```bash
git config --local core.hooksPath .githooks
./.githooks/pre-commit
./.githooks/pre-push
```

- `pre-commit` hook only runs `cargo fmt --all -- --check`
- `pre-push` hook runs `cargo make pre-commit`

## bump_version.sh

```
./scripts/bump_version.sh <version> [--crate NAME]... [--group NAME]... [--npm] [--commit]
```

- Bumps selected Rust crates and/or repo-maintained npm packages
- Rust package groups: `core`, `engines`, `modules`, `bundles`,
  `non-modules`, `rust`, `all`
- Transitively includes publishable reverse dependencies when the target
  version falls outside their current Cargo requirement (for example,
  `0.4 -> 0.5`), preventing mixed internal ABIs
- Updates matching `[workspace.dependencies]` version lower bounds for selected
  Rust crates
- Default is file update only (no git ops)
- Does not create tags or GitHub releases

## publish.sh

```
./scripts/publish.sh [--crate NAME]... [--group NAME]... [--changed] [--changed-since REF] [--package-tag] [--dry-run] [--check-drift] [--yes]
```

- Publishes selected Rust crates in dependency order
- Supports `--crate`, `--group`, `--changed`, and `--changed-since` selection
- `--changed` selects crates with publish-relevant changes since their own
  nearest reachable package or product tag (`<crate>-vX.Y.Z` or `vX.Y.Z`).
  Explicit package version bumps count, while the matching
  `bump_version.sh` lower-bound syncs on `rong*` dependency entries and files
  belonging to nested crates are ignored. External dependency version changes
  (e.g. bumping `tokio`) also count.
- `--changed-since REF` is the same change detection against one explicit ref
- Defaults to all publishable Rust crates only when no selection is provided,
  preserving the old full-publish behavior
- Fails fast when the script's `CRATES` list drifts from the workspace's
  publishable members, so a new crate cannot be silently left unpublished
- Published-version checks use the crates.io sparse index (exact version match),
  with `cargo search` as a network fallback
- Requires `CARGO_REGISTRY_TOKEN`
- Smart waiting: polls crates.io until each package is indexed
- Paces successful uploads by 10 seconds by default and retries transient
  crates.io `429`/`5xx`/network failures with bounded exponential backoff;
  authentication, manifest, and compilation failures still fail immediately
- Optional `--package-tag` creates package-level tags such as
  `rong_timer-v0.4.1`
- `--dry-run` prints the selected publish plan without requiring a token, and
  flags drift: selected crates whose current version is already on crates.io
  (they need a version bump before publishing)
- `--check-drift` makes `--dry-run` exit with status 2 when drift is found
- `--yes` skips the confirmation prompt (useful for CI)

## publish_npm.sh

```
./scripts/publish_npm.sh [--package-tag]
```

- Publishes all repo-maintained npm packages:
  - `@rongjs/rong` from `packages/rong_types`
  - `@rongjs/rong-skill` from `packages/skill`
- Runs only in GitHub Actions with npm Trusted Publishing through OIDC
- Skips the publish if the same npm version already exists
- Optional `--package-tag` creates package-level tags such as
  `npm-rongjs-rong-v0.4.1`
- First-time npm package creation must happen outside this repository automation
  before trusted publishing can be configured for that package

## GitHub publish flow (recommended)

1. Update the package versions that are actually being released and update
   `CHANGELOG.md`.
2. Land the release change on `master`.
3. GitHub → Actions → run workflow `Publish Packages` from `master`.
4. Choose `package_scope`, `rust_selection`, and whether to create package tags.

Notes:

- `Publish Packages` does not infer a workspace version.
- `rust_selection` is passed to `scripts/publish.sh`, for example
  `--changed`, `--crate rong_timer`, `--group engines`, or
  `--changed-since v0.4.0`.
- Package-level tags are optional and separate, e.g. `rong_timer-v0.4.1` or
  `npm-rongjs-rong-v0.4.1`.
- Product-level tags such as `v0.4.1` remain explicit maintainer decisions.
  Set the optional `product_release` input (e.g. `v0.4.1`) when you want the
  workflow to create the tag and a GitHub Release from the matching
  `CHANGELOG.md` entry after a successful publish; leave it empty otherwise.
- Rust publishes authenticate with crates.io Trusted Publishing (GitHub OIDC)
  when the trusted publisher is configured for the crates, and fall back to the
  `CARGO_REGISTRY_TOKEN` secret. npm packages require npm trusted publisher
  configuration.
- CI on `master` pushes posts an informational "Pending release plan"
  (`publish.sh --changed --dry-run`) in the job summary, showing which crates
  have unreleased changes and which need version bumps.

## Local Rust recovery flow

Use this when GitHub Actions is unavailable or when you need to recover Rust
crate publishing manually. npm publishing is intentionally CI-only through
Trusted Publishing.

1. Run `./scripts/bump_version.sh <version>` with a matching package selection.
2. Update `CHANGELOG.md` for the same version.
3. Review, commit, and push the release changes.
4. Run `./scripts/publish.sh` with the same crate/group selection.
5. Create product-level tags or GitHub Releases manually when you intentionally
   want a product release point.

## Troubleshooting

- Version exists on crates.io → bump patch version
- Publish fails mid-way → run `cargo publish -p <crate>`
