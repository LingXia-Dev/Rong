#!/usr/bin/env bash
#
# build_jsc_artifact.sh — build a JSCOnly artifact for the rong source backend.
#
# Shallow-clones UPSTREAM WebKit (https://github.com/WebKit/WebKit), builds
# JavaScriptCore in Release, and installs
# a normalized artifact:
#
#   <out>/include/JavaScriptCore/JavaScript.h        (public C API headers)
#   <out>/include/JavaScriptCore/private/JavaScriptCore/
#                                                       (private headers — bytecode)
#   <out>/include/WTF/  <out>/include/bmalloc/        (transitive private headers)
#   <out>/lib/JavaScriptCore.framework               (Apple)  -- or --
#   <out>/lib/lib{JavaScriptCore,WTF,bmalloc}.a       (Linux, static)
#   <out>/lib/lib{icui18n,icuuc,icudata}.a            (Linux, static ICU)
#
# Windows/MSVC source artifacts use clang-cl + vcpkg static ICU and are
# smoke-tested from the packaged artifact before their TSV rows are pinned.
#
# That layout is exactly what `rong_jscore_sys/build.rs` auto-detects in the
# per-target cache, so afterwards `RONG_JSC_SOURCE=1 cargo build` needs no env
# var. See javascriptcore/sys/README.md.
#
# NOTE: building WebKit takes a long time and needs full Xcode (macOS) or a
# CMake/clang/ICU/ruby toolchain (Linux). This script is exercised by CI
# (.github/workflows/build-jsc-artifacts.yml); run it locally only if you want a
# local artifact.

set -euo pipefail

WEBKIT_URL="${WEBKIT_URL:-https://github.com/WebKit/WebKit.git}"
# PIN ME: set to a known-good WebKit tag for reproducible artifacts. `main`
# tracks tip-of-tree and is not reproducible; override with --webkit-ref.
DEFAULT_WEBKIT_REF="main"

# Static ICU version built from source for non-Apple targets (distro libicu-dev
# is shared-only). Override with the ICU_VERSION env var.
ICU_VERSION="${ICU_VERSION:-74.2}"

WEBKIT_REF="$DEFAULT_WEBKIT_REF"
WEBKIT_ROOT=""        # reuse an existing checkout instead of cloning
TARGET=""             # default: host triple from rustc
OUT_DIR=""            # default: per-target cache
PACKAGE=""            # if set, also write this .tar.gz
JOBS=""
DRY_RUN=0
CHECK_ONLY=0

usage() {
    cat <<'EOF'
Usage: ./scripts/build_jsc_artifact.sh [options]

Options:
  --webkit-ref <ref>    WebKit tag/branch/sha to build (default: main; pin for releases)
  --webkit-root <path>  Use an existing WebKit checkout (skip clone)
  --target <triple>     Target triple (default: host triple from rustc)
  --out <dir>           Install dir (default: <cache>/<target>; then set RONG_JSC_ROOT=<dir>)
  --package <file.tgz>  Also emit a distributable tarball of the install tree
  --jobs <N>            Parallel build jobs
  --dry-run             Print the plan and exit
  --check               Validate host tools/environment and exit before cloning
  -h, --help            Show this help

The cache base is $RONG_JSC_CACHE_DIR, else $XDG_CACHE_HOME/rong/webkit,
else ~/.cache/rong/webkit.
EOF
}

die() {
    echo "ERROR: $*" >&2
    exit 1
}

find_rustc() {
    if command -v rustc >/dev/null 2>&1; then
        command -v rustc
    elif command -v rustc.exe >/dev/null 2>&1; then
        command -v rustc.exe
    else
        return 1
    fi
}

to_shell_path() {
    local path="$1"
    if [[ "${HOST_WINDOWS_BASH:-0}" -eq 1 ]] && command -v cygpath >/dev/null 2>&1; then
        cygpath -u "$path"
    else
        echo "$path"
    fi
}

to_cmake_path() {
    local path="$1"
    if [[ "${HOST_WINDOWS_BASH:-0}" -eq 1 ]] && command -v cygpath >/dev/null 2>&1; then
        cygpath -am "$path"
    else
        echo "${path//\\//}"
    fi
}

prepend_path_dir() {
    local dir="$1"
    [[ -d "$dir" ]] || return 0
    case ":$PATH:" in
        *":$dir:"*) ;;
        *) PATH="$dir:$PATH" ;;
    esac
}

msvc_host_dir() {
    case "${1,,}" in
        x64|amd64) echo "HostX64" ;;
        x86) echo "HostX86" ;;
        arm64) echo "HostARM64" ;;
        *) echo "Host$1" ;;
    esac
}

msvc_target_dir() {
    case "${1,,}" in
        x64|amd64) echo "x64" ;;
        x86) echo "x86" ;;
        arm64) echo "arm64" ;;
        *) echo "$1" ;;
    esac
}

msvc_tools_bin_dir() {
    [[ -n "${VCToolsInstallDir:-}" ]] || return 1

    local root host target candidate
    root="${VCToolsInstallDir%\\}"
    root="${root%/}"
    host="$(msvc_host_dir "${VSCMD_ARG_HOST_ARCH:-x64}")"
    target="$(msvc_target_dir "${VSCMD_ARG_TGT_ARCH:-x64}")"
    candidate="$(to_shell_path "$root/bin/$host/$target")"
    [[ -d "$candidate" ]] || return 1
    echo "$candidate"
}

prepend_msvc_tools_path() {
    [[ "${HOST_WINDOWS_BASH:-0}" -eq 1 ]] || return 0

    local dir
    dir="$(msvc_tools_bin_dir || true)"
    [[ -n "$dir" ]] && prepend_path_dir "$dir"
}

find_msvc_linker() {
    local dir
    dir="$(msvc_tools_bin_dir || true)"
    [[ -n "$dir" && -f "$dir/link.exe" ]] || return 1
    to_cmake_path "$dir/link.exe"
}

bootstrap_windows_path() {
    [[ "${HOST_WINDOWS_BASH:-0}" -eq 1 ]] || return 0

    prepend_msvc_tools_path

    local dir
    for dir in \
        "C:/Program Files/LLVM/bin" \
        "C:/Program Files/CMake/bin" \
        "C:/Program Files/GnuWin32/bin" \
        "C:/GnuWin32/bin" \
        "C:/Ruby32-x64/bin" \
        "C:/Ruby34-x64/bin" \
        "C:/Ruby33-x64/bin" \
        "C:/Python311" \
        "C:/Program Files/Python311"; do
        prepend_path_dir "$(to_shell_path "$dir")"
    done

    if [[ -n "${LOCALAPPDATA:-}" ]]; then
        local local_appdata
        local_appdata="$(to_shell_path "$LOCALAPPDATA")"
        for dir in \
            "$local_appdata/Programs/Python/Python311" \
            "$local_appdata/Programs/Python/Python311/Scripts" \
            "$local_appdata/Microsoft/WinGet/Packages"/oss-winget.gperf*; do
            prepend_path_dir "$dir"
        done
    fi
}

require_command() {
    local cmd="$1"
    local label="${2:-$1}"
    if ! command -v "$cmd" >/dev/null 2>&1; then
        MISSING_TOOLS+=("$label")
    fi
}

require_working_python() {
    local python_cmd="${PYTHON:-python}"
    if ! command -v "$python_cmd" >/dev/null 2>&1; then
        MISSING_TOOLS+=("${python_cmd} (Python 3.11 recommended)")
        return
    fi
    if ! "$python_cmd" - <<'PY' >/dev/null 2>&1
import sys
raise SystemExit(0 if sys.version_info >= (3, 8) else 1)
PY
    then
        MISSING_TOOLS+=("${python_cmd} (working Python 3, not the Windows Store shim)")
    fi
}

report_missing_tools() {
    local heading="$1"
    if [[ "${#MISSING_TOOLS[@]}" -eq 0 ]]; then
        return 0
    fi

    echo "$heading" >&2
    local tool
    for tool in "${MISSING_TOOLS[@]}"; do
        echo "  - $tool" >&2
    done
    return 1
}

clone_webkit() {
    local ref="$1"
    local dest="$2"
    local tmp="$dest.tmp"

    rm -rf "$dest" "$tmp"

    local clone_args=(
        -c core.longpaths=true
        clone
        --depth 1
        --filter=blob:none
        --no-checkout
    )
    if [[ "$ref" =~ ^[0-9a-fA-F]{40}$ ]]; then
        git "${clone_args[@]}" "$WEBKIT_URL" "$tmp"
        git -C "$tmp" fetch --depth 1 origin "$ref"
    else
        clone_args+=(--branch "$ref")
        if ! git "${clone_args[@]}" "$WEBKIT_URL" "$tmp"; then
            rm -rf "$tmp"
            die "failed to clone WebKit ref: $ref"
        fi
    fi

    if [[ ! -d "$tmp" ]]; then
        rm -rf "$tmp"
        die "failed to prepare WebKit checkout for ref: $ref"
    fi

    git -C "$tmp" config core.longpaths true
    git -C "$tmp" sparse-checkout init --cone
    git -C "$tmp" sparse-checkout set \
        CMake \
        JSTests \
        PerformanceTests/JetStream2 \
        PerformanceTests/SunSpider \
        PerformanceTests/V8Spider \
        Source \
        Tools \
        WebKitLibraries \
        Websites/browserbench.org
    git -C "$tmp" checkout --detach "$ref"
    mv "$tmp" "$dest"
}

patch_webkit_for_artifact_build() {
    local webkit_root="$1"
    local subproject="$webkit_root/Tools/Scripts/webkitperl/BuildSubproject.pm"

    [[ -f "$subproject" ]] || return 0
    if grep -q 'RONG_JSC_ARTIFACT_BUILD_TARGETS' "$subproject"; then
        return 0
    fi

    echo "==> Patching WebKit build-jsc artifact targets"
    perl -0pi -e 's/\$buildTarget = "jsc testb3 testair testapi testmasm testdfg testwasmdebugger \$makeArgs";/\$buildTarget = "JavaScriptCore \$makeArgs"; # RONG_JSC_ARTIFACT_BUILD_TARGETS/s;
        s/\$buildTarget \.= "jsc testapi testmasm";/\$buildTarget .= "JavaScriptCore \$makeArgs"; # RONG_JSC_ARTIFACT_BUILD_TARGETS/s' "$subproject"
    if ! grep -q 'RONG_JSC_ARTIFACT_BUILD_TARGETS' "$subproject"; then
        die "failed to patch WebKit build-jsc targets in $subproject"
    fi
}

patch_webkit_for_windows() {
    local webkit_root="$1"
    local macros="$webkit_root/Source/cmake/WebKitMacros.cmake"
    local helper="$webkit_root/Source/cmake/RongWriteForwardingHeader.cmake"

    [[ -f "$macros" ]] || return 0
    cat > "$helper" <<'EOF'
file(TO_CMAKE_PATH "${src_file}" wrapper_src_file)
file(WRITE "${dst_file}" "#include \"${wrapper_src_file}\"\n")
EOF

    if grep -q 'RONG_JSC_WINDOWS_WRAPPER_RULE_HEADERS' "$macros"; then
        return 0
    fi

    echo "==> Patching WebKit forwarding headers to use wrappers on Windows"
    perl -0pi -e 'BEGIN { $r = q~# RONG_JSC_WINDOWS_WRAPPER_RULE_HEADERS: symlinks need privileges; wrappers preserve pragma-once identity.
        add_custom_command(OUTPUT ${dst_file}
            COMMAND ${CMAKE_COMMAND} "-Dsrc_file=${src_file}" "-Ddst_file=${dst_file}" -P "${CMAKE_SOURCE_DIR}/Source/cmake/RongWriteForwardingHeader.cmake"
            MAIN_DEPENDENCY ${src_file}
            VERBATIM
        )~; } s/(function\(WEBKIT_SYMLINK_FILES target_name\).*?)add_custom_command\(OUTPUT \$\{dst_file\}\s*\n\s*(?:# RONG_JSC_WINDOWS_[^\n]*\n\s*)?COMMAND \$\{CMAKE_COMMAND\} -E (?:create_symlink|copy_if_different|create_hardlink) \$\{src_file\} \$\{dst_file\}\s*\n\s*MAIN_DEPENDENCY \$\{file\}\s*\n\s*VERBATIM\s*\)/$1$r/s' "$macros"
    if ! grep -q 'RONG_JSC_WINDOWS_WRAPPER_RULE_HEADERS' "$macros"; then
        die "failed to patch WebKit forwarding headers in $macros"
    fi

    if [[ -d "$webkit_root/WebKitBuild/JSCOnly" ]]; then
        echo "==> Removing stale Windows JSC build tree after forwarding-header patch"
        rm -rf "$webkit_root/WebKitBuild/JSCOnly"
    fi
}

refresh_windows_icu_cache() {
    local webkit_root="$1"
    local cache="$webkit_root/WebKitBuild/JSCOnly/Release/CMakeCache.txt"
    [[ -f "$cache" ]] || return 0

    local stale=0
    if grep -q '^ICU_ROOT:' "$cache"; then
        stale=1
    fi
    if [[ -n "${VCPKG_ROOT_CMAKE:-}" && -n "${VCPKG_TRIPLET:-}" ]] \
        && grep -Fq "$VCPKG_ROOT_CMAKE/installed/$VCPKG_TRIPLET/lib/icu" "$cache"; then
        stale=1
    fi
    if [[ -n "${VCPKG_TRIPLET:-}" ]] && grep -q '^VCPKG_TARGET_TRIPLET:' "$cache" \
        && ! grep -q "^VCPKG_TARGET_TRIPLET:STRING=$VCPKG_TRIPLET$" "$cache"; then
        stale=1
    fi
    if grep -Eq '^ICU_(I18N|UC|DATA)_LIBRARY_RELEASE:FILEPATH=.*/Windows Kits/.*/icu.*\.lib$' "$cache"; then
        stale=1
    fi

    if [[ "$stale" -eq 1 ]]; then
        echo "==> Removing stale Windows ICU CMake cache entries"
        rm -f "$cache" "$webkit_root/WebKitBuild/JSCOnly/Release/CMakeFiles/cmake.check_cache"
    fi
}

preflight() {
    MISSING_TOOLS=()

    if [[ -z "$WEBKIT_ROOT" ]]; then
        require_command git
    fi
    if [[ -n "$PACKAGE" ]]; then
        require_command tar
        if ! command -v shasum >/dev/null 2>&1 && ! command -v sha256sum >/dev/null 2>&1; then
            MISSING_TOOLS+=("shasum or sha256sum")
        fi
    fi

    if [[ "$IS_DARWIN" -eq 1 ]]; then
        if [[ "$HOST_DARWIN" -ne 1 ]]; then
            die "Apple JSC artifacts must be built on macOS; target is $TARGET but host shell is $HOST_UNAME."
        fi
        require_command xcodebuild
    elif [[ "$IS_WINDOWS" -eq 1 ]]; then
        if [[ "$HOST_WINDOWS_BASH" -ne 1 ]]; then
            die "Windows JSC artifacts must be built from Git Bash/MSYS on Windows; target is $TARGET but host shell is $HOST_UNAME."
        fi
        for cmd in git curl tar cmake ninja perl python ruby gperf bison flex clang-cl; do
            require_command "$cmd"
        done
        [[ -d "${RONG_JSC_VCPKG_ROOT:-${VCPKG_ROOT:-C:/vcpkg}}" ]] \
            || MISSING_TOOLS+=("vcpkg at ${RONG_JSC_VCPKG_ROOT:-${VCPKG_ROOT:-C:/vcpkg}}")
    else
        if [[ "$HOST_LINUX" -ne 1 ]]; then
            die "Linux JSC artifacts must be built on Linux; target is $TARGET but host shell is $HOST_UNAME."
        fi
        for cmd in git curl tar make cmake clang clang++ perl python3 ruby gperf bison flex; do
            require_command "$cmd"
        done
    fi

    report_missing_tools "Missing required tools/environment for $TARGET:" || exit 1
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --webkit-ref)  WEBKIT_REF="$2"; shift 2 ;;
        --webkit-root) WEBKIT_ROOT="$2"; shift 2 ;;
        --target)      TARGET="$2"; shift 2 ;;
        --out)         OUT_DIR="$2"; shift 2 ;;
        --package)     PACKAGE="$2"; shift 2 ;;
        --jobs)        JOBS="$2"; shift 2 ;;
        --dry-run)     DRY_RUN=1; shift ;;
        --check)       CHECK_ONLY=1; shift ;;
        -h|--help)     usage; exit 0 ;;
        *) echo "Unknown option: $1" >&2; usage >&2; exit 1 ;;
    esac
done

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ -n "$WEBKIT_ROOT" ]]; then
    WEBKIT_ROOT="$(cd "$WEBKIT_ROOT" && pwd)"
fi
if [[ -n "$OUT_DIR" ]]; then
    mkdir -p "$(dirname "$OUT_DIR")"
    OUT_PARENT="$(cd "$(dirname "$OUT_DIR")" && pwd)"
    OUT_DIR="$OUT_PARENT/$(basename "$OUT_DIR")"
fi
if [[ -n "$PACKAGE" ]]; then
    mkdir -p "$(dirname "$PACKAGE")"
    PACKAGE_PARENT="$(cd "$(dirname "$PACKAGE")" && pwd)"
    PACKAGE="$PACKAGE_PARENT/$(basename "$PACKAGE")"
fi

# --- Resolve target + output dir -------------------------------------------
if [[ -z "$TARGET" ]]; then
    RUSTC_BIN="$(find_rustc || true)"
    if [[ -z "$RUSTC_BIN" ]]; then
        echo "rustc not found; pass --target <triple>." >&2
        exit 1
    fi
    TARGET="$("$RUSTC_BIN" -vV | awk '/^host:/{print $2}')"
fi

if [[ -z "$OUT_DIR" ]]; then
    CACHE_BASE="${RONG_JSC_CACHE_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/rong/webkit}"
    # When RONG_JSC_CACHE_DIR is set it already names the base; otherwise we
    # appended rong/webkit above. build.rs joins <base>/<target>.
    if [[ -n "${RONG_JSC_CACHE_DIR:-}" ]]; then
        OUT_DIR="$RONG_JSC_CACHE_DIR/$TARGET"
    else
        OUT_DIR="$CACHE_BASE/$TARGET"
    fi
fi

HOST_UNAME="$(uname -s)"
case "$HOST_UNAME" in
    Darwin)
        HOST_DARWIN=1
        HOST_WINDOWS_BASH=0
        HOST_LINUX=0
        ;;
    MINGW*|MSYS*|CYGWIN*)
        HOST_DARWIN=0
        HOST_WINDOWS_BASH=1
        HOST_LINUX=0
        ;;
    *)
        HOST_DARWIN=0
        HOST_WINDOWS_BASH=0
        HOST_LINUX=1
        ;;
esac
HOST_WSL=0
if [[ "$HOST_LINUX" -eq 1 && -r /proc/version ]] && grep -qi microsoft /proc/version; then
    HOST_WSL=1
fi

case "$TARGET" in
    *-apple-darwin|*-apple-ios|*-apple-tvos|*-apple-watchos)
        TARGET_OS="apple"
        IS_DARWIN=1
        IS_WINDOWS=0
        ;;
    *-pc-windows-*)
        TARGET_OS="windows"
        IS_DARWIN=0
        IS_WINDOWS=1
        ;;
    *linux*)
        TARGET_OS="linux"
        IS_DARWIN=0
        IS_WINDOWS=0
        ;;
    *)
        die "Unsupported JSC artifact target: $TARGET"
        ;;
esac

if [[ "$HOST_WINDOWS_BASH" -eq 1 ]]; then
    [[ -n "$WEBKIT_ROOT" ]] && WEBKIT_ROOT="$(to_shell_path "$WEBKIT_ROOT")"
    [[ -n "$OUT_DIR" ]] && OUT_DIR="$(to_shell_path "$OUT_DIR")"
    [[ -n "$PACKAGE" ]] && PACKAGE="$(to_shell_path "$PACKAGE")"
fi

VCPKG_TRIPLET=""
VCPKG_ROOT_CMAKE=""
VCPKG_ROOT_SHELL=""
if [[ "$IS_WINDOWS" -eq 1 ]]; then
    VCPKG_TRIPLET="${VCPKG_DEFAULT_TRIPLET:-x64-windows-static-md}"
    VCPKG_ROOT_RAW="${RONG_JSC_VCPKG_ROOT:-${VCPKG_ROOT:-C:/vcpkg}}"
    VCPKG_ROOT_CMAKE="$(to_cmake_path "$VCPKG_ROOT_RAW")"
    VCPKG_ROOT_SHELL="$(to_shell_path "$VCPKG_ROOT_RAW")"
    bootstrap_windows_path
fi

echo "WebKit URL:  $WEBKIT_URL"
echo "WebKit ref:  $WEBKIT_REF"
echo "Target:      $TARGET"
echo "Target OS:   $TARGET_OS"
echo "Host shell:  $HOST_UNAME"
echo "Install to:  $OUT_DIR"
[[ -n "$PACKAGE" ]] && echo "Package:     $PACKAGE"
if [[ "$WEBKIT_REF" == "main" && -z "$WEBKIT_ROOT" ]]; then
    echo "WARNING: building from 'main' (tip-of-tree) is not reproducible; pass --webkit-ref <tag>." >&2
fi
[[ "$DRY_RUN" -eq 1 ]] && exit 0

preflight
if [[ "$CHECK_ONLY" -eq 1 ]]; then
    echo "Preflight OK for $TARGET"
    exit 0
fi

# --- Obtain WebKit source ---------------------------------------------------
if [[ -z "$WEBKIT_ROOT" ]]; then
    WEBKIT_ROOT="$ROOT_DIR/target/webkit-src"
    if [[ ! -d "$WEBKIT_ROOT/.git" ]]; then
        echo "==> Shallow-cloning WebKit ($WEBKIT_REF)"
        clone_webkit "$WEBKIT_REF" "$WEBKIT_ROOT"
    fi
    # If --webkit-ref is a sha, fetch+checkout it explicitly.
    if ! git -C "$WEBKIT_ROOT" rev-parse --verify --quiet "$WEBKIT_REF^{commit}" >/dev/null; then
        git -C "$WEBKIT_ROOT" fetch --depth 1 origin "$WEBKIT_REF"
        git -C "$WEBKIT_ROOT" checkout --detach FETCH_HEAD
    fi
fi
patch_webkit_for_artifact_build "$WEBKIT_ROOT"
if [[ "$IS_WINDOWS" -eq 1 ]]; then
    patch_webkit_for_windows "$WEBKIT_ROOT"
    refresh_windows_icu_cache "$WEBKIT_ROOT"
fi

BUILD_SCRIPT="$WEBKIT_ROOT/Tools/Scripts/build-jsc"
if [[ "$IS_WINDOWS" -eq 1 ]]; then
    [[ -f "$BUILD_SCRIPT" ]] || { echo "Not found: $BUILD_SCRIPT" >&2; exit 1; }
else
    [[ -x "$BUILD_SCRIPT" ]] || { echo "Not found or not executable: $BUILD_SCRIPT" >&2; exit 1; }
fi

if [[ "$IS_DARWIN" -eq 1 ]] && ! xcodebuild -version >/dev/null 2>&1; then
    echo "Full Xcode required for build-jsc on macOS (xcodebuild unavailable)." >&2
    echo "Install Xcode.app and: sudo xcode-select -s /Applications/Xcode.app/Contents/Developer" >&2
    exit 1
fi

# --- Build ------------------------------------------------------------------
BUILD_DIR="$WEBKIT_ROOT/WebKitBuild"
BUILD_DIR_CMAKE="$BUILD_DIR"
if [[ "$IS_WINDOWS" -eq 1 ]]; then
    BUILD_DIR_CMAKE="$(to_cmake_path "$BUILD_DIR")"
fi

# Build ICU as static archives so the artifact can ship libicu{i18n,uc,data}.a
# that build.rs links with `static=` (distro libicu-dev is shared-only). Sets
# ICU_PREFIX. Apple uses the system ICU (icucore) and skips this.
ICU_PREFIX=""
build_static_icu() {
    ICU_PREFIX="$BUILD_DIR/icu-static"
    [[ -f "$ICU_PREFIX/lib/libicuuc.a" ]] && return
    local tag="release-${ICU_VERSION//./-}"      # 74.2 -> release-74-2
    local file="icu4c-${ICU_VERSION//./_}-src.tgz" # 74.2 -> icu4c-74_2-src.tgz
    local url="https://github.com/unicode-org/icu/releases/download/${tag}/${file}"
    local src="$BUILD_DIR/icu-src"
    echo "==> Building static ICU $ICU_VERSION" >&2
    rm -rf "$src"; mkdir -p "$src"
    (
        cd "$src"
        curl -fL --retry 8 --retry-all-errors --retry-delay 10 --connect-timeout 30 --max-time 600 \
            "$url" -o icu.tgz
        tar xzf icu.tgz
    ) >&2
    ( cd "$src/icu/source" \
        && ./configure --prefix="$ICU_PREFIX" --enable-static --disable-shared \
             --disable-tests --disable-samples \
        && make -j"${JOBS:-$(command -v nproc >/dev/null 2>&1 && nproc || echo 4)}" \
        && make install ) >&2
}

declare -a BUILD_CMD=("$BUILD_SCRIPT" "--jsc-only" "--release")
if [[ "$IS_WINDOWS" -eq 1 ]]; then
    BUILD_CMD=("perl" "$BUILD_SCRIPT" "--jsc-only" "--release")
fi
# Skip the TestWebKitAPI/TestWTF target - it's irrelevant here and trips a
# macOS "Objective-C disabled in PCH" quirk on JSCOnly.
BUILD_CMD+=("--cmakeargs=-DENABLE_API_TESTS=OFF")
# build-jsc enables DEVELOPER_MODE by default on JSCOnly. For artifact builds
# that pulls in PerformanceTests from partial/shallow WebKit checkouts, which is
# not needed and can fail before JavaScriptCore is configured.
BUILD_CMD+=("--cmakeargs=-DDEVELOPER_MODE=OFF")
[[ -n "$JOBS" ]] && BUILD_CMD+=("--makeargs=-j$JOBS")

if [[ "$IS_DARWIN" -eq 0 ]]; then
    BUILD_CMD+=("--build-dir=$BUILD_DIR_CMAKE")
    # Static JSC/WTF/bmalloc matches build.rs and keeps normal Cargo builds from
    # depending on a large local WebKit build tree.
    BUILD_CMD+=("--cmakeargs=-DENABLE_STATIC_JSC=ON")
    if [[ "$IS_WINDOWS" -eq 0 ]]; then
        # Linux source artifacts are linked statically into Rust tests/apps.
        # WebKit's JSCOnly JIT build keeps some JIT/LLInt objects outside the
        # installed static archive set, so use C-loop for a self-contained
        # archive artifact.
        BUILD_CMD+=("--cmakeargs=-DCMAKE_DISABLE_PRECOMPILE_HEADERS=ON")
        BUILD_CMD+=("--cloop")
        BUILD_CMD+=("--cmakeargs=-DENABLE_SAMPLING_PROFILER=OFF")
        BUILD_CMD+=("--cmakeargs=-DENABLE_WEBASSEMBLY=OFF")
        export CC="${CC:-clang}"
        export CXX="${CXX:-clang++}"
        build_static_icu
        BUILD_CMD+=("--cmakeargs=-DICU_ROOT=$ICU_PREFIX")
    else
        export CC="${CC:-clang-cl}"
        export CXX="${CXX:-clang-cl}"
        VCPKG_INSTALLED_ROOT="${VCPKG_INSTALLED_DIR:-$VCPKG_ROOT_CMAKE/installed}"
        VCPKG_BUILD_INSTALLED="$VCPKG_INSTALLED_ROOT/$VCPKG_TRIPLET"
        BUILD_CMD+=("--cmakeargs=-DCMAKE_TOOLCHAIN_FILE=$VCPKG_ROOT_CMAKE/scripts/buildsystems/vcpkg.cmake")
        BUILD_CMD+=("--cmakeargs=-DVCPKG_TARGET_TRIPLET=$VCPKG_TRIPLET")
        BUILD_CMD+=("--cmakeargs=-DVCPKG_INSTALLED_DIR=$VCPKG_INSTALLED_ROOT")
        # WebKit has a vcpkg manifest with a pinned Windows ICU. Do not pass an
        # external ICU_ROOT here; mixing manifest headers with another vcpkg
        # install's ICU libraries changes the versioned ICU symbol suffix.
        BUILD_CMD+=("--cmakeargs=-DICU_INCLUDE_DIR=$VCPKG_BUILD_INSTALLED/include")
        BUILD_CMD+=("--cmakeargs=-DICU_DATA_LIBRARY_RELEASE=$VCPKG_BUILD_INSTALLED/lib/sicudt.lib")
        BUILD_CMD+=("--cmakeargs=-DICU_I18N_LIBRARY_RELEASE=$VCPKG_BUILD_INSTALLED/lib/sicuin.lib")
        BUILD_CMD+=("--cmakeargs=-DICU_UC_LIBRARY_RELEASE=$VCPKG_BUILD_INSTALLED/lib/sicuuc.lib")
        BUILD_CMD+=("--cmakeargs=-DICU_DATA_LIBRARY_DEBUG=$VCPKG_BUILD_INSTALLED/debug/lib/sicudtd.lib")
        BUILD_CMD+=("--cmakeargs=-DICU_I18N_LIBRARY_DEBUG=$VCPKG_BUILD_INSTALLED/debug/lib/sicuind.lib")
        BUILD_CMD+=("--cmakeargs=-DICU_UC_LIBRARY_DEBUG=$VCPKG_BUILD_INSTALLED/debug/lib/sicuucd.lib")
        BUILD_CMD+=("--cmakeargs=-DCMAKE_C_FLAGS=\"-Dtypeof=__typeof__ /clang:-Wno-cast-function-type-mismatch\"")
        BUILD_CMD+=("--cmakeargs=-DCMAKE_CXX_FLAGS=\"/clang:-Wno-unknown-attributes /clang:-Wno-cast-function-type-mismatch /clang:-Wno-microsoft-include\"")
        if [[ "${RONG_JSC_SKIP_WIN_LIB_UPDATE:-0}" != "1" && -f "$WEBKIT_ROOT/Tools/Scripts/update-webkit-win-libs.py" ]]; then
            echo "==> Updating WebKit Windows libraries"
            "${PYTHON:-python}" "$WEBKIT_ROOT/Tools/Scripts/update-webkit-win-libs.py"
        fi
    fi
fi

echo "==> Building: ${BUILD_CMD[*]}"
( cd "$WEBKIT_ROOT" && "${BUILD_CMD[@]}" )

# --- Locate build outputs ---------------------------------------------------
# Apple build-jsc → WebKitBuild/Release/JavaScriptCore.framework
# CMake/JSCOnly   → WebKitBuild/JSCOnly/Release/{lib,...}
FRAMEWORK=""
for c in "$BUILD_DIR/Release/JavaScriptCore.framework" \
         "$BUILD_DIR/Release/lib/JavaScriptCore.framework" \
         "$BUILD_DIR/JSCOnly/Release/JavaScriptCore.framework" \
         "$BUILD_DIR/JSCOnly/Release/lib/JavaScriptCore.framework"; do
    [[ -d "$c" ]] && { FRAMEWORK="$c"; break; }
done

LIB_SRC=""
for c in "$BUILD_DIR/JSCOnly/Release/lib" \
         "$BUILD_DIR/JSCOnly/Release/lib64" \
         "$BUILD_DIR/Release/lib" \
         "$BUILD_DIR/Release/lib64" \
         "$BUILD_DIR/Release"; do
    [[ -d "$c" ]] && { LIB_SRC="$c"; break; }
done

find_first() { # <glob> <dir>...
    local pattern="$1"
    shift
    local dir found
    for dir in "$@"; do
        [[ -d "$dir" ]] || continue
        found="$(find "$dir" -type f -iname "$pattern" -print -quit)"
        [[ -n "$found" ]] && { echo "$found"; return 0; }
    done
    return 1
}

copy_static_archive() { # <src.a> <dest.a>
    local src="$1"
    local dest="$2"
    cp "$src" "$dest"

    # WebKit's CMake builds may emit thin archives. Those are fine in-place but
    # unusable after copying into a standalone artifact because their members
    # point back at build-tree object files.
    if command -v file >/dev/null 2>&1 && file "$dest" | grep -qi 'thin archive'; then
        local ar_tool="${AR:-}"
        if [[ -z "$ar_tool" || ! -x "$(command -v "$ar_tool" 2>/dev/null || true)" ]]; then
            if command -v llvm-ar >/dev/null 2>&1; then
                ar_tool="llvm-ar"
            else
                ar_tool="ar"
            fi
        fi
        local regular="$dest.regular"
        rm -f "$regular"
        "$ar_tool" -M <<EOF
CREATE $regular
ADDLIB $src
SAVE
END
EOF
        mv "$regular" "$dest"
    fi
}

# --- Assemble the normalized install tree -----------------------------------
echo "==> Installing into $OUT_DIR"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR/include/JavaScriptCore" "$OUT_DIR/lib"

# WebKit's Headers/ are forwarding symlinks into the source tree; `cp -RL`
# dereferences them so the artifact survives deleting the checkout. On Windows
# our build-time forwarding headers are wrappers to avoid symlink privileges, so
# copy the real header body instead of preserving absolute-path wrappers.
resolve_forwarding_header() { # <file>
    local file="$1"
    local first_line real_file

    first_line=""
    read -r first_line < "$file" || true
    if [[ "$first_line" =~ ^#include[[:space:]]+\"([A-Za-z]:/.*)\"[[:space:]]*$ || "$first_line" =~ ^#include[[:space:]]+\"(/.*)\"[[:space:]]*$ ]]; then
        real_file="$(to_shell_path "${BASH_REMATCH[1]}")"
        [[ -f "$real_file" ]] && echo "$real_file"
    fi
}

copy_headers() { # <src-dir> <dest-dir>
    local src_dir="$1"
    local dest_dir="$2"
    [[ -d "$src_dir" ]] || return 0
    mkdir -p "$dest_dir"

    local file rel dest_file real_file
    while IFS= read -r -d '' file; do
        rel="${file#$src_dir/}"
        dest_file="$dest_dir/$rel"
        mkdir -p "$(dirname "$dest_file")"

        real_file="$(resolve_forwarding_header "$file")"
        if [[ -n "$real_file" ]]; then
            cp -L "$real_file" "$dest_file"
            continue
        fi

        cp -L "$file" "$dest_file"
    done < <(find -L "$src_dir" -type f -print0 2>/dev/null)
}

copy_header_subset() { # <src-dir> <dest-dir>
    local src_dir="$1"
    local dest_dir="$2"
    [[ -d "$src_dir" ]] || return 0
    mkdir -p "$dest_dir"

    local file rel dest_file real_file
    while IFS= read -r -d '' file; do
        rel="${file#$src_dir/}"
        dest_file="$dest_dir/$rel"
        mkdir -p "$(dirname "$dest_file")"

        real_file="$(resolve_forwarding_header "$file")"
        if [[ -n "$real_file" ]]; then
            cp -L "$real_file" "$dest_file"
            continue
        fi

        cp -L "$file" "$dest_file"
    done < <(find -L "$src_dir" -type f \( -name "*.h" -o -name "*.hpp" -o -name "*.inl" \) -print0 2>/dev/null)
}

normalize_forwarding_headers() { # <dir>
    local dir="$1"
    [[ -d "$dir" ]] || return 0

    local file real_file
    while IFS= read -r -d '' file; do
        real_file="$(resolve_forwarding_header "$file")"
        [[ -n "$real_file" ]] || continue
        cp -L "$real_file" "$file"
    done < <(find "$dir" -type f \( -name "*.h" -o -name "*.hpp" -o -name "*.inl" \) -print0 2>/dev/null)
}

copy_wtf_platform_headers() {
    local src_dir="$WEBKIT_ROOT/Source/WTF/wtf"
    local dest_dir="$OUT_DIR/include/WTF/wtf"
    [[ -d "$src_dir" ]] || return 0

    mkdir -p "$dest_dir"
    local header
    for header in PlatformEnableCocoa.h PlatformEnableGlib.h PlatformEnablePlayStation.h PlatformEnableWin.h; do
        [[ -f "$src_dir/$header" ]] && cp -L "$src_dir/$header" "$dest_dir/$header"
    done

    if [[ "$IS_DARWIN" -eq 1 ]]; then
        for dir in cf cocoa darwin spi; do
            copy_header_subset "$src_dir/$dir" "$dest_dir/$dir"
        done
    fi
}

copy_windows_icu_headers() {
    [[ "$IS_WINDOWS" -eq 1 ]] || return 0

    local src_dir=""
    for candidate in \
        "$VCPKG_BUILD_INSTALLED/include/unicode" \
        "$BUILD_DIR/JSCOnly/Release/vcpkg_installed/$VCPKG_TRIPLET/include/unicode"; do
        [[ -d "$candidate" ]] && { src_dir="$candidate"; break; }
    done
    [[ -n "$src_dir" ]] || return 0

    mkdir -p "$OUT_DIR/include/unicode"
    cp -RL "$src_dir/." "$OUT_DIR/include/unicode/"
}

copy_static_icu_headers() {
    [[ -n "$ICU_PREFIX" ]] || return 0
    [[ -d "$ICU_PREFIX/include/unicode" ]] || return 0

    mkdir -p "$OUT_DIR/include/unicode"
    cp -RL "$ICU_PREFIX/include/unicode/." "$OUT_DIR/include/unicode/"
}

copy_nonframework_headers() {
    # Public + private headers from the JSCOnly forwarding-header trees.
    # NOTE: the exact non-Apple header locations vary by WebKit revision; adjust
    # these globs if JavaScript.h / private headers are missing after a build.
    copy_headers "$WEBKIT_ROOT/Source/JavaScriptCore/API"                       "$OUT_DIR/include/JavaScriptCore"
    copy_headers "$BUILD_DIR/JSCOnly/Release/JavaScriptCore/Headers/JavaScriptCore" \
                                                                                  "$OUT_DIR/include/JavaScriptCore"
    copy_headers "$BUILD_DIR/JSCOnly/Release/JavaScriptCore/Headers"        "$OUT_DIR/include/JavaScriptCore"
    copy_headers "$BUILD_DIR/JSCOnly/Release/JavaScriptCore/PrivateHeaders/JavaScriptCore" \
                                                                                   "$OUT_DIR/include/JavaScriptCore/private/JavaScriptCore"
    copy_headers "$BUILD_DIR/JSCOnly/Release/WTF/Headers"                   "$OUT_DIR/include/WTF"
    copy_wtf_platform_headers
    copy_headers "$BUILD_DIR/JSCOnly/Release/bmalloc/Headers"               "$OUT_DIR/include/bmalloc"
    copy_headers "$BUILD_DIR/JavaScriptCore/Headers/JavaScriptCore"          "$OUT_DIR/include/JavaScriptCore"
    copy_headers "$BUILD_DIR/JavaScriptCore/Headers"                         "$OUT_DIR/include/JavaScriptCore"
    copy_headers "$BUILD_DIR/JavaScriptCore/PrivateHeaders/JavaScriptCore" \
                                                                                   "$OUT_DIR/include/JavaScriptCore/private/JavaScriptCore"
    copy_headers "$BUILD_DIR/WTF/Headers"                                    "$OUT_DIR/include/WTF"
    copy_wtf_platform_headers
    copy_headers "$BUILD_DIR/bmalloc/Headers"                                "$OUT_DIR/include/bmalloc"
    copy_headers "$BUILD_DIR/Release/JavaScriptCore/Headers/JavaScriptCore"  "$OUT_DIR/include/JavaScriptCore"
    copy_headers "$BUILD_DIR/Release/JavaScriptCore/Headers"                "$OUT_DIR/include/JavaScriptCore"
    copy_headers "$BUILD_DIR/Release/JavaScriptCore/PrivateHeaders/JavaScriptCore" \
                                                                                   "$OUT_DIR/include/JavaScriptCore/private/JavaScriptCore"
    copy_headers "$BUILD_DIR/Release/WTF/Headers"                           "$OUT_DIR/include/WTF"
    copy_wtf_platform_headers
    copy_headers "$BUILD_DIR/Release/bmalloc/Headers"                       "$OUT_DIR/include/bmalloc"
}

if [[ -n "$FRAMEWORK" ]]; then
    ARTIFACT_KIND="framework"
    echo "    framework: $FRAMEWORK"
    rm -rf "$OUT_DIR/lib/JavaScriptCore.framework"
    ditto "$FRAMEWORK" "$OUT_DIR/lib/JavaScriptCore.framework"
    # Linkers can prefer .tbd stubs inside frameworks and record the stub's
    # @rpath install name instead of the real binary's stamped install name.
    # Keep the artifact relocatable and let build.rs stamp the real binary at
    # the final extracted path.
    find "$OUT_DIR/lib/JavaScriptCore.framework" -type f -name "*.tbd" -delete 2>/dev/null || true
    copy_headers "$FRAMEWORK/Headers"        "$OUT_DIR/include/JavaScriptCore"
    copy_headers "$FRAMEWORK/PrivateHeaders"  "$OUT_DIR/include/JavaScriptCore/private"
    copy_nonframework_headers
elif [[ -n "$LIB_SRC" || "$IS_WINDOWS" -eq 1 ]]; then
    ARTIFACT_KIND="static"
    if [[ "$IS_WINDOWS" -eq 1 ]]; then
        echo "    static libs: searching $BUILD_DIR and WebKitLibraries/win"
        win_lib_dirs=(
            "$VCPKG_BUILD_INSTALLED/lib"
            "$VCPKG_BUILD_INSTALLED/debug/lib"
            "$BUILD_DIR/JSCOnly/Release/vcpkg_installed/$VCPKG_TRIPLET/lib"
            "$BUILD_DIR/JSCOnly/Release/vcpkg_installed/$VCPKG_TRIPLET/debug/lib"
            "$BUILD_DIR/JSCOnly/Release/lib"
            "$BUILD_DIR/JSCOnly/Release"
            "$BUILD_DIR/Release/lib"
            "$BUILD_DIR/Release"
            "$BUILD_DIR"
            "$WEBKIT_ROOT/WebKitLibraries/win"
        )
        for a in JavaScriptCore WTF bmalloc; do
            p="$(find_first "$a.lib" "${win_lib_dirs[@]}" || true)"
            if [[ -n "$p" ]]; then
                echo "    $a.lib: $p"
                cp "$p" "$OUT_DIR/lib/$a.lib"
            else
                echo "WARNING: could not find $a.lib under $BUILD_DIR" >&2
            fi
        done
        for pair in sicuin:icuin sicuuc:icuuc sicudt:icudt; do
            want="${pair%%:*}"
            fallback="${pair#*:}"
            p="$(find_first "$want.lib" "${win_lib_dirs[@]}" || true)"
            [[ -z "$p" ]] && p="$(find_first "$fallback.lib" "${win_lib_dirs[@]}" || true)"
            if [[ -n "$p" ]]; then
                echo "    $want.lib: $p"
                cp "$p" "$OUT_DIR/lib/$want.lib"
            else
                echo "WARNING: could not find $want.lib or $fallback.lib under $BUILD_DIR" >&2
            fi
        done
        mkdir -p "$OUT_DIR/bin"
        find "$BUILD_DIR" "$WEBKIT_ROOT/WebKitLibraries/win" -type f -iname "*.dll" \
            -exec cp {} "$OUT_DIR/bin/" \; 2>/dev/null || true
    else
        echo "    static libs: $LIB_SRC"
        for a in JavaScriptCore WTF bmalloc; do
            p="$(find "$LIB_SRC" -maxdepth 1 -name "lib$a.a" -print -quit)"
            [[ -n "$p" ]] && copy_static_archive "$p" "$OUT_DIR/lib/lib$a.a"
        done
    fi
    copy_nonframework_headers
else
    echo "Could not locate a JavaScriptCore framework or lib dir under $BUILD_DIR" >&2
    exit 1
fi

# (non-Apple/static) bundle the static ICU archives that build.rs links with
# static=icui18n/icuuc/icudata.
if [[ "$IS_DARWIN" -eq 0 && -n "$ICU_PREFIX" ]]; then
    for a in icui18n icuuc icudata; do
        copy_static_archive "$ICU_PREFIX/lib/lib$a.a" "$OUT_DIR/lib/lib$a.a" 2>/dev/null \
            || echo "WARNING: static ICU lib$a.a missing in $ICU_PREFIX/lib" >&2
    done
fi
copy_windows_icu_headers
copy_static_icu_headers

normalize_forwarding_headers "$OUT_DIR/include"

require_file() {
    [[ -f "$1" ]] || { echo "Missing required file: $1" >&2; exit 1; }
}

require_dir() {
    [[ -d "$1" ]] || { echo "Missing required directory: $1" >&2; exit 1; }
}

require_no_absolute_forwarding_headers() { # <dir>
    local dir="$1"
    local leak
    leak="$(grep -R -n -m 1 -E '^#include[[:space:]]+"([A-Za-z]:|/)' "$dir" 2>/dev/null || true)"
    [[ -z "$leak" ]] || { echo "Artifact contains absolute forwarding header: $leak" >&2; exit 1; }
}

require_defined_symbol() { # <library> <symbol>
    local library="$1"
    local symbol="$2"
    local tool
    local inspected=0
    local pattern="^[[:space:]]*[[:xdigit:]]+[[:space:]]+[A-Za-z][[:space:]]+_?${symbol}$"

    for tool in llvm-nm nm; do
        command -v "$tool" >/dev/null 2>&1 || continue
        inspected=1
        if "$tool" -g "$library" 2>/dev/null | grep -E "$pattern" >/dev/null; then
            return 0
        fi
    done

    if command -v dumpbin >/dev/null 2>&1; then
        inspected=1
        if dumpbin /symbols "$library" 2>/dev/null \
            | grep -E "External.*\\|[[:space:]]+_?${symbol}$" >/dev/null; then
            return 0
        fi
    fi

    if [[ "$inspected" -eq 0 ]]; then
        echo "No nm-compatible tool or dumpbin available to validate $library" >&2
    else
        echo "Artifact library does not define required symbol: $symbol" >&2
    fi
    exit 1
}

echo "==> Validating artifact"
require_file "$OUT_DIR/include/JavaScriptCore/JavaScript.h"
require_file "$OUT_DIR/include/JavaScriptCore/private/JavaScriptCore/Completion.h"
require_file "$OUT_DIR/include/JavaScriptCore/private/JavaScriptCore/BytecodeCacheError.h"
require_dir "$OUT_DIR/include/WTF"
require_dir "$OUT_DIR/include/bmalloc"
require_no_absolute_forwarding_headers "$OUT_DIR/include"

if [[ "${ARTIFACT_KIND:-}" == "framework" ]]; then
    require_dir "$OUT_DIR/lib/JavaScriptCore.framework"
    JSC_LIBRARY="$OUT_DIR/lib/JavaScriptCore.framework/JavaScriptCore"
    require_file "$JSC_LIBRARY"
else
    if [[ "$IS_WINDOWS" -eq 1 ]]; then
        for a in JavaScriptCore WTF bmalloc sicuin sicuuc sicudt; do
            require_file "$OUT_DIR/lib/$a.lib"
        done
        JSC_LIBRARY="$OUT_DIR/lib/JavaScriptCore.lib"
    else
        for a in JavaScriptCore WTF bmalloc; do
            require_file "$OUT_DIR/lib/lib$a.a"
        done
        for a in icui18n icuuc icudata; do
            require_file "$OUT_DIR/lib/lib$a.a"
        done
        require_file "$OUT_DIR/include/unicode/utypes.h"
        JSC_LIBRARY="$OUT_DIR/lib/libJavaScriptCore.a"
    fi
fi

# Source artifacts are required to support Rong's hard-interruption contract.
# WebKit's JSCOnly build exports these APIs without an extra CMake toggle; fail
# packaging here if a platform build ever omits or hides them.
require_defined_symbol "$JSC_LIBRARY" JSContextGroupSetExecutionTimeLimit
require_defined_symbol "$JSC_LIBRARY" JSContextGroupClearExecutionTimeLimit

cat > "$OUT_DIR/rong-jsc-artifact.json" <<EOF
{
  "format": 1,
  "source_repo": "$WEBKIT_URL",
  "webkit_ref": "$WEBKIT_REF",
  "target": "$TARGET",
  "kind": "${ARTIFACT_KIND:-unknown}",
  "bytecode_private_headers": true,
  "hard_interrupt": true
}
EOF

HOST_TARGET=""
if command -v rustc >/dev/null 2>&1; then
    HOST_TARGET="$(rustc -vV | awk '/^host:/{print $2}')"
fi
if [[ "${RONG_JSC_SKIP_SMOKE:-0}" != "1" && "$HOST_TARGET" == "$TARGET" ]]; then
    echo "==> Smoke-testing artifact"
    SMOKE_JSC_ROOT="$OUT_DIR"
    SMOKE_ENV=("RONG_JSC_ROOT=$SMOKE_JSC_ROOT" "RONG_JSC_REQUIRE_BYTECODE=1")
    if [[ "$IS_WINDOWS" -eq 1 ]]; then
        SMOKE_JSC_ROOT="$(to_cmake_path "$OUT_DIR")"
        SMOKE_ENV=("RONG_JSC_ROOT=$SMOKE_JSC_ROOT" "RONG_JSC_REQUIRE_BYTECODE=1")
        MSVC_LINKER="$(find_msvc_linker || true)"
        if [[ -n "$MSVC_LINKER" ]]; then
            TARGET_LINKER_ENV="CARGO_TARGET_$(echo "$TARGET" | tr '[:lower:].-' '[:upper:]__')_LINKER"
            SMOKE_ENV+=("$TARGET_LINKER_ENV=$MSVC_LINKER")
        fi
    fi
    ( cd "$ROOT_DIR" && \
      env "${SMOKE_ENV[@]}" cargo test --release --test eval --test interrupt \
        --no-default-features --features jscore-source,tls-aws-lc --quiet )
fi

echo "Installed artifact:"
echo "  include: $OUT_DIR/include"
echo "  lib:     $OUT_DIR/lib"

# --- Optional: package + checksum -------------------------------------------
if [[ -n "$PACKAGE" ]]; then
    echo "==> Packaging $PACKAGE"
    mkdir -p "$(dirname "$PACKAGE")"
    package_entries=(include lib)
    [[ -d "$OUT_DIR/bin" ]] && package_entries+=(bin)
    tar -czf "$PACKAGE" -C "$OUT_DIR" "${package_entries[@]}"
    if command -v shasum >/dev/null 2>&1; then
        SHA="$(shasum -a 256 "$PACKAGE" | awk '{print $1}')"
    else
        SHA="$(sha256sum "$PACKAGE" | awk '{print $1}')"
    fi
    echo "  tarball: $PACKAGE"
    echo "  sha256:  $SHA"
    echo
    echo "webkit-artifacts.tsv row (fill in <release-tag>):"
    echo "  $TARGET   <release-tag>   $(basename "$PACKAGE")   $SHA"
fi
