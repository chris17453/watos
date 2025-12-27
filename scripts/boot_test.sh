#!/bin/bash
# WATOS QEMU Boot Test Runner
# Automated kernel boot validation using QEMU/OVMF

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Configuration
TIMEOUT=${TIMEOUT:-30}
LOG_DIR="$PROJECT_ROOT/ai-temp/logs"
SERIAL_LOG="$LOG_DIR/serial_$(date +%Y%m%d_%H%M%S).log"
QEMU_PID=""

# Success patterns - kernel boot indicators
SUCCESS_PATTERNS=(
    "WATOS"
    "C:\\\\>"
    "kernel"
    "boot"
)

# Failure patterns - indicates crash or error
FAILURE_PATTERNS=(
    "triple fault"
    "panic"
    "exception"
    "ASSERT"
)

usage() {
    echo "WATOS Boot Test Runner"
    echo ""
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --timeout N    Timeout in seconds (default: 30)"
    echo "  --interactive  Run in interactive mode (no timeout)"
    echo "  --verbose      Show QEMU output in real-time"
    echo "  -h, --help     Show this help"
    exit 0
}

INTERACTIVE=false
VERBOSE=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --timeout)
            TIMEOUT=$2
            shift 2
            ;;
        --interactive)
            INTERACTIVE=true
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

log() {
    echo -e "${BLUE}[TEST]${NC} $1"
}

success() {
    echo -e "${GREEN}[PASS]${NC} $1"
}

fail() {
    echo -e "${RED}[FAIL]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

cleanup() {
    if [ -n "$QEMU_PID" ] && kill -0 "$QEMU_PID" 2>/dev/null; then
        kill "$QEMU_PID" 2>/dev/null || true
        wait "$QEMU_PID" 2>/dev/null || true
    fi
}

trap cleanup EXIT

# Create log directory
mkdir -p "$LOG_DIR"

echo "========================================"
echo "WATOS Boot Test"
echo "========================================"
echo "Timeout: ${TIMEOUT}s"
echo "Log: $SERIAL_LOG"
echo ""

# Check prerequisites
log "Checking prerequisites..."

if [ ! -f "$PROJECT_ROOT/uefi_test/EFI/BOOT/BOOTX64.EFI" ]; then
    fail "UEFI boot structure not found. Run ./scripts/build.sh first."
    exit 1
fi

if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
    fail "QEMU not found. Install with: sudo dnf install qemu-system-x86"
    exit 1
fi

OVMF_CODE="/usr/share/OVMF/OVMF_CODE.fd"
OVMF_VARS_TEMPLATE="/usr/share/OVMF/OVMF_VARS.fd"

if [ ! -f "$OVMF_CODE" ]; then
    fail "OVMF firmware not found. Install with: sudo dnf install edk2-ovmf"
    exit 1
fi

# Create working copy of OVMF vars
OVMF_VARS="$PROJECT_ROOT/ai-temp/OVMF_VARS.fd"
cp "$OVMF_VARS_TEMPLATE" "$OVMF_VARS"

success "Prerequisites satisfied"

# Build QEMU command
# Attach boot disk and WFS data disk to ICH9's IDE buses (SATA emulation)
# Use KVM if available for proper CPU idle handling (hlt instruction)
QEMU_CMD=(
    qemu-system-x86_64
    -machine q35,accel=kvm:tcg
    -cpu max,+invtsc
    -smp 2
    -m 256M
    -drive "if=pflash,format=raw,readonly=on,file=$OVMF_CODE"
    -drive "if=pflash,format=raw,file=$OVMF_VARS"
    -drive "format=raw,file=fat:rw:$PROJECT_ROOT/uefi_test,if=none,id=bootdisk"
    -device ide-hd,drive=bootdisk,bus=ide.0
    -netdev user,id=net0
    -device e1000,netdev=net0
    -display none
    -serial "file:$SERIAL_LOG"
    -monitor none
    -no-reboot
)

# Add WFS data disk if it exists (for disk testing)
# Attach to ICH9 AHCI's ide.1 bus (SATA port 1)
if [ -f "$PROJECT_ROOT/output/watos.img" ]; then
    QEMU_CMD+=(-drive "file=$PROJECT_ROOT/output/watos.img,format=raw,if=none,id=wfsdisk")
    QEMU_CMD+=(-device ide-hd,drive=wfsdisk,bus=ide.1)
fi

# Add DOS 6.22 disk as FAT drive if the folder exists
DOS_DIR="$PROJECT_ROOT/dos-6.22"
if [ -d "$DOS_DIR" ]; then
    QEMU_CMD+=(-drive "file=fat:rw:$DOS_DIR,format=raw,if=none,id=dosdisk")
    QEMU_CMD+=(-device ide-hd,drive=dosdisk,bus=ide.2)
fi

if [ "$INTERACTIVE" = true ]; then
    log "Starting QEMU in interactive mode..."
    log "Serial log: $SERIAL_LOG"
    log "Close the QEMU window to exit (Ctrl+C in terminal)"

    # Build QEMU command with optional WFS data disk
    # Use KVM if available for proper CPU idle handling (hlt instruction)
    QEMU_ARGS=(
        -machine q35,accel=kvm:tcg
        -cpu max,+invtsc
        -smp 2
        -m 256M
        -bios "$OVMF_CODE"
        -drive "file=fat:rw:$PROJECT_ROOT/uefi_test,format=raw,if=none,id=bootdisk"
        -device ide-hd,drive=bootdisk,bus=ide.0
        -netdev user,id=net0
        -device e1000,netdev=net0
        -vga std
        -display gtk
        -chardev stdio,id=char0,mux=on,logfile="$SERIAL_LOG"
        -serial chardev:char0
    )

    # Add WFS data disk if it exists
    if [ -f "$PROJECT_ROOT/output/watos.img" ]; then
        log "Adding WFS data disk: output/watos.img to ide.1"
        QEMU_ARGS+=(-drive "file=$PROJECT_ROOT/output/watos.img,format=raw,if=none,id=wfsdisk")
        QEMU_ARGS+=(-device ide-hd,drive=wfsdisk,bus=ide.1)
    fi

    # Add DOS 6.22 disk as FAT drive if the folder exists
    DOS_DIR="$PROJECT_ROOT/dos-6.22"
    if [ -d "$DOS_DIR" ]; then
        log "Adding DOS 6.22 disk: dos-6.22/ as FAT drive on ide.2"
        QEMU_ARGS+=(-drive "file=fat:rw:$DOS_DIR,format=raw,if=none,id=dosdisk")
        QEMU_ARGS+=(-device ide-hd,drive=dosdisk,bus=ide.2)
    fi

    qemu-system-x86_64 "${QEMU_ARGS[@]}"
    exit 0
fi

# Start QEMU in background
log "Starting QEMU..."
"${QEMU_CMD[@]}" &
QEMU_PID=$!

log "QEMU PID: $QEMU_PID"
log "Waiting for kernel boot (timeout: ${TIMEOUT}s)..."

# Monitor for success/failure
START_TIME=$(date +%s)
RESULT="timeout"

while true; do
    CURRENT_TIME=$(date +%s)
    ELAPSED=$((CURRENT_TIME - START_TIME))

    # Check if QEMU is still running
    if ! kill -0 "$QEMU_PID" 2>/dev/null; then
        log "QEMU exited"
        break
    fi

    # Check timeout
    if [ $ELAPSED -ge $TIMEOUT ]; then
        log "Timeout reached"
        break
    fi

    # Check log file for patterns
    if [ -f "$SERIAL_LOG" ]; then
        # Check for success patterns
        for pattern in "${SUCCESS_PATTERNS[@]}"; do
            if grep -qi "$pattern" "$SERIAL_LOG" 2>/dev/null; then
                RESULT="success"
                break 2
            fi
        done

        # Check for failure patterns
        for pattern in "${FAILURE_PATTERNS[@]}"; do
            if grep -qi "$pattern" "$SERIAL_LOG" 2>/dev/null; then
                RESULT="failure"
                break 2
            fi
        done

        if [ "$VERBOSE" = true ] && [ -f "$SERIAL_LOG" ]; then
            tail -1 "$SERIAL_LOG" 2>/dev/null || true
        fi
    fi

    sleep 0.5
done

# Cleanup QEMU
cleanup

echo ""
echo "========================================"

# Analyze results
if [ -f "$SERIAL_LOG" ]; then
    LOG_SIZE=$(wc -c < "$SERIAL_LOG")
    LOG_LINES=$(wc -l < "$SERIAL_LOG")
    log "Serial log: $LOG_LINES lines, $LOG_SIZE bytes"

    if [ "$VERBOSE" = true ] || [ "$RESULT" != "success" ]; then
        echo ""
        echo "--- Serial Output (last 20 lines) ---"
        tail -20 "$SERIAL_LOG" 2>/dev/null || echo "(empty)"
        echo "--- End Serial Output ---"
        echo ""
    fi
else
    warn "No serial output captured"
fi

case $RESULT in
    success)
        success "Kernel boot test PASSED"
        echo "========================================"
        exit 0
        ;;
    failure)
        fail "Kernel boot test FAILED - crash detected"
        echo "Check log: $SERIAL_LOG"
        echo "========================================"
        exit 1
        ;;
    timeout)
        # Timeout might still be success if no output expected
        if [ -f "$SERIAL_LOG" ] && [ "$(wc -c < "$SERIAL_LOG")" -gt 0 ]; then
            warn "Boot test inconclusive - timeout with some output"
            echo "Check log: $SERIAL_LOG"
            echo "========================================"
            exit 2
        else
            fail "Kernel boot test FAILED - no output (possible hang)"
            echo "========================================"
            exit 1
        fi
        ;;
esac
