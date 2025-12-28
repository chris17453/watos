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

$(rustc --print sysroot)/lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-objcopy --binary-architecture=i386:x86-64 "$KERNEL_ELF" -O binary "$PROJECT_ROOT/kernel.bin"
success "Kernel binary extracted: kernel.bin ($(du -h "$PROJECT_ROOT/kernel.bin" | cut -f1))"

# Step 3: Build bootloader
log "Building UEFI bootloader (target: x86_64-unknown-uefi)..."
cd "$PROJECT_ROOT"
if CARGO_TARGET_DIR="$PROJECT_ROOT/target" cargo build $CARGO_FLAGS --target x86_64-unknown-uefi -p bootloader --manifest-path crates/boot/Cargo.toml 2>&1; then
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

# Step 7: Build WATOS native applications
log "Building WATOS native applications..."
mkdir -p "$PROJECT_ROOT/rootfs/BIN"

# Build GWBASIC for WATOS
log "Building GWBASIC for WATOS..."
cd "$PROJECT_ROOT/crates/apps/gwbasic"

# Build the library
if CARGO_TARGET_DIR="$PROJECT_ROOT/target" cargo build $CARGO_FLAGS \
    --target x86_64-unknown-none \
    --no-default-features \
    --features watos \
    --lib 2>&1; then
    success "GWBASIC library built"
else
    echo -e "${YELLOW}[WARN]${NC} GWBASIC library build failed (optional)"
fi

# Build the executable binary with linker script for proper load address (0x400000)
GWBASIC_RUSTFLAGS="-C link-arg=-T$PROJECT_ROOT/crates/apps/gwbasic/linker.ld -C relocation-model=static"
if RUSTFLAGS="$GWBASIC_RUSTFLAGS" CARGO_TARGET_DIR="$PROJECT_ROOT/target" cargo build $CARGO_FLAGS \
    --target x86_64-unknown-none \
    --no-default-features \
    --features watos \
    --bin gwbasic 2>&1; then

    # Copy to rootfs/BIN
    if [ "$BUILD_TYPE" = "release" ]; then
        GWBASIC_BIN="$PROJECT_ROOT/target/x86_64-unknown-none/release/gwbasic"
    else
        GWBASIC_BIN="$PROJECT_ROOT/target/x86_64-unknown-none/debug/gwbasic"
    fi

    if [ -f "$GWBASIC_BIN" ]; then
        # Copy to rootfs root (mkfs_wfs doesn't support subdirs yet)
        cp "$GWBASIC_BIN" "$PROJECT_ROOT/rootfs/GWBASIC.EXE"
        # Also copy to uefi_test for FAT filesystem boot
        cp "$GWBASIC_BIN" "$PROJECT_ROOT/uefi_test/GWBASIC.EXE"
        success "GWBASIC binary built and copied to rootfs and uefi_test ($(du -h "$GWBASIC_BIN" | cut -f1))"
    fi
else
    echo -e "${YELLOW}[WARN]${NC} GWBASIC binary build failed (optional)"
fi
cd "$PROJECT_ROOT"

# Create /apps/system directory for system utilities
mkdir -p "$PROJECT_ROOT/rootfs/apps/system"
mkdir -p "$PROJECT_ROOT/uefi_test/apps/system"

# Build echo application
log "Building echo for WATOS..."
cd "$PROJECT_ROOT/crates/apps/echo"

# Build with linker script for proper load address (0x1000000 = 16MB, avoids kernel at 0x100000)
ECHO_RUSTFLAGS="-C link-arg=-T$PROJECT_ROOT/crates/apps/echo/src/linker.ld -C relocation-model=static"
if RUSTFLAGS="$ECHO_RUSTFLAGS" CARGO_TARGET_DIR="$PROJECT_ROOT/target" cargo build $CARGO_FLAGS \
    --target x86_64-unknown-none \
    --no-default-features \
    --bin echo 2>&1; then

    if [ "$BUILD_TYPE" = "release" ]; then
        ECHO_BIN="$PROJECT_ROOT/target/x86_64-unknown-none/release/echo"
    else
        ECHO_BIN="$PROJECT_ROOT/target/x86_64-unknown-none/debug/echo"
    fi

    if [ -f "$ECHO_BIN" ]; then
        cp "$ECHO_BIN" "$PROJECT_ROOT/rootfs/apps/system/echo"
        cp "$ECHO_BIN" "$PROJECT_ROOT/uefi_test/apps/system/echo"
        success "echo built -> /apps/system/echo ($(du -h "$ECHO_BIN" | cut -f1))"
    fi
else
    echo -e "${YELLOW}[WARN]${NC} echo build failed (optional)"
fi
cd "$PROJECT_ROOT"

# Build date application
log "Building date for WATOS..."
cd "$PROJECT_ROOT/crates/apps/date"

# Build with linker script for proper load address (0x1000000 = 16MB, avoids kernel at 0x100000)
DATE_RUSTFLAGS="-C link-arg=-T$PROJECT_ROOT/crates/apps/date/src/linker.ld -C relocation-model=static"
if RUSTFLAGS="$DATE_RUSTFLAGS" CARGO_TARGET_DIR="$PROJECT_ROOT/target" cargo build $CARGO_FLAGS \
    --target x86_64-unknown-none \
    --no-default-features \
    --bin date 2>&1; then

    if [ "$BUILD_TYPE" = "release" ]; then
        DATE_BIN="$PROJECT_ROOT/target/x86_64-unknown-none/release/date"
    else
        DATE_BIN="$PROJECT_ROOT/target/x86_64-unknown-none/debug/date"
    fi

    if [ -f "$DATE_BIN" ]; then
        cp "$DATE_BIN" "$PROJECT_ROOT/rootfs/apps/system/date"
        cp "$DATE_BIN" "$PROJECT_ROOT/uefi_test/apps/system/date"
        success "date built -> /apps/system/date ($(du -h "$DATE_BIN" | cut -f1))"
    fi
else
    echo -e "${YELLOW}[WARN]${NC} date build failed (optional)"
fi
cd "$PROJECT_ROOT"

# Build clear application
log "Building clear for WATOS..."
cd "$PROJECT_ROOT/crates/apps/clear"

CLEAR_RUSTFLAGS="-C link-arg=-T$PROJECT_ROOT/crates/apps/watos-app.ld -C relocation-model=static"
if RUSTFLAGS="$CLEAR_RUSTFLAGS" CARGO_TARGET_DIR="$PROJECT_ROOT/target" cargo build $CARGO_FLAGS \
    --target x86_64-unknown-none \
    --bin clear 2>&1; then

    if [ "$BUILD_TYPE" = "release" ]; then
        CLEAR_BIN="$PROJECT_ROOT/target/x86_64-unknown-none/release/clear"
    else
        CLEAR_BIN="$PROJECT_ROOT/target/x86_64-unknown-none/debug/clear"
    fi

    if [ -f "$CLEAR_BIN" ]; then
        cp "$CLEAR_BIN" "$PROJECT_ROOT/rootfs/apps/system/clear"
        cp "$CLEAR_BIN" "$PROJECT_ROOT/uefi_test/apps/system/clear"
        success "clear built -> /apps/system/clear ($(du -h "$CLEAR_BIN" | cut -f1))"
    fi
else
    echo -e "${YELLOW}[WARN]${NC} clear build failed (optional)"
fi
cd "$PROJECT_ROOT"

# Build uname application
log "Building uname for WATOS..."
cd "$PROJECT_ROOT/crates/apps/uname"

UNAME_RUSTFLAGS="-C link-arg=-T$PROJECT_ROOT/crates/apps/watos-app.ld -C relocation-model=static"
if RUSTFLAGS="$UNAME_RUSTFLAGS" CARGO_TARGET_DIR="$PROJECT_ROOT/target" cargo build $CARGO_FLAGS \
    --target x86_64-unknown-none \
    --bin uname 2>&1; then

    if [ "$BUILD_TYPE" = "release" ]; then
        UNAME_BIN="$PROJECT_ROOT/target/x86_64-unknown-none/release/uname"
    else
        UNAME_BIN="$PROJECT_ROOT/target/x86_64-unknown-none/debug/uname"
    fi

    if [ -f "$UNAME_BIN" ]; then
        cp "$UNAME_BIN" "$PROJECT_ROOT/rootfs/apps/system/uname"
        cp "$UNAME_BIN" "$PROJECT_ROOT/uefi_test/apps/system/uname"
        success "uname built -> /apps/system/uname ($(du -h "$UNAME_BIN" | cut -f1))"
    fi
else
    echo -e "${YELLOW}[WARN]${NC} uname build failed (optional)"
fi
cd "$PROJECT_ROOT"

# Build uptime application
log "Building uptime for WATOS..."
cd "$PROJECT_ROOT/crates/apps/uptime"

UPTIME_RUSTFLAGS="-C link-arg=-T$PROJECT_ROOT/crates/apps/watos-app.ld -C relocation-model=static"
if RUSTFLAGS="$UPTIME_RUSTFLAGS" CARGO_TARGET_DIR="$PROJECT_ROOT/target" cargo build $CARGO_FLAGS \
    --target x86_64-unknown-none \
    --bin uptime 2>&1; then

    if [ "$BUILD_TYPE" = "release" ]; then
        UPTIME_BIN="$PROJECT_ROOT/target/x86_64-unknown-none/release/uptime"
    else
        UPTIME_BIN="$PROJECT_ROOT/target/x86_64-unknown-none/debug/uptime"
    fi

    if [ -f "$UPTIME_BIN" ]; then
        cp "$UPTIME_BIN" "$PROJECT_ROOT/rootfs/apps/system/uptime"
        cp "$UPTIME_BIN" "$PROJECT_ROOT/uefi_test/apps/system/uptime"
        success "uptime built -> /apps/system/uptime ($(du -h "$UPTIME_BIN" | cut -f1))"
    fi
else
    echo -e "${YELLOW}[WARN]${NC} uptime build failed (optional)"
fi
cd "$PROJECT_ROOT"

# Build ps application
log "Building ps for WATOS..."
cd "$PROJECT_ROOT/crates/apps/ps"

PS_RUSTFLAGS="-C link-arg=-T$PROJECT_ROOT/crates/apps/watos-app.ld -C relocation-model=static"
if RUSTFLAGS="$PS_RUSTFLAGS" CARGO_TARGET_DIR="$PROJECT_ROOT/target" cargo build $CARGO_FLAGS \
    --target x86_64-unknown-none \
    --bin ps 2>&1; then

    if [ "$BUILD_TYPE" = "release" ]; then
        PS_BIN="$PROJECT_ROOT/target/x86_64-unknown-none/release/ps"
    else
        PS_BIN="$PROJECT_ROOT/target/x86_64-unknown-none/debug/ps"
    fi

    if [ -f "$PS_BIN" ]; then
        cp "$PS_BIN" "$PROJECT_ROOT/rootfs/apps/system/ps"
        cp "$PS_BIN" "$PROJECT_ROOT/uefi_test/apps/system/ps"
        success "ps built -> /apps/system/ps ($(du -h "$PS_BIN" | cut -f1))"
    fi
else
    echo -e "${YELLOW}[WARN]${NC} ps build failed (optional)"
fi
cd "$PROJECT_ROOT"

# Build console/terminal application (TERM.EXE)
log "Building console (TERM.EXE) for WATOS..."
cd "$PROJECT_ROOT/crates/apps/console"

# Build with linker script for proper load address (0x400000)
CONSOLE_RUSTFLAGS="-C link-arg=-T$PROJECT_ROOT/crates/apps/console/linker.ld -C relocation-model=static"
if RUSTFLAGS="$CONSOLE_RUSTFLAGS" CARGO_TARGET_DIR="$PROJECT_ROOT/target" cargo build $CARGO_FLAGS \
    --target x86_64-unknown-none \
    --bin console 2>&1; then

    # Copy to rootfs and uefi_test
    if [ "$BUILD_TYPE" = "release" ]; then
        CONSOLE_BIN="$PROJECT_ROOT/target/x86_64-unknown-none/release/console"
    else
        CONSOLE_BIN="$PROJECT_ROOT/target/x86_64-unknown-none/debug/console"
    fi

    if [ -f "$CONSOLE_BIN" ]; then
        # Create SYSTEM folder for system apps
        mkdir -p "$PROJECT_ROOT/rootfs/SYSTEM"
        mkdir -p "$PROJECT_ROOT/uefi_test/SYSTEM"
        # Copy as TERM.EXE - the terminal emulator (autostart)
        cp "$CONSOLE_BIN" "$PROJECT_ROOT/rootfs/SYSTEM/TERM.EXE"
        cp "$CONSOLE_BIN" "$PROJECT_ROOT/uefi_test/SYSTEM/TERM.EXE"
        success "Console (SYSTEM/TERM.EXE) built and copied to rootfs and uefi_test ($(du -h "$CONSOLE_BIN" | cut -f1))"
    fi
else
    echo -e "${YELLOW}[WARN]${NC} Console binary build failed (optional)"
fi
cd "$PROJECT_ROOT"

# Auto-discover and build other WATOS applications
log "Auto-discovering WATOS applications..."
for app_dir in "$PROJECT_ROOT/crates"/*; do
    if [ -d "$app_dir" ] && [ -f "$app_dir/Cargo.toml" ]; then
        app_name=$(basename "$app_dir")
        
        # Skip already processed apps and non-WATOS crates
        if [ "$app_name" = "gwbasic" ] || [ "$app_name" = "echo" ] || [ "$app_name" = "console" ] || [ "$app_name" = "watos-syscall" ]; then
            continue
        fi
        
        # Check if this is a WATOS application (has watos feature or dependency)
        if grep -q "watos" "$app_dir/Cargo.toml" 2>/dev/null; then
            log "Building WATOS application: $app_name..."
            cd "$app_dir"
            
            if CARGO_TARGET_DIR="$PROJECT_ROOT/target" cargo build $CARGO_FLAGS \
                --target x86_64-unknown-none \
                --no-default-features \
                --bin "$app_name" 2>&1; then
                
                # Copy to rootfs and uefi_test
                if [ "$BUILD_TYPE" = "release" ]; then
                    APP_BIN="$PROJECT_ROOT/target/x86_64-unknown-none/release/$app_name"
                else
                    APP_BIN="$PROJECT_ROOT/target/x86_64-unknown-none/debug/$app_name"
                fi
                
                if [ -f "$APP_BIN" ]; then
                    # Convert to uppercase for DOS-style naming
                    APP_NAME_UPPER=$(echo "$app_name" | tr '[:lower:]' '[:upper:]')
                    cp "$APP_BIN" "$PROJECT_ROOT/rootfs/${APP_NAME_UPPER}.EXE"
                    cp "$APP_BIN" "$PROJECT_ROOT/uefi_test/${APP_NAME_UPPER}.EXE"
                    success "$app_name binary built and copied ($(du -h "$APP_BIN" | cut -f1))"
                fi
            else
                echo -e "${YELLOW}[WARN]${NC} $app_name build failed (optional)"
            fi
            cd "$PROJECT_ROOT"
        fi
    fi
done

# Step 8: Create WFS data disk image (if mkfs.wfs available)
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
echo "Built Applications:"
if [ -f "$PROJECT_ROOT/rootfs/SYSTEM/TERM.EXE" ]; then
    echo "  - SYSTEM/TERM.EXE  (Terminal emulator - autostart)"
fi
if [ -f "$PROJECT_ROOT/rootfs/GWBASIC.EXE" ]; then
    echo "  - GWBASIC.EXE      (GWBASIC interpreter)"
fi
if [ -f "$PROJECT_ROOT/rootfs/ECHO.EXE" ]; then
    echo "  - ECHO.EXE         (Echo utility)"
fi
# Show any other discovered applications in root
for app_file in "$PROJECT_ROOT/rootfs"/*.EXE; do
    if [ -f "$app_file" ]; then
        app_filename=$(basename "$app_file")
        if [ "$app_filename" != "GWBASIC.EXE" ] && [ "$app_filename" != "ECHO.EXE" ]; then
            echo "  - $app_filename"
        fi
    fi
done
# Show SYSTEM folder apps
for app_file in "$PROJECT_ROOT/rootfs/SYSTEM"/*.EXE; do
    if [ -f "$app_file" ]; then
        app_filename=$(basename "$app_file")
        if [ "$app_filename" != "TERM.EXE" ]; then
            echo "  - SYSTEM/$app_filename"
        fi
    fi
done
echo ""
echo "Next steps:"
echo "  ./scripts/test.sh        - Run automated tests"
echo "  ./scripts/boot_test.sh   - Run QEMU boot test"
