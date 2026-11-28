# rong_quickjs

QuickJS backend for RongJS.

This crate integrates the vendored `rong_quickjs_sys` bindings with Rong's
engine-agnostic runtime traits and value model.

- Use `rong` if you want the public embedding API.
- Use `rong_quickjs` if you are wiring the QuickJS backend into lower-level
  Rong internals.
- Use `rong_quickjs_sys` only if you need the raw FFI layer.

## Optional features

- `intl` (`rong`'s `intl`): `rong_quickjs::intl::init` installs rong's
  ECMA-402 `Intl` (`rong_intl`) on a QuickJS context.
- `jit` (`rong`'s `quickjs-jit`): Cranelift baseline JIT compiler via
  `rong_quickjs::jit` on `x86_64` and `aarch64` (no-op on iOS and other targets).
