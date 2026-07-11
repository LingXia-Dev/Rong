# rong_modules

Feature-gated module bundle for RongJS.

This crate collects the published Rong modules behind one dependency so embedders
can enable a curated module set with Cargo features instead of wiring each
module crate individually.

## Compile-time and per-context selection

Cargo features define the maximum module set available in the binary. Module
features include their runtime dependencies, so enabling `http`, for example,
also compiles its abort, exception, buffer, URL, and stream support.

At runtime, `init` installs an explicit module set and its dependency closure:

```rust
rong_modules::init(&ctx, ["console", "http"])?;
```

Use `init_all` only when the context should receive every compiled module:

```rust
rong_modules::init_all(&ctx)?;
```

Resolution happens before the context is modified. An unknown name or a module
not compiled into the binary returns `E_INVALID_ARG`; it never leaves a
partially initialized subset behind. Repeated calls initialize only modules not
already installed through this registry in the same context.

Registry inspection APIs:

- `compiled_modules()` returns the static module descriptors.
- `compiled_module_names()` iterates the compiled module names.
- `is_compiled(name)` checks build-time availability.
- `resolve_modules(names)` validates a request and returns its dependency
  closure.
- `init(ctx, names)` installs an explicit dependency-closed module set.
- `init_all(ctx)` explicitly installs every compiled module.
- `initialized_module_names(ctx)` and `is_initialized(ctx, name)` inspect
  per-context registry state.

Dependencies are part of the effective API grant. Module selection controls
what this registry installs; it does not track direct calls to individual
module crates or host-injected objects.
