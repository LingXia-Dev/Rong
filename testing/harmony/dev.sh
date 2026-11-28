#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
APP_DIR="$SCRIPT_DIR/app"
ENTRY_DIR="$APP_DIR/entry"

TARGET="aarch64-unknown-linux-ohos"
CRATE_MANIFEST="$SCRIPT_DIR/rong_harmony_smoke/Cargo.toml"
SO_NAME="librong_harmony_smoke.so"
SO_SRC="$REPO_ROOT/target/$TARGET/release/$SO_NAME"
SO_DEST="$ENTRY_DIR/libs/arm64-v8a/$SO_NAME"
HAP_PATH=""
APP_BUNDLE="app.rong.harmony.smoke"
APP_ABILITY="EntryAbility"
DEVICE_PORT="${DEVICE_PORT:-18080}"
LOCAL_PORT="${LOCAL_PORT:-}"
TEST_FILTER="${TEST_FILTER:-}"
TEST_TIMEOUT_SECS="${TEST_TIMEOUT_SECS:-60}"
APP_PID=""
DO_PACKAGE=true
DO_INSTALL=true
DO_START=true

usage() {
  cat <<'EOF'
Usage:
  ./testing/harmony/dev.sh [test] [options]

Default behavior:
  1. Build the Rust .so
  2. Install app dependencies
  3. Build the Harmony app
  4. Install the HAP on the connected phone
  5. Force-stop the old app process
  6. Start the app
  7. Wait until the device HTTP server is listening
  8. Forward a local TCP port to the device
  9. Use curl to POST one test run and print the JSON summary

Environment:
  TEST_FILTER        Optional substring filter; empty runs all tests
  DEVICE_PORT        Device HTTP server port (default: 18080)
  LOCAL_PORT         Local forwarded port; defaults to a free ephemeral port
  TEST_TIMEOUT_SECS  HTTP readiness and test timeout (default: 60)

Options:
  test           Build, install, start, and run device-side tests
  --rust-only    Only build and stage the .so
  --no-install   Build app but do not install/start it
  --no-start     Install app but do not start it
  -h, --help     Show this help
EOF
}

if [ "${1:-}" = "test" ]; then
  shift
fi

for arg in "$@"; do
  case "$arg" in
    --rust-only)
      DO_PACKAGE=false
      DO_INSTALL=false
      DO_START=false
      ;;
    --no-install)
      DO_INSTALL=false
      DO_START=false
      ;;
    --no-start)
      DO_START=false
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $arg" >&2
      usage >&2
      exit 1
      ;;
  esac
done

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Required command not found: $1" >&2
    exit 1
  fi
}

check_signing_config() {
  if grep -q '"signingConfigs": \[\]' "$APP_DIR/build-profile.json5"; then
    cat >&2 <<'EOF'
Harmony signing is not configured in testing/harmony/app/build-profile.json5.
Fill in a local signingConfigs entry before using the full app packaging flow.
If you only want the native library, run:
  ./testing/harmony/dev.sh --rust-only
EOF
    exit 1
  fi
}

setup_ohos_env() {
  if [ -z "${OHOS_NDK_HOME:-}" ]; then
    echo "OHOS_NDK_HOME is not set" >&2
    exit 1
  fi

  export CARGO_TARGET_DIR="$REPO_ROOT/target"
  export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_OHOS_LINKER="$OHOS_NDK_HOME/native/llvm/bin/aarch64-unknown-linux-ohos-clang"
  export AR_aarch64_unknown_linux_ohos="$OHOS_NDK_HOME/native/llvm/bin/llvm-ar"
  export CC_aarch64_unknown_linux_ohos="$OHOS_NDK_HOME/native/llvm/bin/aarch64-unknown-linux-ohos-clang"
  export CXX_aarch64_unknown_linux_ohos="$OHOS_NDK_HOME/native/llvm/bin/aarch64-unknown-linux-ohos-clang++"
  export CPATH="$OHOS_NDK_HOME/native/sysroot/usr/include:$OHOS_NDK_HOME/native/sysroot/usr/include/aarch64-linux-ohos"
  export BINDGEN_EXTRA_CLANG_ARGS="--sysroot=$OHOS_NDK_HOME/native/sysroot -I$OHOS_NDK_HOME/native/sysroot/usr/include -I$OHOS_NDK_HOME/native/sysroot/usr/include/aarch64-linux-ohos"
}

build_rust() {
  echo "[1/9] Building Rust library..."
  setup_ohos_env
  cd "$REPO_ROOT"
  cargo build --release --target "$TARGET" --manifest-path "$CRATE_MANIFEST" --no-default-features --features "jsvm,tls-ring"
  mkdir -p "$(dirname "$SO_DEST")"
  cp "$SO_SRC" "$SO_DEST"
  echo "  staged $SO_NAME -> $SO_DEST"
}

install_app_deps() {
  echo "[2/9] Installing Harmony app dependencies..."
  require_cmd ohpm
  rm -rf "$APP_DIR/oh_modules" 2>/dev/null || true
  rm -f "$APP_DIR/oh-package-lock.json5" 2>/dev/null || true
  (cd "$APP_DIR" && ohpm install)
}

resolve_hap_path() {
  local output_dir="$ENTRY_DIR/build/default/outputs/default"
  if [ -f "$output_dir/entry-default-signed.hap" ]; then
    echo "$output_dir/entry-default-signed.hap"
    return 0
  fi
  if [ -f "$output_dir/entry-default-unsigned.hap" ]; then
    echo "$output_dir/entry-default-unsigned.hap"
    return 0
  fi
  find "$output_dir" -maxdepth 1 -type f -name '*.hap' | head -n1
}

build_app() {
  echo "[3/9] Building Harmony app..."
  require_cmd hvigorw
  rm -rf "$ENTRY_DIR/build"
  (cd "$APP_DIR" && hvigorw assembleHap)
  HAP_PATH="$(resolve_hap_path)"
  if [ -z "${HAP_PATH:-}" ] || [ ! -f "$HAP_PATH" ]; then
    echo "Expected HAP not found under: $ENTRY_DIR/build/default/outputs/default" >&2
    exit 1
  fi
  echo "  using HAP: $HAP_PATH"
}

install_app() {
  echo "[4/9] Installing HAP on device..."
  require_cmd hdc
  if ! hdc list targets | grep -q '.'; then
    echo "No Harmony device connected" >&2
    exit 1
  fi
  hdc install -r "$HAP_PATH"
}

stop_old_app() {
  echo "[5/9] Stopping previous app process..."
  require_cmd hdc
  hdc shell aa force-stop "$APP_BUNDLE" >/dev/null 2>&1 || true
}

start_app() {
  echo "[6/9] Starting app..."
  require_cmd hdc
  hdc shell aa start -a "$APP_ABILITY" -b "$APP_BUNDLE"
}

resolve_app_pid() {
  local pid=""
  local deadline=$((SECONDS + TEST_TIMEOUT_SECS))
  while [ "$SECONDS" -lt "$deadline" ]; do
    pid="$(hdc shell pidof "$APP_BUNDLE" 2>/dev/null | tr -d '\r' | tr -d '\n')"
    if [ -n "$pid" ]; then
      APP_PID="$pid"
      echo "  app pid: $APP_PID"
      return 0
    fi
    sleep 1
  done
  echo "Timed out waiting for app pid for $APP_BUNDLE" >&2
  exit 2
}

wait_for_device_server() {
  echo "[7/9] Waiting for device HTTP server on tcp:$DEVICE_PORT..."
  local port_hex
  port_hex="$(printf '%04X' "$DEVICE_PORT")"
  local deadline=$((SECONDS + TEST_TIMEOUT_SECS))
  while [ "$SECONDS" -lt "$deadline" ]; do
    if hdc shell "grep -qi ':$port_hex' /proc/$APP_PID/net/tcp /proc/$APP_PID/net/tcp6" >/dev/null 2>&1; then
      echo "  device server is listening on tcp:$DEVICE_PORT"
      return 0
    fi
    sleep 1
  done
  echo "Timed out waiting for device HTTP server on tcp:$DEVICE_PORT" >&2
  exit 2
}

resolve_local_port() {
  if [ -n "$LOCAL_PORT" ]; then
    return
  fi
  LOCAL_PORT="$(python3 - <<'PY'
import socket

sock = socket.socket()
sock.bind(("127.0.0.1", 0))
print(sock.getsockname()[1])
sock.close()
PY
)"
}

setup_port_forward() {
  resolve_local_port
  echo "[8/9] Forwarding local port tcp:$LOCAL_PORT -> device tcp:$DEVICE_PORT..."
  require_cmd hdc
  hdc fport "tcp:$LOCAL_PORT" "tcp:$DEVICE_PORT"
}

wait_for_local_http() {
  require_cmd curl
  local deadline=$((SECONDS + TEST_TIMEOUT_SECS))
  while [ "$SECONDS" -lt "$deadline" ]; do
    if env -u ALL_PROXY -u all_proxy -u HTTP_PROXY -u http_proxy -u HTTPS_PROXY -u https_proxy \
      curl --noproxy '*' -fsS --max-time 2 "http://127.0.0.1:$LOCAL_PORT/health" \
      >/tmp/rong_harmony_health.json 2>/dev/null; then
      echo "  local health check is ready on tcp:$LOCAL_PORT"
      return 0
    fi
    sleep 1
  done
  echo "Timed out waiting for local curl health check on tcp:$LOCAL_PORT" >&2
  exit 2
}

run_tests_over_http() {
  echo "[9/9] Running Harmony tests over HTTP..."
  require_cmd curl
  local payload_file
  local report_file
  payload_file="$(mktemp)"
  report_file="$(mktemp)"
  trap 'rm -f "$payload_file" "$report_file" /tmp/rong_harmony_health.json' RETURN

  python3 - "$TEST_FILTER" "$payload_file" <<'PY'
import json
import sys

payload = {"filter": sys.argv[1]}
with open(sys.argv[2], "w", encoding="utf-8") as fh:
    json.dump(payload, fh)
PY

  env -u ALL_PROXY -u all_proxy -u HTTP_PROXY -u http_proxy -u HTTPS_PROXY -u https_proxy \
  curl --noproxy '*' -fsS \
    -H 'Content-Type: application/json' \
    --max-time "$TEST_TIMEOUT_SECS" \
    --data-binary "@$payload_file" \
    "http://127.0.0.1:$LOCAL_PORT/run" \
    -o "$report_file"

  python3 - "$report_file" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as fh:
    report = json.load(fh)

if "error" in report and not report.get("ok", False):
    print(f"Test runner error: {report['error']}", file=sys.stderr)
    sys.exit(2)

print(
    "Test summary: "
    f"filter={report.get('filter', '') or '<all>'} "
    f"total={report.get('total', 0)} "
    f"passed={report.get('passed', 0)} "
    f"failed={report.get('failed', 0)} "
    f"crashed={report.get('crashed', 0)} "
    f"elapsed={report.get('elapsed_ms', 0)}ms"
)

for case in report.get("cases", []):
    status = str(case.get("status", "")).upper()
    name = case.get("name", "<unknown>")
    elapsed = case.get("elapsed_ms", 0)
    error = case.get("error")
    if error:
        print(f"  {status:5} {name} ({elapsed} ms): {error}")
    else:
        print(f"  {status:5} {name} ({elapsed} ms)")

failed = int(report.get("failed", 0))
crashed = int(report.get("crashed", 0))
sys.exit(0 if failed == 0 and crashed == 0 else 1)
PY
}

build_rust

if [ "$DO_PACKAGE" = true ]; then
  check_signing_config
  install_app_deps
  build_app
else
  echo "Rust-only mode complete"
  exit 0
fi

if [ "$DO_INSTALL" = true ]; then
  install_app
else
  echo "Build complete: $HAP_PATH"
  exit 0
fi

if [ "$DO_START" = true ]; then
  stop_old_app
  start_app
  resolve_app_pid
  wait_for_device_server
  setup_port_forward
  wait_for_local_http
  run_tests_over_http
else
  echo "Install complete"
  exit 0
fi

echo "Done."
