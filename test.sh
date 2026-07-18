#!/bin/bash

# Rong Test Runner
# Supports running tests across all JavaScript engines

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging helpers
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[PASS]${NC} $1"
}

log_error() {
    echo -e "${RED}[FAIL]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

HOST_TLS_BACKEND="${HOST_TLS_BACKEND:-tls-aws-lc}"
SUPPORTED_ENGINES=("quickjs" "jscore" "arkjs")

# The default jscore feature set is the production shape (no private SPI), so
# the cooperative-only path and the interrupt test's skip branches stay
# covered. Engine preemption is exercised by a dedicated interrupt-spi run;
# see run_jscore_interrupt_spi.
jscore_features() {
    local feature_set="jscore,$HOST_TLS_BACKEND"
    if [[ -n "${RONG_JSC_SOURCE:-}" ]]; then
        feature_set="$feature_set,rong/jscore-source"
    fi
    echo "$feature_set"
}

# Additionally exercise JSC hard interruption behind the opt-in SPI feature.
run_jscore_interrupt_spi() {
    local features
    features="$(jscore_features),rong/jscore-interrupt"
    log_info "Running interrupt tests with JSC execution-time-limit SPI"
    TOTAL_TESTS=$((TOTAL_TESTS + 1))

    if run_cargo_test --release --test=interrupt --no-default-features \
        --features="$features" --quiet; then
        log_success "JSC execution-time-limit SPI interrupt tests passed"
        PASSED_TESTS=$((PASSED_TESTS + 1))
        return 0
    else
        log_error "JSC execution-time-limit SPI interrupt tests failed"
        FAILED_TESTS=$((FAILED_TESTS + 1))

        if [[ "$FAIL_FAST" == true ]]; then
            log_error "Stopping due to fail-fast mode (default); use -k/--continue-on-error to keep going"
            print_summary
            exit 1
        fi
        return 1
    fi
}

run_cargo_test() {
    if [[ -n "${RONG_TEST_TIMEOUT_SECONDS:-}" ]] && command -v perl >/dev/null 2>&1; then
        perl -e 'alarm shift; exec @ARGV' "$RONG_TEST_TIMEOUT_SECONDS" cargo test "$@"
    else
        cargo test "$@"
    fi
}

host_is_apple() {
    rustc -vV | grep -q '^host: .*apple'
}

jscore_source_configured() {
    [[ -n "${RONG_JSC_SOURCE:-}" ]] ||
        [[ -n "${RONG_JSC_ROOT:-}" ]]
}

jscore_source_lib_dir() {
    local target
    target="$(rustc -vV | awk '/^host: / { print $2; exit }')"

    if [[ -n "${RONG_JSC_ROOT:-}" ]]; then
        local dirs=(
            "$RONG_JSC_ROOT/lib"
            "$RONG_JSC_ROOT/usr/lib"
            "$RONG_JSC_ROOT/WebKitBuild/JSCOnly/Release/lib"
            "$RONG_JSC_ROOT/WebKitBuild/Release/lib"
        )
        local dir
        for dir in "${dirs[@]}"; do
            if [[ -d "$dir/JavaScriptCore.framework" ]]; then
                echo "$dir"
                return 0
            fi
        done
        echo "$RONG_JSC_ROOT/lib"
        return 0
    fi

    local cache_base
    if [[ -n "${RONG_JSC_CACHE_DIR:-}" ]]; then
        cache_base="$RONG_JSC_CACHE_DIR"
    else
        cache_base="${XDG_CACHE_HOME:-$HOME/.cache}/rong/webkit"
    fi
    echo "$cache_base/$target/lib"
}

configure_jscore_source_apple_runtime() {
    local engine=$1
    if [[ "$engine" != "jscore" ]] || ! host_is_apple || ! jscore_source_configured; then
        return 0
    fi

    local lib_dir
    lib_dir="$(jscore_source_lib_dir)"
    [[ -n "$lib_dir" ]] || return 0

    case " ${RUSTFLAGS:-} " in
        *"-Wl,-rpath,$lib_dir"*) ;;
        *) export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-C link-arg=-Wl,-rpath,$lib_dir" ;;
    esac
    export DYLD_FRAMEWORK_PATH="$lib_dir${DYLD_FRAMEWORK_PATH:+:$DYLD_FRAMEWORK_PATH}"
    log_info "Using JSC source framework path: $lib_dir"
}

ENGINES=("quickjs")
if host_is_apple || jscore_source_configured; then
    ENGINES+=("jscore")
fi

# Cleanup function to kill child processes
cleanup() {
    if [[ "${CLEANED_UP:-false}" == true ]]; then
        return 0
    fi
    CLEANED_UP=true

    if pgrep -P $$ >/dev/null 2>&1; then
        log_warning "Cleaning up child processes..."
        pkill -TERM -P $$ 2>/dev/null || true
        sleep 0.2
        pkill -KILL -P $$ 2>/dev/null || true
        wait 2>/dev/null || true
    fi
}

# Set trap to cleanup on exit/signals
trap cleanup EXIT
trap 'cleanup; exit 130' INT
trap 'cleanup; exit 143' TERM

# Auto-discover test categories
get_core_tests() {
    if [[ -d "tests" ]]; then
        find tests -name "*.rs" -type f | sed 's|tests/||; s|\.rs$||' | sort
    fi
}

get_module_tests() {
    if [[ -d "modules" ]]; then
        find modules -maxdepth 1 -type d | sed 's|modules/||' | grep -v '^$' | grep -v '^modules$' | grep -v '^\.' | sort
    fi
}

# Initialize test arrays
CORE_TESTS=($(get_core_tests))
MODULE_TESTS=($(get_module_tests))

# Statistics
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0

print_header() {
    echo -e "${BLUE}================================${NC}"
    echo -e "${BLUE}  Rong Test Runner${NC}"
    echo -e "${BLUE}================================${NC}"
}

print_usage() {
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  -e, --engine ENGINE           Run tests for specific engine (quickjs, jscore, arkjs, all)"
    echo "  -t, --test TEST               Run specific test (core test name or module name)"
    echo "  -c, --core                    Run only core tests"
    echo "  -m, --modules                 Run only module tests"
    echo "  -k, --continue-on-error       Continue running tests after failure (default: stop on error)"
    echo "  -h, --help                    Show this help message"
    echo ""
    echo "Examples:"
    echo "  $0                            # Run all tests, stop on first failure"
    echo "  $0 -k                         # Run all tests, continue on failures"
    echo "  $0 -e quickjs                 # Run all tests on QuickJS"
    echo "  $0 -e jscore -c               # Run core tests on JavaScriptCore"
    echo "  $0 -e arkjs                   # Show how to run ArkJS device-side tests"
    echo "  $0 -t iterator                # Run iterator tests on all engines"
    echo "  $0 -t rong_http               # Run rong_http module tests on all engines"
    echo "  $0 -k -m                      # Run all module tests, continue on error"
}

print_arkjs_instructions() {
    echo -e "${BLUE}================================${NC}"
    echo -e "${BLUE}  ArkJS Test Flow${NC}"
    echo -e "${BLUE}================================${NC}"
    echo "ArkJS tests run on a HarmonyOS device via the smoke app."
    echo ""
    echo "Run:"
    echo "  ./testing/harmony/dev.sh test"
    echo ""
    echo "The script prints the JSON-derived test summary on the PC."
    echo "For debug logs only:"
    echo "  hdc hilog | grep RongSmoke"
    echo ""
    echo "The device-side smoke run includes the generated tests from tests/*.rs,"
    echo "including the Rong worker/runtime cases from tests/rong.rs."
}

run_core_test() {
    local test_name=$1
    local engine=$2
    local feature_set="$engine,$HOST_TLS_BACKEND"
    if [[ "$engine" == "jscore" ]]; then
        feature_set="$(jscore_features)"
    fi

    log_info "Running core test: $test_name (engine: $engine)"
    TOTAL_TESTS=$((TOTAL_TESTS + 1))

    if run_cargo_test --release --test="$test_name" --no-default-features --features="$feature_set" --quiet; then
        log_success "Core test $test_name passed on $engine"
        PASSED_TESTS=$((PASSED_TESTS + 1))
        return 0
    else
        log_error "Core test $test_name failed on $engine"
        FAILED_TESTS=$((FAILED_TESTS + 1))

        if [[ "$FAIL_FAST" == true ]]; then
            log_error "Stopping due to fail-fast mode (default); use -k/--continue-on-error to keep going"
            print_summary
            exit 1
        fi
        return 1
    fi
}

run_module_test() {
    local module_name=$1
    local engine=$2

    log_info "Running module test: $module_name (engine: $engine)"
    TOTAL_TESTS=$((TOTAL_TESTS + 1))

    local feature_set="$engine,$HOST_TLS_BACKEND"
    if [[ "$engine" == "jscore" ]]; then
        feature_set="$(jscore_features)"
    fi

    local cargo_args=(-p "$module_name" --no-default-features --features="$feature_set" --quiet)
    if [[ "$module_name" == "rong_timer" ]]; then
        # Timer tests share the global host executor and are sensitive to crate-level test overlap.
        cargo_args+=(-- --test-threads=1)
    fi

    if run_cargo_test "${cargo_args[@]}"; then
        log_success "Module test $module_name passed on $engine"
        PASSED_TESTS=$((PASSED_TESTS + 1))
        return 0
    else
        log_error "Module test $module_name failed on $engine"
        FAILED_TESTS=$((FAILED_TESTS + 1))

        if [[ "$FAIL_FAST" == true ]]; then
            log_error "Stopping due to fail-fast mode (default); use -k/--continue-on-error to keep going"
            print_summary
            exit 1
        fi
        return 1
    fi
}

run_all_core_tests() {
    local engine=$1

    echo -e "\n${YELLOW}Running core tests on $engine...${NC}"

    for test in "${CORE_TESTS[@]}"; do
        run_core_test "$test" "$engine" || true
    done
}

run_all_module_tests() {
    local engine=$1

    echo -e "\n${YELLOW}Running module tests on $engine...${NC}"

    for module in "${MODULE_TESTS[@]}"; do
        run_module_test "$module" "$engine" || true
    done
}

run_specific_test() {
    local test_name=$1
    local engine=$2

    # Check if it's a core test
    for core_test in "${CORE_TESTS[@]}"; do
        if [[ "$core_test" == "$test_name" ]]; then
            run_core_test "$test_name" "$engine"
            return $?
        fi
    done

    # Check if it's a module test
    for module_test in "${MODULE_TESTS[@]}"; do
        if [[ "$module_test" == "$test_name" ]]; then
            run_module_test "$test_name" "$engine"
            return $?
        fi
    done

    log_error "Unknown test: $test_name"
    return 1
}

print_summary() {
    echo -e "\n${BLUE}================================${NC}"
    echo -e "${BLUE}  Test Summary${NC}"
    echo -e "${BLUE}================================${NC}"
    echo -e "Total tests: $TOTAL_TESTS"
    echo -e "${GREEN}Passed: $PASSED_TESTS${NC}"
    echo -e "${RED}Failed: $FAILED_TESTS${NC}"

    if [[ $FAILED_TESTS -eq 0 ]]; then
        echo -e "\n${GREEN}All tests passed! 🎉${NC}"
        exit 0
    else
        echo -e "\n${RED}Some tests failed! ❌${NC}"
        exit 1
    fi
}

# Parse command line arguments
ENGINE_FILTER=""
TEST_FILTER=""
CORE_ONLY=false
MODULES_ONLY=false
FAIL_FAST=true  # Default: stop on first failure

while [[ $# -gt 0 ]]; do
    case $1 in
        -e|--engine)
            ENGINE_FILTER="$2"
            shift 2
            ;;
        -t|--test)
            TEST_FILTER="$2"
            shift 2
            ;;
        -c|--core)
            CORE_ONLY=true
            shift
            ;;
        -m|--modules)
            MODULES_ONLY=true
            shift
            ;;
        -k|--continue-on-error)
            FAIL_FAST=false
            shift
            ;;
        -h|--help)
            print_usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            print_usage
            exit 1
            ;;
    esac
done

# Validate engine filter
if [[ -n "$ENGINE_FILTER" && "$ENGINE_FILTER" != "all" ]]; then
    if [[ ! " ${SUPPORTED_ENGINES[@]} " =~ " ${ENGINE_FILTER} " ]]; then
        log_error "Unknown engine: $ENGINE_FILTER"
        echo "Available engines: ${SUPPORTED_ENGINES[*]}"
        exit 1
    fi
    if [[ "$ENGINE_FILTER" == "arkjs" ]]; then
        print_arkjs_instructions
        exit 0
    fi
    ENGINES=("$ENGINE_FILTER")
fi

print_header

# Main execution
for engine in "${ENGINES[@]}"; do
    echo -e "\n${YELLOW}Testing with engine: $engine${NC}"
    configure_jscore_source_apple_runtime "$engine"

    if [[ -n "$TEST_FILTER" ]]; then
        # Run specific test
        run_specific_test "$TEST_FILTER" "$engine" || true
    elif [[ "$CORE_ONLY" == true ]]; then
        # Run only core tests
        run_all_core_tests "$engine"
    elif [[ "$MODULES_ONLY" == true ]]; then
        # Run only module tests
        run_all_module_tests "$engine"
    else
        # Run all tests
        run_all_core_tests "$engine"
        run_all_module_tests "$engine"
        # JSC engine preemption lives behind the opt-in private SPI, run once
        # more with it enabled (the default pass above covered the SPI-free
        # production shape).
        if [[ "$engine" == "jscore" ]]; then
            run_jscore_interrupt_spi || true
        fi
    fi
done

print_summary
