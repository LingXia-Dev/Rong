# rong_command

Command execution APIs mounted on the `Rong` namespace.

## Embedder authority

`init()` installs the JavaScript APIs with no host gate — the historical
behavior used by `rong_modules` and scripts.

Hosts that must bind process execution to a live session grant should call
`init_with_authority` instead. The authority is sealed on the `JSContext`:
JavaScript cannot install or replace it, a second installer is rejected, and a
failing `authorize` kills in-flight children and refuses retained handles.

```rust
use std::sync::Arc;
use rong_command::{ProcessAuthority, init_with_authority};

struct SessionGrant;

impl ProcessAuthority for SessionGrant {
    fn authorize(&self) -> Result<(), String> {
        // Recheck the live host grant. Err(...) revokes running children.
        Ok(())
    }
}

init_with_authority(&ctx, Arc::new(SessionGrant))?;
```

## JS APIs

- `Rong.spawn(...)` - async subprocess wrapper with streams, timeouts, and exit hooks
- `Rong.spawnSync(...)` - synchronous subprocess execution with captured `stdout` / `stderr`
- `Rong.stdin` / `Rong.stdout` / `Rong.stderr` - runtime stdio handles on the `Rong` namespace
- `Rong.$` - shell template tag with `.text()`, `.json()`, `.lines()`, `.blob()`, `.run()`, `.quiet()`, `.nothrow()`, and `.cwd()`
