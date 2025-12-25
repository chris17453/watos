#!/bin/bash
# WATOS Container Runner
# Build and run DOS64 in a container

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

log() { echo -e "${BLUE}[CONTAINER]${NC} $1"; }
success() { echo -e "${GREEN}[OK]${NC} $1"; }
fail() { echo -e "${RED}[ERROR]${NC} $1"; exit 1; }

usage() {
    echo "WATOS Container Runner"
    echo ""
    echo "Usage: $0 [command]"
    echo ""
    echo "Commands:"
    echo "  build    - Build the container image"
    echo "  run      - Run WATOS in container (interactive)"
    echo "  daemon   - Run WATOS as background daemon"
    echo "  stop     - Stop running container"
    echo "  logs     - Show container logs"
    echo "  shell    - Get shell in container"
    echo ""
    exit 0
}

# Ensure UEFI boot structure exists
check_build() {
    if [ ! -f "$PROJECT_ROOT/uefi_test/EFI/BOOT/BOOTX64.EFI" ]; then
        log "UEFI boot structure not found. Building first..."
        ./scripts/build.sh || fail "Build failed"
    fi

    # Ensure OVMF_VARS exists
    if [ ! -f "$PROJECT_ROOT/ai-temp/OVMF_VARS.fd" ]; then
        mkdir -p "$PROJECT_ROOT/ai-temp"
        cp /usr/share/OVMF/OVMF_VARS.fd "$PROJECT_ROOT/ai-temp/OVMF_VARS.fd"
    fi
}

case "${1:-run}" in
    build)
        check_build
        log "Building container image..."
        docker build -t watos:latest .
        success "Container image built: watos:latest"
        ;;
    run)
        check_build
        log "Running WATOS in container (interactive)..."
        log "Ports: 8080 (HTTP), 2323 (Telnet)"
        log "Type 'exit' in DOS64 to shutdown"
        echo ""
        docker run -it --rm \
            -p 8080:8080 \
            -p 2323:2323 \
            --name watos-dos64 \
            watos:latest
        ;;
    daemon)
        check_build
        log "Starting WATOS as daemon..."
        docker run -d \
            -p 8080:8080 \
            -p 2323:2323 \
            --name watos-dos64 \
            watos:latest
        success "WATOS running in background"
        log "Use '$0 logs' to see output"
        log "Use '$0 stop' to stop"
        ;;
    stop)
        log "Stopping WATOS container..."
        docker stop watos-dos64 2>/dev/null || true
        docker rm watos-dos64 2>/dev/null || true
        success "Container stopped"
        ;;
    logs)
        docker logs -f watos-dos64
        ;;
    shell)
        docker exec -it watos-dos64 /bin/bash
        ;;
    -h|--help|help)
        usage
        ;;
    *)
        echo "Unknown command: $1"
        usage
        ;;
esac
