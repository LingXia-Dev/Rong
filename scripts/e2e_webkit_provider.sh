#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SUBMODULE_MODE="init"
BUILD_CONFIGURATION="release"
SKIP_BUILD=0
declare -a CHECK_CMD=()

usage() {
    cat <<'EOF'
Usage:
  ./scripts/e2e_webkit_provider.sh [options] [-- <cargo command...>]

Options:
  --init            Initialize pinned submodule commit (default)
  --bump            Update submodule to latest main before build/check
  --debug           Build debug JavaScriptCore
  --release         Build release JavaScriptCore (default)
  --skip-build      Skip build step and only run check
  -h, --help        Show this help

Examples:
  ./scripts/e2e_webkit_provider.sh
  ./scripts/e2e_webkit_provider.sh --bump
  ./scripts/e2e_webkit_provider.sh -- cargo test -p rong --no-default-features --features jscore-provider-webkit
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --init)
            SUBMODULE_MODE="init"
            shift
            ;;
        --bump)
            SUBMODULE_MODE="bump"
            shift
            ;;
        --debug)
            BUILD_CONFIGURATION="debug"
            shift
            ;;
        --release)
            BUILD_CONFIGURATION="release"
            shift
            ;;
        --skip-build)
            SKIP_BUILD=1
            shift
            ;;
        --)
            shift
            CHECK_CMD=("$@")
            break
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

echo "== Step 1: submodule $SUBMODULE_MODE =="
"$ROOT_DIR/scripts/webkit_submodule.sh" "$SUBMODULE_MODE"

if [[ "$SKIP_BUILD" -eq 0 ]]; then
    echo
    echo "== Step 2: build WebKit provider ($BUILD_CONFIGURATION) =="
    "$ROOT_DIR/scripts/build_webkit_provider.sh" "--$BUILD_CONFIGURATION"
fi

echo
echo "== Step 3: validate jscore-provider-webkit =="
if [[ "${#CHECK_CMD[@]}" -eq 0 ]]; then
    "$ROOT_DIR/scripts/check_jscore_webkit.sh"
else
    "$ROOT_DIR/scripts/check_jscore_webkit.sh" "${CHECK_CMD[@]}"
fi
