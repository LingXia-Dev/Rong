# GitHub Workflows

## `ci.yml` (tests)

- **Trigger:** push + pull_request
- **Runs:** `test.sh` on macOS for:
  - `quickjs`
  - `jscore`
- **Submodules in CI:**
  - Initializes `quickjs/sys/quickjs-ng` only
  - Does not recursively fetch `third_party/WebKit` by default

`jscore-provider-webkit` is validated via local scripts or dedicated provider jobs where
WebKit build artifacts are available.

## `release-pr.yml` (release-plz)

- **Trigger:** manual (`workflow_dispatch`)
- **Runs:** release-plz to open/update a Release PR based on Conventional Commits
- **Labels:** applies `release` label to Release PRs (see `release-plz.toml`)
  - **Action:** `release-plz/action@v0.5`

## `release.yml` (release-plz)

- **Trigger:** manual (`workflow_dispatch`)
- **Runs:** publish workflow after Release PR merge (creates tag, publishes crates, creates GitHub Release)
  - **Action:** `release-plz/action@v0.5`

## Secrets

- `CARGO_REGISTRY_TOKEN` (required for publish)

## Local testing

```bash
bash test.sh -e quickjs
bash test.sh -e jscore
```

For local release steps, see `scripts/README.md`.
