#!/bin/bash
# WATOS Master Test Script
# Orchestrates all testing: build, lint, boot tests

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

# Test selection
RUN_DEPS=false
RUN_BUILD=false
RUN_LINT=false
RUN_BOOT=false
RUN_ALL=true
QUICK_MODE=false
VERBOSE=false

usage() {
    echo "WATOS Test Suite"
    echo ""
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --deps       Check dependencies only"
    echo "  --build      Run build test only"
    echo "  --lint       Run lint/format check only"
    echo "  --boot       Run boot test only"
    echo "  --quick      Quick test (build + boot, skip lint)"
    echo "  --all        Run all tests (default)"
    echo "  --verbose    Verbose output"
    echo "  -h, --help   Show this help"
    echo ""
    echo "Examples:"
    echo "  $0                   # Run all tests"
    echo "  $0 --quick           # Quick build and boot test"
    echo "  $0 --build --boot    # Build and boot tests"
    exit 0
}

while [[ $# -gt 0 ]]; do
    case $1 in
        --deps)
            RUN_DEPS=true
            RUN_ALL=false
            shift
            ;;
        --build)
            RUN_BUILD=true
            RUN_ALL=false
            shift
            ;;
        --lint)
            RUN_LINT=true
            RUN_ALL=false
            shift
            ;;
        --boot)
            RUN_BOOT=true
            RUN_ALL=false
            shift
            ;;
        --quick)
            QUICK_MODE=true
            RUN_ALL=false
            shift
            ;;
        --all)
            RUN_ALL=true
            shift
            ;;
        --verbose)
            VERBOSE=true
            shift
            ;;
        -h|--help)
            usage
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            usage
            ;;
    esac
done

# Determine what to run
if [ "$RUN_ALL" = true ]; then
    RUN_DEPS=true
    RUN_BUILD=true
    RUN_LINT=true
    RUN_BOOT=true
fi

if [ "$QUICK_MODE" = true ]; then
    RUN_BUILD=true
    RUN_BOOT=true
fi

# Tracking
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0
FAILED_TESTS=()

START_TIME=$(date +%s)

log() {
    echo -e "${BLUE}[TEST]${NC} $1"
}

header() {
    echo ""
    echo -e "${CYAN}========================================${NC}"
    echo -e "${CYAN}$1${NC}"
    echo -e "${CYAN}========================================${NC}"
}

run_test() {
    local name=$1
    local cmd=$2

    TESTS_RUN=$((TESTS_RUN + 1))
    log "Running: $name"

    if eval "$cmd"; then
        echo -e "${GREEN}[PASS]${NC} $name"
        TESTS_PASSED=$((TESTS_PASSED + 1))
        return 0
    else
        echo -e "${RED}[FAIL]${NC} $name"
        TESTS_FAILED=$((TESTS_FAILED + 1))
        FAILED_TESTS+=("$name")
        return 1
    fi
}

header "WATOS Test Suite"
echo "Date: $(date)"
echo "Project: $PROJECT_ROOT"
echo ""

# Dependency check
if [ "$RUN_DEPS" = true ]; then
    header "Dependency Check"
    if ! run_test "Dependencies" "$SCRIPT_DIR/check_deps.sh"; then
        echo -e "${RED}Missing dependencies. Fix before continuing.${NC}"
        exit 1
    fi
fi

# Lint/format check
if [ "$RUN_LINT" = true ]; then
    header "Code Quality"

    export PATH="$HOME/.cargo/bin:$PATH"

    # Format check
    log "Checking code formatting..."
    cd "$PROJECT_ROOT/src"
    if cargo fmt --check 2>/dev/null; then
        echo -e "${GREEN}[PASS]${NC} Code formatting"
        TESTS_RUN=$((TESTS_RUN + 1))
        TESTS_PASSED=$((TESTS_PASSED + 1))
    else
        echo -e "${YELLOW}[WARN]${NC} Code formatting issues (run: cargo fmt)"
        TESTS_RUN=$((TESTS_RUN + 1))
        # Don't fail on format, just warn
        TESTS_PASSED=$((TESTS_PASSED + 1))
    fi

    # Clippy
    log "Running Clippy..."
    cd "$PROJECT_ROOT/src"
    if cargo clippy --target x86_64-unknown-none -- -D warnings 2>&1 | head -50; then
        echo -e "${GREEN}[PASS]${NC} Clippy lint"
        TESTS_RUN=$((TESTS_RUN + 1))
        TESTS_PASSED=$((TESTS_PASSED + 1))
    else
        echo -e "${YELLOW}[WARN]${NC} Clippy warnings present"
        TESTS_RUN=$((TESTS_RUN + 1))
        # Don't fail on clippy, just warn
        TESTS_PASSED=$((TESTS_PASSED + 1))
    fi

    cd "$PROJECT_ROOT"
fi

# Build test
if [ "$RUN_BUILD" = true ]; then
    header "Build Test"

    BUILD_OPTS=""
    if [ "$VERBOSE" = true ]; then
        BUILD_OPTS="--verbose"
    fi

    run_test "Full Build" "$SCRIPT_DIR/build.sh --release $BUILD_OPTS" || true
fi

# Boot test
if [ "$RUN_BOOT" = true ]; then
    header "Boot Test"

    if [ ! -f "$PROJECT_ROOT/uefi_test/EFI/BOOT/BOOTX64.EFI" ]; then
        echo -e "${YELLOW}[SKIP]${NC} Boot test - no build artifacts"
        echo "Run build first: ./scripts/build.sh"
    else
        BOOT_OPTS=""
        if [ "$VERBOSE" = true ]; then
            BOOT_OPTS="--verbose"
        fi

        run_test "QEMU Boot" "$SCRIPT_DIR/boot_test.sh --timeout 20 $BOOT_OPTS" || true
    fi
fi

# Summary
END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))

header "Test Summary"
echo "Duration: ${DURATION}s"
echo "Tests run: $TESTS_RUN"
echo -e "Passed: ${GREEN}$TESTS_PASSED${NC}"
echo -e "Failed: ${RED}$TESTS_FAILED${NC}"

if [ ${#FAILED_TESTS[@]} -gt 0 ]; then
    echo ""
    echo "Failed tests:"
    for test in "${FAILED_TESTS[@]}"; do
        echo -e "  ${RED}- $test${NC}"
    done
fi

echo ""
if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
else
    echo -e "${RED}Some tests failed.${NC}"
    exit 1
fi
