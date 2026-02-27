#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
declare -a CORE_TESTS=("eval" "error" "function" "promise" "class" "macro")
TEST_FILTER=""

usage() {
    cat <<'EOF'
Usage:
  ./scripts/parity_jscore_provider.sh [--test <name>]

Examples:
  ./scripts/parity_jscore_provider.sh
  ./scripts/parity_jscore_provider.sh --test eval

Runs the selected core tests twice:
  1) jscore
  2) jscore-provider-webkit
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --test)
            TEST_FILTER="$2"
            shift 2
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

cd "$ROOT_DIR"

if [[ -n "$TEST_FILTER" ]]; then
    CORE_TESTS=("$TEST_FILTER")
fi

if [[ -z "${RONG_JSC_WEBKIT_ROOT:-}" && -f "target/webkit-provider/env.sh" ]]; then
    # shellcheck source=/dev/null
    source "target/webkit-provider/env.sh"
fi

if [[ -z "${RONG_JSC_WEBKIT_ROOT:-}" ]]; then
    echo "RONG_JSC_WEBKIT_ROOT is required for jscore-provider-webkit parity tests" >&2
    echo "Run ./scripts/build_webkit_provider.sh first, or export RONG_JSC_WEBKIT_ROOT manually." >&2
    exit 1
fi

if [[ -z "${RONG_JSC_WEBKIT_LINK_KIND:-}" ]]; then
    if [[ "$(uname -s)" == "Darwin" ]]; then
        export RONG_JSC_WEBKIT_LINK_KIND="framework"
    else
        export RONG_JSC_WEBKIT_LINK_KIND="dylib"
    fi
fi

echo "== Running parity core tests: jscore =="
for test_name in "${CORE_TESTS[@]}"; do
    cargo test --test "$test_name" --no-default-features --features jscore --quiet
done

echo
echo "== Running parity core tests: jscore-provider-webkit =="
for test_name in "${CORE_TESTS[@]}"; do
    cargo test --test "$test_name" --no-default-features --features jscore-provider-webkit --quiet
done

echo
echo "Parity tests passed for: ${CORE_TESTS[*]}"
