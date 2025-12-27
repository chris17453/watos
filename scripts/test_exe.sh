#!/bin/bash
# WATOS EXE Test Script - Test executables without booting the OS

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Build exe-tester if needed
if [ ! -f "target/release/exe-tester" ]; then
    echo -e "${BLUE}Building exe-tester...${NC}"
    cd tools/exe-tester
    cargo build --release
    cd "$PROJECT_ROOT"
    cp tools/exe-tester/target/release/exe-tester target/release/
    echo -e "${GREEN}exe-tester built${NC}"
fi

# Default executable to test
EXE_FILE="${1:-rootfs/ECHO.EXE}"

if [ ! -f "$EXE_FILE" ]; then
    echo -e "${YELLOW}File not found: $EXE_FILE${NC}"
    echo "Usage: $0 [exe-file]"
    echo "Available EXE files:"
    find rootfs uefi_test -name "*.EXE" 2>/dev/null | head -10
    exit 1
fi

echo -e "${BLUE}Testing: $EXE_FILE${NC}\n"

# Run the tester with all checks
./target/release/exe-tester "$EXE_FILE" --dump-entry --check-syscalls
