# Publishing Scripts (Maintainer)

Recommended path: use the **GitHub Actions** release-plz workflows. Local scripts are here for manual use / emergencies.

## bump_version.sh

```
./scripts/bump_version.sh <version> [--commit]
```

- Updates `[workspace.package]` and syncs `[workspace.dependencies]`
- Default is file update only (no git ops)
- Does not create tags (release-plz uses per-package tags)

## publish.sh

```
./scripts/publish.sh [--no-verify] [--allow-dirty] [--yes]
```

- Publishes all workspace crates in dependency order
- Requires `CARGO_REGISTRY_TOKEN`
- Excludes: rong_arkjs* (WIP), rong_cli, rong_test, examples
- Smart waiting: polls crates.io until each package is indexed
- `--yes` skips the confirmation prompt (useful for CI)

## GitHub release flow (recommended, manual)

1. Land changes on `master` (prefer Conventional Commits: `fix: ...`, `feat: ...`, `feat!: ...`).
2. GitHub → Actions → run workflow `Release: Prepare PR` (select branch `master`).
3. Review and merge the generated “Release PR” (this PR contains the version bumps + changelog updates).
4. GitHub → Actions → run workflow `Release: Publish` (select branch `master`).

Notes:
- The “version bump” is done by release-plz inside the Release PR; you generally do **not** run `bump_version.sh` for the GitHub-based flow.
- `Release: Publish` requires `CARGO_REGISTRY_TOKEN` secret to publish to crates.io.
  - The GitHub workflows use `release-plz/action@v0.5` (latest v0.5.x).

## Local manual flow (not recommended)

Use this only if you intentionally want to bypass release-plz automation:

1. Run `./scripts/bump_version.sh <version>` and commit the changes.
2. Run `./scripts/publish.sh` to publish crates.
3. Create Git tags / GitHub Releases manually as needed.

## Troubleshooting

- Version exists on crates.io → bump patch version
- Publish fails mid-way → run `cargo publish -p <crate>`

## WebKit Provider Helpers

### webkit_submodule.sh

```
./scripts/webkit_submodule.sh init
./scripts/webkit_submodule.sh bump
./scripts/webkit_submodule.sh status
```

- `init`: initialize/update `third_party/WebKit` to the pinned commit
- `bump`: update to latest `main` from remote and print the new commit
- `status`: print current submodule commit

Proxy note:

```
git config --global http.proxy http://127.0.0.1:7890
git config --global https.proxy http://127.0.0.1:7890
```

### check_jscore_webkit.sh

```
./scripts/check_jscore_webkit.sh
./scripts/check_jscore_webkit.sh /abs/path/to/WebKit
./scripts/check_jscore_webkit.sh /abs/path/to/WebKit cargo test -p rong --no-default-features --features jscore-provider-webkit
```

- Loads `target/webkit-provider/env.sh` automatically when present
- Exports `RONG_JSC_WEBKIT_ROOT` automatically
- Defaults `RONG_JSC_WEBKIT_LINK_KIND`:
  - `framework` on macOS
  - `dylib` on non-macOS
- Runs `cargo check -p rong --no-default-features --features jscore-provider-webkit` when no custom command is provided

### build_webkit_provider.sh

```
./scripts/build_webkit_provider.sh --release
./scripts/build_webkit_provider.sh --debug
./scripts/build_webkit_provider.sh --webkit-root /abs/path/to/WebKit --build-dir /tmp/WebKitBuild
```

- Runs `Tools/Scripts/build-jsc`
- Resolves include/lib locations
- Writes provider env file at `target/webkit-provider/env.sh`
- On macOS, requires full Xcode (`xcodebuild`) rather than Command Line Tools only

### e2e_webkit_provider.sh

```
./scripts/e2e_webkit_provider.sh
./scripts/e2e_webkit_provider.sh --bump
./scripts/e2e_webkit_provider.sh -- cargo test -p rong --no-default-features --features jscore-provider-webkit
```

- Full flow: submodule init/bump -> build provider -> check provider
- After build, `bash test.sh -e jscore-provider-webkit -c` reuses generated env from `target/webkit-provider/env.sh`

### parity_jscore_provider.sh

```
./scripts/parity_jscore_provider.sh
./scripts/parity_jscore_provider.sh --test eval
```

- Runs selected core tests against:
  - `jscore`
  - `jscore-provider-webkit`
