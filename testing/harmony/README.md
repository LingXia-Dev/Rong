# Harmony Test App

This folder contains the minimum pieces needed for on-device JSVM testing:

- `rong_harmony_smoke/`: Rust cdylib that exposes `startHttpServer()`
- `app/`: minimal HarmonyOS app that loads the `.so` and starts the HTTP server on launch
- `dev.sh`: builds the cdylib, stages it into the app, installs/starts the HAP, forwards a local port, and uses `curl` to trigger tests and read the JSON result

## Layout

```text
testing/harmony/
├── rong_harmony_smoke/   # Rust cdylib entry point for HTTP-controlled test runs
├── app/                  # Minimal HarmonyOS test shell
└── dev.sh                # Build, stage, install, and launch helper
```

## Flow

1. Build and stage the native library:

```bash
./testing/harmony/dev.sh --rust-only
```

Use `TEST_FILTER=...` to run a single case or subset. Leave it empty to run all
device tests.

2. Or run the full device flow:

```bash
./testing/harmony/dev.sh test
```

3. Run a single case or subset from the PC by setting a filter before launch:

```bash
TEST_FILTER=rong.test_call_simple ./testing/harmony/dev.sh test
```

4. For debugging, watch runner logs:

```bash
hdc hilog | grep RongSmoke
```

The app page only explains what the app does. The native library starts a local
HTTP server inside the app process. `dev.sh` waits until the device port is
listening, forwards a host port with `hdc fport`, then uses `curl` against
`/health` and `/run` to get the full JSON report on the PC. `RongSmoke` hilog
is only for debugging.

## Notes

- Generated outputs and caches are intentionally not kept in this tree.
- `build-profile.json5` intentionally leaves signing empty. Fill it in with your local Harmony signing config before packaging the app.
