#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SUBMODULE_PATH="third_party/WebKit"

usage() {
    cat <<'EOF'
Usage:
  ./scripts/webkit_submodule.sh init
  ./scripts/webkit_submodule.sh bump
  ./scripts/webkit_submodule.sh status

Commands:
  init    Initialize/update the WebKit submodule to the pinned commit
  bump    Update submodule to latest remote tip (main), then show new commit
  status  Print current submodule commit and remotes
EOF
}

cmd="${1:-init}"

cd "$ROOT_DIR"

case "$cmd" in
    init)
        git submodule update --init --depth 1 -- "$SUBMODULE_PATH"
        ;;
    bump)
        git submodule update --init --remote --depth 1 -- "$SUBMODULE_PATH"
        ;;
    status)
        ;;
    -h|--help|help)
        usage
        exit 0
        ;;
    *)
        echo "Unknown command: $cmd" >&2
        usage >&2
        exit 1
        ;;
esac

echo "== WebKit submodule status =="
git submodule status -- "$SUBMODULE_PATH"
echo
echo "== WebKit commit =="
git -C "$SUBMODULE_PATH" rev-parse HEAD
