# Engine & Provider Strategy

This document defines the near-term design for:

- Multi-engine support (`quickjs`, `jscore`, `arkjs`)
- JavaScriptCore provider split (`sys` vs source-built WebKit provider)
- Harmony/ArkJS testing under device-only constraints
- Permission-sensitive module behavior on HarmonyOS

## 1. Current Status

- Android production path uses `quickjs`.
- Apple platforms use `jscore` through system JavaScriptCore (`rong_jscore_sys`).
- iOS/App Store practical constraint: prefer the system-provided `JavaScriptCore.framework` for `jscore`. Treat the source-built WebKit/JSC provider as non-Apple-only to minimize review and platform-integration risk.
- `arkjs` backend code exists, but runtime validation is blocked by limited Harmony test environments.
- Harmony permission policy is stricter than iOS, especially for process/filesystem/network-adjacent APIs.
- There is intent to support a source-built JavaScriptCore provider (WebKit-based, Bun migration path) for Linux/Windows.

## 2. Goals

1. Keep the top-level Rong API unchanged across engines.
2. Decouple "engine" from "provider" for JavaScriptCore.
3. Enable deterministic builds for Linux/Windows with source-built JSC provider.
4. Make ArkJS testable without requiring every developer to own Harmony hardware.
5. Normalize module behavior under strict permission models.

## 3. Non-Goals

- No short-term requirement to publish `arkjs` crates to crates.io.
- No promise of 100% API parity with Node.js for restricted Harmony environments.
- No runtime engine hot-switching in one binary; engine selection remains compile-time.

## 4. Terms

- Engine: JS runtime family (`QuickJS`, `JavaScriptCore`, `ArkJS`).
- Provider: concrete implementation source for one engine family (for JSC: default system provider or `provider-webkit`).

## 5. Feature Model (Proposed)

### 5.1 Top-Level Engine Features (unchanged)

At `rong` / `rong_modules` / `rong_cli` level:

- `quickjs`
- `jscore`
- `arkjs` (placeholder until ready)

Rules:

- `quickjs` and `jscore` stay mutually exclusive.
- `arkjs` remains opt-in and non-default until validation/packaging is stable.

### 5.2 JSC Provider Features (new axis)

Current implementation:

- default path (no provider feature): use OS JavaScriptCore (`xcrun` + framework) on Apple targets
- `provider-webkit`: use source-built WebKit/Bun-migrated provider (Linux/Windows target)

Current compile-time guards:

- default path + non-Apple => build error with actionable message
- `provider-webkit` + `target_os = ios` => build error (policy and platform constraints)

### 5.3 Workspace Feature Forwarding

`rong` feature forwarding should remain explicit:

- `jscore` enables `rong_jscore`
- provider selection forwarded separately as additive flags where needed
- CLI presets can keep "works out-of-box" behavior per platform

## 6. JSC Provider Architecture

### 6.1 Layering

- `rong_core`: engine-agnostic traits (`JSContextImpl`, `JSRuntimeImpl`, `JSValueImpl`)
- `rong_jscore`: JSC backend logic against a provider abstraction
- `rong_jscore_sys` / `rong_jscore_webkit`: FFI + build/link details

Provider crates own:

- symbol binding
- linking strategy
- header/bindgen/build scripts
- platform-specific ABI compatibility checks

Backend crate (`rong_jscore`) owns:

- value semantics
- context/runtime behavior
- promise/call/eval bridge
- error shaping

### 6.2 Implementation Status

The current workspace uses the "single provider crate with feature switch" approach:

- `rong_jscore_sys` default path links Apple system framework
- `provider-webkit` switches `build.rs` to WebKit inputs from environment variables (`RONG_JSC_WEBKIT_ROOT` preferred, with explicit include/lib overrides)
- iOS explicitly rejects `provider-webkit`

### 6.3 ABI & API Stability Expectations

- Provider crates must expose a stable minimal raw API surface consumed by `rong_jscore`.
- Provider-specific extensions should be feature-gated and not leak into `rong` public API.

## 7. Test Strategy

### 7.1 Test Levels

- L0: `rong_core` unit tests (engine-agnostic behavior)
- L1: engine conformance tests (`tests/*.rs`, `tests/unit/*.js`) across `quickjs` and `jscore`
- L2: provider parity tests (same `jscore` tests under `provider-sys` and `provider-webkit`)
- L3: ArkJS device validation (remote/device lane)

### 7.2 Harmony/ArkJS Practical Plan

Because desktop Harmony hosts are not available:

1. Keep compile/lint checks host-side:
   - `cargo check -p rong_arkjs`
   - `cargo test -p rong_arkjs --no-run`
2. Introduce device-runner scripts in `scripts/` (future):
   - package tests to runnable artifact
   - push to Harmony device farm / physical device
   - collect structured logs + pass/fail summary
3. Start with smoke suites:
   - eval/exception/promise/class/typed-array
4. Expand to module suites gradually, prioritizing permission-sensitive modules.

### 7.3 CI Matrix (Target Shape)

- QuickJS lane:
  - Linux/macOS/Android cross builds
  - full test suite
- JSC sys lane:
  - macOS (system framework)
- JSC webkit lane:
  - Linux/Windows (source-built provider)
- ArkJS lane:
  - build-only in main CI
  - scheduled or manually triggered device tests

## 8. Permission Model for Harmony

Define module capability tiers:

- Tier A (always safe): `console`, `assert`, pure value ops
- Tier B (permission-checked): `fs`, `http`, `storage`, `process`, `child_process`
- Tier C (platform-limited): APIs that cannot be supported without privileged capabilities

Guidelines:

- Permission denial should map to stable error codes (`E_PERMISSION_DENIED` + module code).
- Avoid silent fallback when security implications exist.
- For APIs with partial support, document behavior per platform and return deterministic errors.

## 9. Rollout Plan

### Milestone 0: Docs & Interfaces

- finalize this strategy doc
- freeze engine/provider naming
- define minimum provider trait surface

### Milestone 1: JSC Provider Split

- extract provider boundary from current `rong_jscore_sys`-only path
- add compile guards and feature forwarding

### Milestone 2: WebKit Provider Bring-up

- initial Linux build success
- pass core eval/call/promise tests

### Milestone 3: ArkJS Device Harness

- automated push/run/log for smoke tests on Harmony devices
- baseline stability report

### Milestone 4: Permission-Driven Module Matrix

- publish platform-module compatibility table
- add conformance tests for permission failures

## 10. Open Decisions

1. Whether to keep feature-switch mode or split out a dedicated `rong_jscore_webkit` crate.
2. Whether `rong_cli` should auto-select provider by target platform or require explicit flags.
3. Artifact strategy for source-built JSC provider in CI (cache vs prebuilt binaries).
4. Minimum Harmony OS/device versions to support in ArkJS validation.
5. How much of WebKit build orchestration should live in-tree vs external scripts/CI images.
