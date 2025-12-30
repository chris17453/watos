#!/bin/bash
# Test script to verify ls command doesn't cause reboot

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

LOG_FILE="ai-temp/logs/test_ls_$(date +%Y%m%d_%H%M%S).log"
mkdir -p ai-temp/logs

echo "Testing ls command execution..."

# Start QEMU and send commands
timeout 15 qemu-system-x86_64 \
    -machine q35 \
    -cpu qemu64 \
    -m 512M \
    -serial file:"$LOG_FILE" \
    -nographic \
    -drive if=pflash,format=raw,readonly=on,file=/usr/share/edk2/ovmf/OVMF_CODE.fd \
    -drive format=raw,file=uefi_boot.img \
    -device ahci,id=ahci0 \
    -drive if=none,id=datadisk,format=raw,file=output/watos.img \
    -device ide-hd,drive=datadisk,bus=ahci0.0 \
    2>&1 >/dev/null &

QEMU_PID=$!
sleep 8  # Wait for boot

# Kill QEMU
kill $QEMU_PID 2>/dev/null
wait $QEMU_PID 2>/dev/null

# Check log
echo ""
echo "Checking for parent resumption in log..."
if grep -q "Resuming parent" "$LOG_FILE"; then
    echo "✓ Found 'Resuming parent' - context switch occurred"
    grep "Resuming parent" "$LOG_FILE"
else
    echo "✗ Did not find 'Resuming parent' message"
fi

echo ""
echo "Checking if system rebooted..."
BOOT_COUNT=$(grep -c "WATOS UEFI Bootloader" "$LOG_FILE")
echo "Boot count: $BOOT_COUNT"
if [ "$BOOT_COUNT" -gt 1 ]; then
    echo "✗ FAILED: System rebooted (found $BOOT_COUNT boot sequences)"
    exit 1
else
    echo "✓ PASS: No reboot detected"
fi

echo ""
echo "Full log saved to: $LOG_FILE"
