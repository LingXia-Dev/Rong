## Summary

- 

## Validation

- [ ] `cargo make ci-verify`
- [ ] `ENGINE=jscore cargo make ci-verify` (if macOS / JavaScriptCore behavior changed)
- [ ] GitHub Actions `CI` passed, or only `scope` / `npm-package` ran for docs or npm-only changes
- [ ] GitHub Actions `CI` `jscore-source-*` jobs passed on macOS/Linux/Windows (if JavaScriptCore source backend changed)
- [ ] GitHub Actions `Build JSC artifacts` was run and `javascriptcore/sys/webkit-artifacts.tsv` updated (if pinned JSC artifact version changed)
- [ ] `npm --prefix packages/rong_types run build` (if Rong TypeScript package changed)
- [ ] `npm --prefix packages/skill run check` (if `docs/skills`, `docs/api`, or the skill package changed)
- [ ] `npm --prefix packages/test run check` (if the JavaScript test package changed)
- [ ] `cargo make check-arkjs-ohos` (if ArkJS / OHOS code changed)
- [ ] `cargo make clippy-arkjs-ohos` (if ArkJS / OHOS code changed)
- [ ] `./testing/harmony/dev.sh test` on a device or local Harmony runner (if Harmony runtime behavior changed)
- [ ] Relied on GitHub Actions host CI for platform coverage
- [ ] Not run locally

If any relevant checks were skipped, explain why:

- 

## Release Impact

- [ ] No user-facing release note needed
- [ ] `CHANGELOG.md` updated
- [ ] Version bump included

## Notes

- 
