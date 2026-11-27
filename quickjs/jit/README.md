# rong_quickjs_jit

Cranelift-backed baseline JIT compiler for RongJS on QuickJS-NG.

This crate is internal to `rong_quickjs`. Enable the `quickjs-jit` feature on
`rong` (or `jit` on `rong_quickjs`) and use `rong::quickjs::jit`.

## Design

- **Zero overhead when disabled**: `rong_quickjs_sys` only injects its minimal
  `#ifdef CONFIG_RONG_JIT` hooks into the build-time merged `quickjs.c` when the
  `jit` feature is enabled. The vendored `quickjs-ng` submodule is untouched.
- **Hot-function compilation**: Normal JS bytecode functions are profiled on
  entry and compiled with Cranelift once their call count reaches
  `JitConfig::hot_threshold`.
- **Interpreter fallback**: Functions using opcodes not yet lowered by the JIT
  (such as generators, `async`/`await`, `try`/`catch`, `with`, `eval`, or child
  closures capturing outer locals) or frames exceeding 512 slots are marked and
  run directly in QuickJS's interpreter without recompilation attempts.
- **Platform support**: Enabled on `x86_64` and `aarch64` (Windows, macOS,
  Linux, Android, HarmonyOS). On iOS and other targets, the crate compiles as a
  graceful no-op (`is_supported() == false`).
- **Operational notes**:
  - Compiled machine code lives in the runtime's `JITModule` arena and is
    released when the `JSRuntime` is dropped; avoid `hot_threshold = 1` on
    workloads that continuously evaluate and discard one-shot `new Function`
    closures.
  - Stack traces preserve JIT call frames (including self-recursion) and report
    the function's entry line number.
