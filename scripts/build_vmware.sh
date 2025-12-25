#!/bin/bash
# Build WATOS for VMware (ESXi, Workstation, Fusion)
# Creates a VMDK disk image with UEFI boot

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

OUTPUT_DIR="$PROJECT_ROOT/vmware"
DISK_SIZE_MB=64

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

log() { echo -e "${BLUE}[VMWARE]${NC} $1"; }
success() { echo -e "${GREEN}[OK]${NC} $1"; }
fail() { echo -e "${RED}[ERROR]${NC} $1"; exit 1; }

# Check for required tools
check_deps() {
    local missing=()

    command -v qemu-img >/dev/null 2>&1 || missing+=("qemu-img")
    command -v mformat >/dev/null 2>&1 || missing+=("mtools (mformat)")
    command -v mcopy >/dev/null 2>&1 || missing+=("mtools (mcopy)")

    if [ ${#missing[@]} -gt 0 ]; then
        echo "Missing dependencies: ${missing[*]}"
        echo "Install with: sudo dnf install qemu-img mtools"
        exit 1
    fi
}

# Ensure kernel is built
check_build() {
    if [ ! -f "$PROJECT_ROOT/uefi_test/EFI/BOOT/BOOTX64.EFI" ]; then
        log "Building kernel first..."
        ./scripts/build.sh || fail "Build failed"
    fi
}

echo "========================================"
echo "WATOS VMware Image Builder"
echo "========================================"

check_deps
check_build

mkdir -p "$OUTPUT_DIR"

log "Creating ${DISK_SIZE_MB}MB raw disk image..."
dd if=/dev/zero of="$OUTPUT_DIR/watos.img" bs=1M count=$DISK_SIZE_MB status=progress

log "Creating FAT32 filesystem with EFI structure..."
# Create a FAT32 filesystem
mformat -i "$OUTPUT_DIR/watos.img" -F -v WATOS ::

log "Copying boot files..."
# Create EFI directory structure
mmd -i "$OUTPUT_DIR/watos.img" ::/EFI
mmd -i "$OUTPUT_DIR/watos.img" ::/EFI/BOOT

# Copy bootloader and kernel
mcopy -i "$OUTPUT_DIR/watos.img" "$PROJECT_ROOT/uefi_test/EFI/BOOT/BOOTX64.EFI" ::/EFI/BOOT/
mcopy -i "$OUTPUT_DIR/watos.img" "$PROJECT_ROOT/uefi_test/kernel.bin" ::/

log "Converting to VMDK format..."
qemu-img convert -f raw -O vmdk "$OUTPUT_DIR/watos.img" "$OUTPUT_DIR/watos.vmdk"

# Clean up raw image
rm "$OUTPUT_DIR/watos.img"

log "Creating VMX configuration file..."
cat > "$OUTPUT_DIR/watos.vmx" << 'EOF'
.encoding = "UTF-8"
displayName = "WATOS DOS64"
guestOS = "other-64"
memsize = "256"
numvcpus = "2"

# UEFI firmware
firmware = "efi"

# Disk
scsi0.present = "TRUE"
scsi0.virtualDev = "pvscsi"
scsi0:0.present = "TRUE"
scsi0:0.fileName = "watos.vmdk"

# Network - e1000 for compatibility (or vmxnet3 for performance)
ethernet0.present = "TRUE"
ethernet0.virtualDev = "e1000"
ethernet0.connectionType = "nat"
ethernet0.addressType = "generated"
ethernet0.startConnected = "TRUE"

# Serial port (for console output)
serial0.present = "TRUE"
serial0.fileType = "file"
serial0.fileName = "serial.log"
serial0.yieldOnMsrRead = "TRUE"

# Misc
virtualHW.version = "19"
pciBridge0.present = "TRUE"
pciBridge4.present = "TRUE"
pciBridge4.virtualDev = "pcieRootPort"
pciBridge4.functions = "8"
EOF

log "Creating OVF for easy import..."
cat > "$OUTPUT_DIR/watos.ovf" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<Envelope xmlns="http://schemas.dmtf.org/ovf/envelope/1"
          xmlns:ovf="http://schemas.dmtf.org/ovf/envelope/1"
          xmlns:vssd="http://schemas.dmtf.org/wbem/wscim/1/cim-schema/2/CIM_VirtualSystemSettingData"
          xmlns:rasd="http://schemas.dmtf.org/wbem/wscim/1/cim-schema/2/CIM_ResourceAllocationSettingData">
  <References>
    <File ovf:id="watos-disk" ovf:href="watos.vmdk"/>
  </References>
  <DiskSection>
    <Info>Virtual disk</Info>
    <Disk ovf:diskId="watos-disk" ovf:capacity="67108864" ovf:format="http://www.vmware.com/interfaces/specifications/vmdk.html"/>
  </DiskSection>
  <VirtualSystem ovf:id="WATOS">
    <Info>WATOS DOS64 - 64-bit DOS-compatible OS</Info>
    <Name>WATOS DOS64</Name>
    <OperatingSystemSection ovf:id="100">
      <Info>64-bit OS</Info>
    </OperatingSystemSection>
    <VirtualHardwareSection>
      <Info>Virtual Hardware</Info>
      <System>
        <vssd:VirtualSystemType>vmx-19</vssd:VirtualSystemType>
      </System>
      <Item>
        <rasd:Description>256MB RAM</rasd:Description>
        <rasd:ResourceType>4</rasd:ResourceType>
        <rasd:VirtualQuantity>256</rasd:VirtualQuantity>
      </Item>
      <Item>
        <rasd:Description>2 vCPUs</rasd:Description>
        <rasd:ResourceType>3</rasd:ResourceType>
        <rasd:VirtualQuantity>2</rasd:VirtualQuantity>
      </Item>
    </VirtualHardwareSection>
  </VirtualSystem>
</Envelope>
EOF

success "VMware image created!"
echo ""
echo "========================================"
echo "Output files in $OUTPUT_DIR:"
echo "  - watos.vmdk  (disk image)"
echo "  - watos.vmx   (VMware Workstation/Fusion config)"
echo "  - watos.ovf   (for ESXi/vCenter import)"
echo ""
echo "Usage:"
echo "  VMware Workstation/Fusion: Open watos.vmx"
echo "  ESXi/vCenter: Import watos.ovf + watos.vmdk"
echo "========================================"
