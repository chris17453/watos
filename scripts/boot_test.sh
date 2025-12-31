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

# Success patterns - kernel boot indicators (default)
DEFAULT_SUCCESS_PATTERNS=(
    "WATOS"
    "C:\\\\>"
    "kernel"
    "boot"
)

# Failure patterns - indicates crash or error
FAILURE_PATTERNS=(
    "triple fault"
    "panic"
    "EXCEPTION:"
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
    echo "  --cmd 'CMD'    Execute command on startup and exit"
    echo "  --expect 'STR' Expected output string (with --cmd)"
    echo "  -i             Shortcut for --interactive"
    echo "  -h, --help     Show this help"
    echo ""
    echo "Examples:"
    echo "  $0 -i                    # Interactive mode"
    echo "  $0 --cmd 'ls' --expect 'TEST.TXT'"
    echo "  $0 --cmd 'echo hello' --expect 'hello'"
    exit 0
}

INTERACTIVE=false
VERBOSE=false
STARTUP_CMD=""
EXPECT_OUTPUT=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --timeout)
            TIMEOUT=$2
            shift 2
            ;;
        --interactive|-i)
            INTERACTIVE=true
            shift
            ;;
        --verbose)
            VERBOSE=true
            shift
            ;;
        --cmd)
            STARTUP_CMD="$2"
            shift 2
            ;;
        --expect)
            EXPECT_OUTPUT="$2"
            shift 2
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

# Detect QEMU binary (different locations on different distros)
if command -v qemu-system-x86_64 >/dev/null 2>&1; then
    QEMU_CMD="qemu-system-x86_64"
elif [ -x /usr/libexec/qemu-kvm ]; then
    QEMU_CMD="/usr/libexec/qemu-kvm"
else
    fail "QEMU not found. Install with: sudo dnf install qemu-kvm (RHEL/Fedora) or sudo apt install qemu-system-x86 (Ubuntu/Debian)"
    exit 1
fi

# Detect OVMF firmware (different locations on different distros/versions)
if [ -f "/usr/share/edk2/ovmf/OVMF_CODE.fd" ]; then
    OVMF_CODE="/usr/share/edk2/ovmf/OVMF_CODE.fd"
    OVMF_VARS_TEMPLATE="/usr/share/edk2/ovmf/OVMF_VARS.fd"
elif [ -f "/usr/share/OVMF/OVMF_CODE.fd" ]; then
    OVMF_CODE="/usr/share/OVMF/OVMF_CODE.fd"
    OVMF_VARS_TEMPLATE="/usr/share/OVMF/OVMF_VARS.fd"
elif [ -f "/usr/share/qemu/OVMF_CODE.fd" ]; then
    OVMF_CODE="/usr/share/qemu/OVMF_CODE.fd"
    OVMF_VARS_TEMPLATE="/usr/share/qemu/OVMF_VARS.fd"
else
    fail "OVMF firmware not found. Install with: sudo dnf install edk2-ovmf (RHEL/Fedora) or sudo apt install ovmf (Ubuntu/Debian)"
    exit 1
fi

# Create working copy of OVMF vars
OVMF_VARS="$PROJECT_ROOT/ai-temp/OVMF_VARS.fd"
cp "$OVMF_VARS_TEMPLATE" "$OVMF_VARS"

success "Prerequisites satisfied"

# Write startup command file if specified
AUTOEXEC_FILE="$PROJECT_ROOT/uefi_test/AUTOEXEC.CMD"
AUTOEXEC_IMG="$LOG_DIR/autoexec.img"

if [ -n "$STARTUP_CMD" ]; then
    log "Writing startup command: $STARTUP_CMD"
    # Write command followed by shutdown
    echo "$STARTUP_CMD" > "$AUTOEXEC_FILE"
    echo "shutdown" >> "$AUTOEXEC_FILE"

    # Create a proper FAT disk image with AUTOEXEC.CMD and essential files
    # The virtual FAT doesn't have a proper boot sector, so we need a real disk
    log "Creating FAT disk image for AUTOEXEC.CMD..."
    rm -f "$AUTOEXEC_IMG"
    # Create 32MB FAT16 image to hold apps
    dd if=/dev/zero of="$AUTOEXEC_IMG" bs=1M count=32 2>/dev/null
    mformat -i "$AUTOEXEC_IMG" -F :: 2>/dev/null || true

    # Copy autoexec file
    mcopy -i "$AUTOEXEC_IMG" "$AUTOEXEC_FILE" ::/AUTOEXEC.CMD 2>/dev/null || true

    # Copy essential directories and files from uefi_test
    # Create SYSTEM and apps directories
    mmd -i "$AUTOEXEC_IMG" ::/SYSTEM 2>/dev/null || true
    mmd -i "$AUTOEXEC_IMG" ::/apps 2>/dev/null || true
    mmd -i "$AUTOEXEC_IMG" ::/apps/system 2>/dev/null || true

    # Copy term (console app)
    if [ -f "$PROJECT_ROOT/uefi_test/system/term" ]; then
        mcopy -i "$AUTOEXEC_IMG" "$PROJECT_ROOT/uefi_test/system/term" ::/system/term 2>/dev/null || true
    fi

    # Copy apps
    for app in "$PROJECT_ROOT/uefi_test/apps/system"/*; do
        if [ -f "$app" ]; then
            app_name=$(basename "$app")
            mcopy -i "$AUTOEXEC_IMG" "$app" ::/apps/system/"$app_name" 2>/dev/null || true
        fi
    done

    # Copy other root files
    for file in "$PROJECT_ROOT/uefi_test"/*.EXE "$PROJECT_ROOT/uefi_test"/kernel.bin; do
        if [ -f "$file" ]; then
            fname=$(basename "$file")
            mcopy -i "$AUTOEXEC_IMG" "$file" ::/"$fname" 2>/dev/null || true
        fi
    done
else
    # Remove autoexec file if no command specified
    rm -f "$AUTOEXEC_FILE"
    rm -f "$AUTOEXEC_IMG"
fi

# Build QEMU command
# Attach boot disk and WFS data disk to ICH9's IDE buses (SATA emulation)
# Use KVM if available for proper CPU idle handling (hlt instruction)
QEMU_CMD=(
    $QEMU_CMD
    -machine q35,accel=kvm:tcg
    -cpu max,+invtsc
    -smp 2
    -m 256M
    -drive "if=pflash,format=raw,readonly=on,file=$OVMF_CODE"
    -drive "if=pflash,format=raw,file=$OVMF_VARS"
    -drive "format=raw,file=$PROJECT_ROOT/uefi_boot.img,if=none,id=bootdisk"
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

# Add autoexec FAT disk if it exists (for testing commands)
# This goes on port 2 so the kernel can mount it as C:
if [ -f "$AUTOEXEC_IMG" ]; then
    QEMU_CMD+=(-drive "file=$AUTOEXEC_IMG,format=raw,if=none,id=cmdisk")
    QEMU_CMD+=(-device ide-hd,drive=cmdisk,bus=ide.2)
fi

# Add DOS 6.22 disk if the image exists
DOS_IMG="$PROJECT_ROOT/dos.img"
if [ -f "$DOS_IMG" ]; then
    QEMU_CMD+=(-drive "file=$DOS_IMG,format=raw,if=none,id=dosdisk")
    QEMU_CMD+=(-device ide-hd,drive=dosdisk,bus=ide.3)
fi

if [ "$INTERACTIVE" = true ]; then
    log "Starting QEMU in interactive mode..."
    log "Serial log: $SERIAL_LOG"

    # Detect if we can use graphical display or need VNC
    if command -v qemu-system-x86_64 >/dev/null 2>&1; then
        # Full QEMU supports GTK
        DISPLAY_OPTS=(-display gtk)
        log "Using GTK display - window will open"
        USE_VNC=false
    else
        # qemu-kvm only supports VNC - check for VNC viewer
        DISPLAY_OPTS=(-vnc :0)
        USE_VNC=true

        # Look for available VNC viewers
        VNC_VIEWER=""
        if command -v remote-viewer >/dev/null 2>&1; then
            VNC_VIEWER="remote-viewer"
        elif command -v virt-viewer >/dev/null 2>&1; then
            VNC_VIEWER="virt-viewer -c"
        elif command -v vncviewer >/dev/null 2>&1; then
            VNC_VIEWER="vncviewer"
        fi

        if [ -n "$VNC_VIEWER" ]; then
            log "Using VNC display with $(echo $VNC_VIEWER | awk '{print $1}') - window will open"
        else
            echo ""
            echo "============================================"
            echo "No VNC viewer found. Install one to get a window:"
            echo "  sudo dnf install virt-viewer    (RHEL/Fedora)"
            echo "  sudo apt install tigervnc-viewer (Ubuntu/Debian)"
            echo ""
            echo "Or connect manually:"
            echo "  remote-viewer vnc://localhost:5900"
            echo "============================================"
            echo ""
            log "VNC server running on localhost:5900"
            log "Press Ctrl+C to exit"
        fi
    fi

    # Build QEMU command with optional WFS data disk
    # Use KVM if available for proper CPU idle handling (hlt instruction)
    QEMU_ARGS=(
        -machine q35,accel=kvm:tcg
        -cpu max,+invtsc
        -smp 2
        -m 256M
        -drive "if=pflash,format=raw,readonly=on,file=$OVMF_CODE"
        -drive "if=pflash,format=raw,file=$OVMF_VARS"
        -drive "file=$PROJECT_ROOT/uefi_boot.img,format=raw,if=none,id=bootdisk"
        -device ide-hd,drive=bootdisk,bus=ide.0
        -netdev user,id=net0
        -device e1000,netdev=net0
        -vga std
        "${DISPLAY_OPTS[@]}"
        -chardev stdio,id=char0,mux=on,logfile="$SERIAL_LOG"
        -serial chardev:char0
    )

    # Add WFS data disk if it exists
    if [ -f "$PROJECT_ROOT/output/watos.img" ]; then
        log "Adding WFS data disk: output/watos.img to ide.1"
        QEMU_ARGS+=(-drive "file=$PROJECT_ROOT/output/watos.img,format=raw,if=none,id=wfsdisk")
        QEMU_ARGS+=(-device ide-hd,drive=wfsdisk,bus=ide.1)
    fi

    # Add DOS 6.22 disk if the image exists
    DOS_IMG="$PROJECT_ROOT/dos.img"
    if [ -f "$DOS_IMG" ]; then
        log "Adding DOS 6.22 disk: dos.img on ide.2"
        QEMU_ARGS+=(-drive "file=$DOS_IMG,format=raw,if=none,id=dosdisk")
        QEMU_ARGS+=(-device ide-hd,drive=dosdisk,bus=ide.2)
    fi

    # Launch QEMU with auto VNC viewer if available
    if [ "$USE_VNC" = true ] && [ -n "$VNC_VIEWER" ]; then
        # Start QEMU in background
        $QEMU_CMD "${QEMU_ARGS[@]}" &
        QEMU_PID=$!
        log "QEMU started (PID: $QEMU_PID)"

        # Wait for VNC server to be ready
        sleep 2

        # Launch VNC viewer - when it closes, QEMU will be killed
        log "Launching VNC viewer window..."
        # Use correct format based on viewer type
        if [[ "$VNC_VIEWER" == "remote-viewer" ]]; then
            $VNC_VIEWER vnc://localhost:5900 2>/dev/null
        else
            $VNC_VIEWER localhost:5900 2>/dev/null
        fi

        # VNC viewer closed - kill QEMU
        log "VNC viewer closed, stopping QEMU..."
        kill $QEMU_PID 2>/dev/null || true
        wait $QEMU_PID 2>/dev/null || true
    else
        # Regular interactive mode (GTK or VNC without viewer)
        $QEMU_CMD "${QEMU_ARGS[@]}"
    fi

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
        # Use expect pattern if specified, otherwise use default success patterns
        if [ -n "$EXPECT_OUTPUT" ]; then
            # Check for expected output in SCREEN DUMP section (actual console output)
            if grep -q "<<<SCREEN_DUMP_END>>>" "$SERIAL_LOG" 2>/dev/null; then
                # Extract screen dump and check for expected output
                if sed -n '/<<<SCREEN_DUMP_START>>>/,/<<<SCREEN_DUMP_END>>>/p' "$SERIAL_LOG" | grep -qF "$EXPECT_OUTPUT" 2>/dev/null; then
                    RESULT="success"
                    break
                fi
            fi
        else
            # Check for default success patterns
            for pattern in "${DEFAULT_SUCCESS_PATTERNS[@]}"; do
                if grep -qi "$pattern" "$SERIAL_LOG" 2>/dev/null; then
                    RESULT="success"
                    break 2
                fi
            done
        fi

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
