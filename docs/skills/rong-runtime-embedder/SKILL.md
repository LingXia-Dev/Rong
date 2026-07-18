---
name: rong-runtime-embedder
description: >-
  Build, configure, and debug Rust hosts that embed the Rong JavaScript runtime.
  Use when selecting QuickJS, JavaScriptCore, or ArkJS features; creating
  runtimes and contexts; configuring module subsets and the host executor;
  choosing shared or pinned worker pools; enforcing execution timeouts or hard
  interruption; or reasoning about runtime, context, worker, and shutdown
  lifecycles. Do not use for writing Rong JavaScript scripts or authoring
  `rong_*` Rust-to-JavaScript module bindings.
---

# Embedding Rong in Rust

Keep host policy explicit. Select one engine, choose the smallest module
surface, pick a placement model deliberately, and bound untrusted or potentially
non-yielding execution.

## Workflow

1. Inspect the host crate's Rong features and target platform.
2. Read [references/runtime-setup.md](references/runtime-setup.md) when creating
   a runtime/context, selecting an engine, installing modules, or configuring
   the process-global executor.
3. Read [references/worker-pools.md](references/worker-pools.md) when choosing
   raw runtimes, shared workers, pinned workers, task handles, or shutdown
   behavior.
4. Read [references/interruption.md](references/interruption.md) whenever work
   needs a timeout, deadline, cancellation policy, hard interruption, or an
   engine capability decision.
5. Implement against public `rong` and `rong_modules` APIs. Inspect core
   internals only when changing Rong itself.
6. Verify the exact engine and feature combinations affected by the change.

## Core Rules

- Enable exactly one of `quickjs`, `jscore`, or `arkjs` in a host binary.
- Keep a raw `JSRuntime` and its contexts on their owning thread. Use Rong
  workers for host-side cross-thread scheduling.
- Use `shared()` for independent stateless tasks and `pinned::<K, S>()` for
  deterministic key affinity and retained state.
- Prefer `call_with_timeout` or `TaskHandle::join_with_timeout` over manually
  coordinating timers and interrupt cleanup.
- Treat `InterruptMode::Cooperative` as unable to stop non-yielding synchronous
  JavaScript. Do not claim a hard execution bound in that mode.
- Keep pool `Worker::terminate()` separate from hard interruption: termination
  is graceful, while timeout/interrupt is the bounded-execution mechanism.
- Install only the dependency-closed module set the host intends to expose;
  use `rong_modules::init_all` only for an explicit full-surface policy.
- Reject sync bridge APIs from inside a Rong worker or active Tokio runtime;
  use their async counterparts there.

## Verification

Start with focused checks, then expand to the affected engine matrix:

```bash
cargo fmt --all -- --check
cargo check -p rong --no-default-features
cargo test -p rong --features quickjs --test rong
cargo test -p rong --features quickjs --test interrupt
```

For JSC interruption changes, also test system-framework cooperative mode,
system-framework opt-in preemption, and `jscore-source`. For ArkJS changes, use
the HarmonyOS/OpenHarmony target and device harness rather than assuming desktop
behavior.
