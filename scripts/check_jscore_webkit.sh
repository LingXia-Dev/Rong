#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WEBKIT_ROOT_DEFAULT="$ROOT_DIR/third_party/WebKit"
PROVIDER_ENV_DEFAULT="$ROOT_DIR/target/webkit-provider/env.sh"

usage() {
    cat <<'EOF'
Usage:
  ./scripts/check_jscore_webkit.sh [webkit_root] [cargo args...]
  ./scripts/check_jscore_webkit.sh [cargo args...]

Examples:
  ./scripts/check_jscore_webkit.sh
  ./scripts/check_jscore_webkit.sh /abs/path/to/WebKit
  ./scripts/check_jscore_webkit.sh /abs/path/to/WebKit cargo test -p rong --no-default-features --features jscore-provider-webkit

Notes:
  - Loads target/webkit-provider/env.sh when available.
  - Exports RONG_JSC_WEBKIT_ROOT automatically.
  - Defaults RONG_JSC_WEBKIT_LINK_KIND to framework on macOS and dylib otherwise.
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" || "${1:-}" == "help" ]]; then
    usage
    exit 0
fi

if [[ -z "${RONG_JSC_WEBKIT_ROOT:-}" && -f "$PROVIDER_ENV_DEFAULT" ]]; then
    # shellcheck source=/dev/null
    source "$PROVIDER_ENV_DEFAULT"
fi

if [[ -z "${RONG_JSC_WEBKIT_ROOT:-}" ]]; then
    webkit_root="$WEBKIT_ROOT_DEFAULT"
    if [[ "${1:-}" != "" && "${1:-}" != "cargo" ]]; then
        webkit_root="$1"
        shift
    fi
    export RONG_JSC_WEBKIT_ROOT="$webkit_root"
fi

if [[ ! -d "$RONG_JSC_WEBKIT_ROOT" ]]; then
    echo "WebKit root not found: $RONG_JSC_WEBKIT_ROOT" >&2
    exit 1
fi

if [[ -z "${RONG_JSC_WEBKIT_INCLUDE_DIR:-}" ]]; then
    if [[ ! -f "$RONG_JSC_WEBKIT_ROOT/JavaScript.h" && \
          ! -f "$RONG_JSC_WEBKIT_ROOT/JavaScriptCore/JavaScript.h" && \
          ! -f "$RONG_JSC_WEBKIT_ROOT/include/JavaScript.h" && \
          ! -f "$RONG_JSC_WEBKIT_ROOT/include/JavaScriptCore/JavaScript.h" && \
          ! -f "$RONG_JSC_WEBKIT_ROOT/Headers/JavaScript.h" && \
          ! -f "$RONG_JSC_WEBKIT_ROOT/JavaScriptCore.framework/Headers/JavaScript.h" && \
          ! -f "$RONG_JSC_WEBKIT_ROOT/Frameworks/JavaScriptCore.framework/Headers/JavaScript.h" && \
          ! -f "$RONG_JSC_WEBKIT_ROOT/WebKitBuild/Release/JavaScriptCore.framework/Headers/JavaScript.h" && \
          ! -f "$RONG_JSC_WEBKIT_ROOT/WebKitBuild/Debug/JavaScriptCore.framework/Headers/JavaScript.h" ]]; then
        echo "No JavaScriptCore headers detected under RONG_JSC_WEBKIT_ROOT=$RONG_JSC_WEBKIT_ROOT" >&2
        echo "Run ./scripts/build_webkit_provider.sh first, or pass a built provider root (e.g. macOS SDK System/Library)." >&2
        exit 1
    fi
fi

if [[ -z "${RONG_JSC_WEBKIT_LINK_KIND:-}" ]]; then
    if [[ "$(uname -s)" == "Darwin" ]]; then
        export RONG_JSC_WEBKIT_LINK_KIND="framework"
    else
        export RONG_JSC_WEBKIT_LINK_KIND="dylib"
    fi
fi

if [[ $# -eq 0 ]]; then
    set -- cargo check -p rong --no-default-features --features jscore-provider-webkit
fi

echo "RONG_JSC_WEBKIT_ROOT=$RONG_JSC_WEBKIT_ROOT"
echo "RONG_JSC_WEBKIT_LINK_KIND=$RONG_JSC_WEBKIT_LINK_KIND"
if [[ -n "${RONG_JSC_WEBKIT_INCLUDE_DIR:-}" ]]; then
    echo "RONG_JSC_WEBKIT_INCLUDE_DIR=$RONG_JSC_WEBKIT_INCLUDE_DIR"
fi
if [[ -n "${RONG_JSC_WEBKIT_LIB_DIR:-}" ]]; then
    echo "RONG_JSC_WEBKIT_LIB_DIR=$RONG_JSC_WEBKIT_LIB_DIR"
fi
echo "Running: $*"
"$@"
