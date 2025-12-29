#!/bin/bash
# Create bootable FAT32 disk image from uefi_test directory
# Uses mtools (no root/sudo required)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m'

log() {
    echo -e "${BLUE}[IMG]${NC} $1"
}

success() {
    echo -e "${GREEN}[OK]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if mtools is available
if ! command -v mformat >/dev/null 2>&1; then
    error "mtools not found. Install with: sudo dnf install mtools"
    exit 1
fi

SOURCE_DIR="$PROJECT_ROOT/uefi_test"
IMAGE_FILE="$PROJECT_ROOT/uefi_boot.img"

if [ ! -d "$SOURCE_DIR" ]; then
    error "Source directory not found: $SOURCE_DIR"
    exit 1
fi

log "Creating bootable disk image from $SOURCE_DIR"

# Create a 64MB disk image (plenty of space for 1.7MB content)
log "Creating 64MB disk image..."
dd if=/dev/zero of="$IMAGE_FILE" bs=1M count=64 2>/dev/null

# Format as FAT32 using mtools (no mounting required)
log "Formatting as FAT32..."
mformat -i "$IMAGE_FILE" -F -v "WATOS_BOOT" ::

# Copy all files from uefi_test to the image
log "Copying boot files..."

# Create EFI directory structure
mmd -i "$IMAGE_FILE" ::/EFI 2>/dev/null || true
mmd -i "$IMAGE_FILE" ::/EFI/BOOT 2>/dev/null || true

# Copy bootloader
if [ -f "$SOURCE_DIR/EFI/BOOT/BOOTX64.EFI" ]; then
    mcopy -i "$IMAGE_FILE" "$SOURCE_DIR/EFI/BOOT/BOOTX64.EFI" ::/EFI/BOOT/
    log "  - Copied BOOTX64.EFI"
fi

# Copy kernel
if [ -f "$SOURCE_DIR/kernel.bin" ]; then
    mcopy -i "$IMAGE_FILE" "$SOURCE_DIR/kernel.bin" ::/
    log "  - Copied kernel.bin"
fi

# Copy apps directory if it exists
if [ -d "$SOURCE_DIR/apps" ]; then
    mmd -i "$IMAGE_FILE" ::/apps 2>/dev/null || true
    mmd -i "$IMAGE_FILE" ::/apps/system 2>/dev/null || true

    # Copy all system apps
    for app in "$SOURCE_DIR/apps/system"/*; do
        if [ -f "$app" ]; then
            mcopy -i "$IMAGE_FILE" "$app" ::/apps/system/
        fi
    done
    log "  - Copied apps/system/ directory"
fi

# Copy SYSTEM directory if it exists (contains TERM.EXE)
if [ -d "$SOURCE_DIR/SYSTEM" ]; then
    mmd -i "$IMAGE_FILE" ::/SYSTEM 2>/dev/null || true

    # Copy all SYSTEM apps
    for app in "$SOURCE_DIR/SYSTEM"/*; do
        if [ -f "$app" ]; then
            mcopy -i "$IMAGE_FILE" "$app" ::/SYSTEM/
        fi
    done
    log "  - Copied SYSTEM/ directory"
fi

# Copy AUTOEXEC.CMD if it exists
if [ -f "$SOURCE_DIR/AUTOEXEC.CMD" ]; then
    mcopy -i "$IMAGE_FILE" "$SOURCE_DIR/AUTOEXEC.CMD" ::/
    log "  - Copied AUTOEXEC.CMD"
fi

# Verify the image
log "Verifying disk image..."
mdir -i "$IMAGE_FILE" :: >/dev/null 2>&1 && success "Disk image created successfully: uefi_boot.img ($(du -h "$IMAGE_FILE" | cut -f1))"

exit 0
