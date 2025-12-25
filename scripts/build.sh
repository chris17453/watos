#!/bin/bash
# WATOS Automated Build Script
# Builds both kernel and UEFI bootloader

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

# Defaults
BUILD_TYPE="release"
CLEAN_BUILD=false
VERBOSE=false
CREATE_UEFI_STRUCTURE=true

usage() {
    echo "WATOS Build Script"
    echo ""
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --debug      Build in debug mode"
    echo "  --release    Build in release mode (default)"
    echo "  --clean      Clean before building"
    echo "  --no-uefi    Skip creating UEFI boot structure"
    echo "  --verbose    Verbose output"
    echo "  -h, --help   Show this help"
    exit 0
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --debug)
            BUILD_TYPE="debug"
            shift
            ;;
        --release)
            BUILD_TYPE="release"
            shift
            ;;
        --clean)
            CLEAN_BUILD=true
            shift
            ;;
        --no-uefi)
            CREATE_UEFI_STRUCTURE=false
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

# Ensure Rust environment
export PATH="$HOME/.cargo/bin:$PATH"

log() {
    echo -e "${BLUE}[BUILD]${NC} $1"
}

success() {
    echo -e "${GREEN}[OK]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
    exit 1
}

START_TIME=$(date +%s)

echo "========================================"
echo "WATOS Build System"
echo "========================================"
echo "Build type: $BUILD_TYPE"
echo "Project root: $PROJECT_ROOT"
echo ""

# Clean if requested
if [ "$CLEAN_BUILD" = true ]; then
    log "Cleaning previous build artifacts..."
    rm -rf target/
    rm -f kernel.bin BOOTX64.EFI
    rm -rf uefi_test/
    success "Clean complete"
fi

# Build flags
if [ "$BUILD_TYPE" = "release" ]; then
    CARGO_FLAGS="--release"
else
    CARGO_FLAGS=""
fi

if [ "$VERBOSE" = true ]; then
    CARGO_FLAGS="$CARGO_FLAGS --verbose"
fi

# Step 1: Build kernel (main package at root)
log "Building kernel (target: x86_64-unknown-none)..."
cd "$PROJECT_ROOT"
if CARGO_TARGET_DIR="$PROJECT_ROOT/target" cargo build $CARGO_FLAGS --target x86_64-unknown-none -p watos 2>&1; then
    success "Kernel build complete"
else
    error "Kernel build failed"
fi

# Step 2: Extract kernel binary
log "Extracting kernel binary..."
if [ "$BUILD_TYPE" = "release" ]; then
    KERNEL_ELF="$PROJECT_ROOT/target/x86_64-unknown-none/release/watos"
else
    KERNEL_ELF="$PROJECT_ROOT/target/x86_64-unknown-none/debug/watos"
fi

if [ ! -f "$KERNEL_ELF" ]; then
    error "Kernel ELF not found at $KERNEL_ELF"
fi

rust-objcopy --binary-architecture=i386:x86-64 "$KERNEL_ELF" -O binary "$PROJECT_ROOT/kernel.bin"
success "Kernel binary extracted: kernel.bin ($(du -h "$PROJECT_ROOT/kernel.bin" | cut -f1))"

# Step 3: Build bootloader
log "Building UEFI bootloader (target: x86_64-unknown-uefi)..."
cd "$PROJECT_ROOT"
if CARGO_TARGET_DIR="$PROJECT_ROOT/target" cargo build $CARGO_FLAGS --target x86_64-unknown-uefi -p bootloader --manifest-path src/bootloader/Cargo.toml 2>&1; then
    success "Bootloader build complete"
else
    error "Bootloader build failed"
fi

# Step 4: Copy bootloader
log "Creating UEFI executable..."
if [ "$BUILD_TYPE" = "release" ]; then
    BOOTLOADER_EFI="$PROJECT_ROOT/target/x86_64-unknown-uefi/release/bootloader.efi"
else
    BOOTLOADER_EFI="$PROJECT_ROOT/target/x86_64-unknown-uefi/debug/bootloader.efi"
fi

if [ ! -f "$BOOTLOADER_EFI" ]; then
    error "Bootloader EFI not found at $BOOTLOADER_EFI"
fi

cp "$BOOTLOADER_EFI" "$PROJECT_ROOT/BOOTX64.EFI"
success "UEFI executable: BOOTX64.EFI ($(du -h "$PROJECT_ROOT/BOOTX64.EFI" | cut -f1))"

# Step 5: Create UEFI boot structure
if [ "$CREATE_UEFI_STRUCTURE" = true ]; then
    log "Creating UEFI boot structure..."
    mkdir -p "$PROJECT_ROOT/uefi_test/EFI/BOOT"
    cp "$PROJECT_ROOT/BOOTX64.EFI" "$PROJECT_ROOT/uefi_test/EFI/BOOT/"
    cp "$PROJECT_ROOT/kernel.bin" "$PROJECT_ROOT/uefi_test/"
    success "UEFI structure created in uefi_test/"
fi

# Step 6: Build mkfs.wfs tool (if not already built)
MKFS_WFS="$PROJECT_ROOT/output/mkfs_wfs"
if [ ! -f "$MKFS_WFS" ]; then
    log "Building mkfs.wfs tool..."
    cd "$PROJECT_ROOT/tools/mkfs.wfs"
    if rustup run stable cargo build --release 2>&1; then
        mkdir -p "$PROJECT_ROOT/output"
        cp target/x86_64-unknown-linux-gnu/release/mkfs_wfs "$MKFS_WFS"
        success "mkfs.wfs built"
    else
        echo -e "${YELLOW}[WARN]${NC} mkfs.wfs build failed (optional)"
    fi
    cd "$PROJECT_ROOT"
fi

# Step 7: Create WFS data disk image (if mkfs.wfs available)
if [ -f "$MKFS_WFS" ]; then
    log "Creating WFS data disk image..."
    mkdir -p "$PROJECT_ROOT/rootfs"

    # Create default system files if they don't exist
    [ -f "$PROJECT_ROOT/rootfs/CONFIG.SYS" ] || echo "REM WATOS Configuration" > "$PROJECT_ROOT/rootfs/CONFIG.SYS"
    [ -f "$PROJECT_ROOT/rootfs/AUTOEXEC.BAT" ] || echo "@ECHO WATOS Ready" > "$PROJECT_ROOT/rootfs/AUTOEXEC.BAT"

    "$MKFS_WFS" -o "$PROJECT_ROOT/output/watos.img" -s 64M -d "$PROJECT_ROOT/rootfs"
    success "WFS disk image created: output/watos.img"
fi

END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))

echo ""
echo "========================================"
echo -e "${GREEN}Build Successful!${NC}"
echo "========================================"
echo "Duration: ${DURATION}s"
echo ""
echo "Artifacts:"
echo "  - BOOTX64.EFI      (UEFI bootloader)"
echo "  - kernel.bin       (kernel binary)"
if [ "$CREATE_UEFI_STRUCTURE" = true ]; then
    echo "  - uefi_test/       (bootable structure)"
fi
if [ -f "$PROJECT_ROOT/output/watos.img" ]; then
    echo "  - output/watos.img (WFS data disk)"
fi
echo ""
echo "Next steps:"
echo "  ./scripts/test.sh        - Run automated tests"
echo "  ./scripts/boot_test.sh   - Run QEMU boot test"
