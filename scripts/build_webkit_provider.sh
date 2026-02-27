#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WEBKIT_ROOT="$ROOT_DIR/third_party/WebKit"
BUILD_DIR=""
CONFIGURATION="release"
DRY_RUN=0
INIT_SUBMODULE=0
MAKE_ARGS=""
declare -a CMAKE_ARGS=()

usage() {
    cat <<'EOF'
Usage:
  ./scripts/build_webkit_provider.sh [options]

Options:
  --webkit-root <path>    WebKit source root (default: ./third_party/WebKit)
  --build-dir <path>      build-jsc --build-dir path (default: <webkit-root>/WebKitBuild)
  --release               Build release configuration (default)
  --debug                 Build debug configuration
  --makeargs <args>       Forwarded to build-jsc --makeargs
  --cmakeargs <args>      Forwarded to build-jsc --cmakeargs (repeatable)
  --init-submodule        Run webkit_submodule.sh init before build
  --dry-run               Print resolved command/config without building
  -h, --help              Show this help

Outputs:
  - Builds JavaScriptCore from WebKit source
  - Writes provider env file to target/webkit-provider/env.sh
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --webkit-root)
            WEBKIT_ROOT="$2"
            shift 2
            ;;
        --build-dir)
            BUILD_DIR="$2"
            shift 2
            ;;
        --release)
            CONFIGURATION="release"
            shift
            ;;
        --debug)
            CONFIGURATION="debug"
            shift
            ;;
        --makeargs)
            MAKE_ARGS="$2"
            shift 2
            ;;
        --cmakeargs)
            CMAKE_ARGS+=("$2")
            shift 2
            ;;
        --init-submodule)
            INIT_SUBMODULE=1
            shift
            ;;
        --dry-run)
            DRY_RUN=1
            shift
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

if [[ "$INIT_SUBMODULE" -eq 1 ]]; then
    "$ROOT_DIR/scripts/webkit_submodule.sh" init
fi

if [[ ! -d "$WEBKIT_ROOT" ]]; then
    echo "WebKit root not found: $WEBKIT_ROOT" >&2
    exit 1
fi

if [[ -z "$BUILD_DIR" ]]; then
    BUILD_DIR="$WEBKIT_ROOT/WebKitBuild"
fi

BUILD_SCRIPT="$WEBKIT_ROOT/Tools/Scripts/build-jsc"
if [[ ! -x "$BUILD_SCRIPT" ]]; then
    echo "WebKit build script not found or not executable: $BUILD_SCRIPT" >&2
    exit 1
fi

if [[ "$(uname -s)" == "Darwin" ]]; then
    if ! xcodebuild -version >/dev/null 2>&1; then
        echo "Full Xcode is required for build-jsc on macOS (xcodebuild unavailable)." >&2
        echo "Install Xcode.app and run: sudo xcode-select -s /Applications/Xcode.app/Contents/Developer" >&2
        exit 1
    fi
fi

declare -a BUILD_CMD=(
    "$BUILD_SCRIPT"
)

if [[ "$(uname -s)" != "Darwin" ]]; then
    BUILD_CMD+=("--build-dir=$BUILD_DIR")
fi

if [[ "$CONFIGURATION" == "release" ]]; then
    BUILD_CMD+=("--release")
else
    BUILD_CMD+=("--debug")
fi

if [[ -n "$MAKE_ARGS" ]]; then
    BUILD_CMD+=("--makeargs=$MAKE_ARGS")
fi

for cmake_args in "${CMAKE_ARGS[@]}"; do
    BUILD_CMD+=("--cmakeargs=$cmake_args")
done

echo "WebKit root: $WEBKIT_ROOT"
echo "Build dir:   $BUILD_DIR"
echo "Config:      $CONFIGURATION"
echo "Command:     ${BUILD_CMD[*]}"

if [[ "$DRY_RUN" -eq 1 ]]; then
    exit 0
fi

(cd "$WEBKIT_ROOT" && "${BUILD_CMD[@]}")

case "$CONFIGURATION" in
    release) CONFIG_DIR_NAME="Release" ;;
    debug) CONFIG_DIR_NAME="Debug" ;;
    *)
        echo "Unsupported configuration: $CONFIGURATION" >&2
        exit 1
        ;;
esac

declare -a INCLUDE_CANDIDATES=(
    "$BUILD_DIR/$CONFIG_DIR_NAME/JavaScriptCore.framework/Headers"
    "$BUILD_DIR/$CONFIG_DIR_NAME/include"
    "$BUILD_DIR/include"
)

declare -a LIB_CANDIDATES=(
    "$BUILD_DIR/$CONFIG_DIR_NAME"
    "$BUILD_DIR/$CONFIG_DIR_NAME/lib"
    "$BUILD_DIR/lib"
)

INCLUDE_DIR=""
for candidate in "${INCLUDE_CANDIDATES[@]}"; do
    if [[ -f "$candidate/JavaScript.h" || -f "$candidate/JavaScriptCore/JavaScript.h" ]]; then
        INCLUDE_DIR="$candidate"
        break
    fi
done

LIB_DIR=""
for candidate in "${LIB_CANDIDATES[@]}"; do
    if [[ -d "$candidate" ]]; then
        LIB_DIR="$candidate"
        break
    fi
done

if [[ -z "$INCLUDE_DIR" ]]; then
    echo "Unable to locate JavaScriptCore headers after build." >&2
    echo "Checked: ${INCLUDE_CANDIDATES[*]}" >&2
    exit 1
fi

if [[ -z "$LIB_DIR" ]]; then
    echo "Unable to locate library/framework directory after build." >&2
    echo "Checked: ${LIB_CANDIDATES[*]}" >&2
    exit 1
fi

if [[ "$(uname -s)" == "Darwin" ]]; then
    DEFAULT_LINK_KIND="framework"
else
    DEFAULT_LINK_KIND="dylib"
fi

ENV_OUT_DIR="$ROOT_DIR/target/webkit-provider"
ENV_OUT_FILE="$ENV_OUT_DIR/env.sh"
mkdir -p "$ENV_OUT_DIR"

cat > "$ENV_OUT_FILE" <<EOF
#!/usr/bin/env bash
export RONG_JSC_WEBKIT_ROOT="$WEBKIT_ROOT"
export RONG_JSC_WEBKIT_INCLUDE_DIR="$INCLUDE_DIR"
export RONG_JSC_WEBKIT_LIB_DIR="$LIB_DIR"
export RONG_JSC_WEBKIT_LINK_KIND="\${RONG_JSC_WEBKIT_LINK_KIND:-$DEFAULT_LINK_KIND}"
EOF

chmod +x "$ENV_OUT_FILE"

echo
echo "Generated provider env file: $ENV_OUT_FILE"
echo "To use it manually:"
echo "  source \"$ENV_OUT_FILE\""
echo "  cargo check -p rong --no-default-features --features jscore-provider-webkit"
