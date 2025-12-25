#!/bin/bash
# WATOS Dependency Checker
# Verifies all required tools are installed

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

MISSING=0

check_command() {
    local cmd=$1
    local install_hint=$2
    if command -v "$cmd" >/dev/null 2>&1; then
        echo -e "${GREEN}[OK]${NC} $cmd"
        return 0
    else
        echo -e "${RED}[MISSING]${NC} $cmd - $install_hint"
        MISSING=$((MISSING + 1))
        return 1
    fi
}

check_rustup_component() {
    local component=$1
    if rustup component list 2>/dev/null | grep -q "^${component}.*installed"; then
        echo -e "${GREEN}[OK]${NC} rustup: $component"
        return 0
    else
        echo -e "${RED}[MISSING]${NC} rustup: $component - rustup component add $component"
        MISSING=$((MISSING + 1))
        return 1
    fi
}

check_rustup_target() {
    local target=$1
    if rustup target list 2>/dev/null | grep -q "^${target}.*installed"; then
        echo -e "${GREEN}[OK]${NC} target: $target"
        return 0
    else
        echo -e "${RED}[MISSING]${NC} target: $target - rustup target add $target"
        MISSING=$((MISSING + 1))
        return 1
    fi
}

check_file() {
    local file=$1
    local install_hint=$2
    if [ -f "$file" ]; then
        echo -e "${GREEN}[OK]${NC} $file"
        return 0
    else
        echo -e "${RED}[MISSING]${NC} $file - $install_hint"
        MISSING=$((MISSING + 1))
        return 1
    fi
}

echo "========================================"
echo "WATOS Build Dependency Check"
echo "========================================"
echo ""

echo "=== Core Tools ==="
check_command "rustc" "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
check_command "cargo" "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
check_command "rustup" "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"

echo ""
echo "=== Rust Toolchain ==="
# Check for nightly
if rustup toolchain list 2>/dev/null | grep -q "nightly"; then
    echo -e "${GREEN}[OK]${NC} rustup: nightly toolchain"
else
    echo -e "${RED}[MISSING]${NC} rustup: nightly toolchain - rustup toolchain install nightly"
    MISSING=$((MISSING + 1))
fi

echo ""
echo "=== Rust Components ==="
check_rustup_component "rust-src"
check_rustup_component "llvm-tools"

echo ""
echo "=== Rust Targets ==="
check_rustup_target "x86_64-unknown-uefi"

echo ""
echo "=== Binary Tools ==="
check_command "rust-objcopy" "cargo install cargo-binutils && rustup component add llvm-tools"

echo ""
echo "=== Testing Tools ==="
check_command "qemu-system-x86_64" "sudo dnf install qemu-system-x86"

echo ""
echo "=== UEFI Firmware ==="
check_file "/usr/share/OVMF/OVMF_CODE.fd" "sudo dnf install edk2-ovmf"
check_file "/usr/share/OVMF/OVMF_VARS.fd" "sudo dnf install edk2-ovmf"

echo ""
echo "========================================"
if [ $MISSING -eq 0 ]; then
    echo -e "${GREEN}All dependencies satisfied!${NC}"
    exit 0
else
    echo -e "${RED}Missing $MISSING dependencies${NC}"
    echo ""
    echo "Quick install (Fedora):"
    echo "  sudo dnf install qemu-system-x86 edk2-ovmf"
    echo "  rustup toolchain install nightly"
    echo "  rustup component add rust-src llvm-tools"
    echo "  rustup target add x86_64-unknown-uefi"
    echo "  cargo install cargo-binutils"
    exit 1
fi
