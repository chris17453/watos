//! PCI Bus Enumeration
//!
//! Scans PCI configuration space to find devices

use core::arch::asm;

// PCI Configuration Space ports
const PCI_CONFIG_ADDR: u16 = 0xCF8;
const PCI_CONFIG_DATA: u16 = 0xCFC;

// PCI Configuration Space offsets
pub const PCI_VENDOR_ID: u8 = 0x00;
pub const PCI_DEVICE_ID: u8 = 0x02;
pub const PCI_COMMAND: u8 = 0x04;
pub const PCI_STATUS: u8 = 0x06;
pub const PCI_CLASS: u8 = 0x0B;
pub const PCI_SUBCLASS: u8 = 0x0A;
pub const PCI_HEADER_TYPE: u8 = 0x0E;
pub const PCI_BAR0: u8 = 0x10;
pub const PCI_BAR1: u8 = 0x14;
pub const PCI_BAR2: u8 = 0x18;
pub const PCI_BAR3: u8 = 0x1C;
pub const PCI_BAR4: u8 = 0x20;
pub const PCI_BAR5: u8 = 0x24;
pub const PCI_INTERRUPT_LINE: u8 = 0x3C;

// PCI Command register bits
pub const PCI_CMD_IO_SPACE: u16 = 0x0001;
pub const PCI_CMD_MEMORY_SPACE: u16 = 0x0002;
pub const PCI_CMD_BUS_MASTER: u16 = 0x0004;
pub const PCI_CMD_INTERRUPT_DISABLE: u16 = 0x0400;

// Known device IDs
pub const INTEL_VENDOR_ID: u16 = 0x8086;
pub const E1000_DEVICE_ID: u16 = 0x100E;      // 82545EM (QEMU default)
pub const E1000_DEVICE_ID_ALT: u16 = 0x100F;  // 82545EM (alternate)
pub const E1000_DEVICE_ID_I217: u16 = 0x153A; // I217-LM

/// PCI device location
#[derive(Debug, Clone, Copy)]
pub struct PciDevice {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class: u8,
    pub subclass: u8,
}

impl PciDevice {
    /// Get the base address from a BAR
    pub fn bar(&self, bar_num: u8) -> u32 {
        let offset = PCI_BAR0 + (bar_num * 4);
        pci_read_config_dword(self.bus, self.device, self.function, offset)
    }

    /// Get memory-mapped IO base address (BAR0 for e1000)
    pub fn mmio_base(&self) -> u64 {
        self.bar_mmio(0)
    }

    /// Get AHCI Base Address Register (BAR5) for AHCI controllers
    pub fn ahci_base(&self) -> u64 {
        self.bar_mmio(5)
    }

    /// Get memory-mapped IO base from a specific BAR
    fn bar_mmio(&self, bar_num: u8) -> u64 {
        let bar = self.bar(bar_num);

        // Check if it's a memory BAR (bit 0 = 0)
        if bar & 1 == 0 {
            // Check if 64-bit BAR (bits 1-2 = 10)
            if (bar >> 1) & 3 == 2 {
                let bar_hi = self.bar(bar_num + 1);
                // 64-bit address
                ((bar_hi as u64) << 32) | ((bar & 0xFFFFFFF0) as u64)
            } else {
                // 32-bit address
                (bar & 0xFFFFFFF0) as u64
            }
        } else {
            // I/O BAR
            0
        }
    }

    /// Enable bus mastering and memory space access
    pub fn enable(&self) {
        let cmd = pci_read_config_word(self.bus, self.device, self.function, PCI_COMMAND);
        let new_cmd = cmd | PCI_CMD_MEMORY_SPACE | PCI_CMD_BUS_MASTER;
        pci_write_config_word(self.bus, self.device, self.function, PCI_COMMAND, new_cmd);
    }

    /// Get interrupt line
    pub fn interrupt_line(&self) -> u8 {
        pci_read_config_byte(self.bus, self.device, self.function, PCI_INTERRUPT_LINE)
    }
}

/// Build PCI configuration address
fn pci_config_address(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    ((1u32 << 31) |                    // Enable bit
     ((bus as u32) << 16) |            // Bus number
     ((device as u32) << 11) |         // Device number
     ((function as u32) << 8) |        // Function number
     ((offset as u32) & 0xFC))         // Register offset (aligned)
}

/// Write to PCI configuration address port
unsafe fn outl(port: u16, value: u32) {
    asm!("out dx, eax", in("dx") port, in("eax") value, options(nostack, preserves_flags));
}

/// Read from PCI configuration data port
unsafe fn inl(port: u16) -> u32 {
    let value: u32;
    asm!("in eax, dx", out("eax") value, in("dx") port, options(nostack, preserves_flags));
    value
}

/// Read a byte from PCI configuration space
pub fn pci_read_config_byte(bus: u8, device: u8, function: u8, offset: u8) -> u8 {
    let dword = pci_read_config_dword(bus, device, function, offset & 0xFC);
    ((dword >> ((offset & 3) * 8)) & 0xFF) as u8
}

/// Read a word from PCI configuration space
pub fn pci_read_config_word(bus: u8, device: u8, function: u8, offset: u8) -> u16 {
    let dword = pci_read_config_dword(bus, device, function, offset & 0xFC);
    ((dword >> ((offset & 2) * 8)) & 0xFFFF) as u16
}

/// Read a dword from PCI configuration space
pub fn pci_read_config_dword(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    let addr = pci_config_address(bus, device, function, offset);
    unsafe {
        outl(PCI_CONFIG_ADDR, addr);
        inl(PCI_CONFIG_DATA)
    }
}

/// Write a word to PCI configuration space
pub fn pci_write_config_word(bus: u8, device: u8, function: u8, offset: u8, value: u16) {
    let addr = pci_config_address(bus, device, function, offset);
    unsafe {
        outl(PCI_CONFIG_ADDR, addr);
        // Read-modify-write
        let dword = inl(PCI_CONFIG_DATA);
        let shift = (offset & 2) * 8;
        let mask = !(0xFFFF << shift);
        let new_dword = (dword & mask) | ((value as u32) << shift);
        outl(PCI_CONFIG_DATA, new_dword);
    }
}

/// Scan PCI bus for a specific device
pub fn find_device(vendor_id: u16, device_id: u16) -> Option<PciDevice> {
    for bus in 0..=255u8 {
        for device in 0..32u8 {
            for function in 0..8u8 {
                let vid = pci_read_config_word(bus, device, function, PCI_VENDOR_ID);

                // No device present
                if vid == 0xFFFF {
                    continue;
                }

                let did = pci_read_config_word(bus, device, function, PCI_DEVICE_ID);

                if vid == vendor_id && did == device_id {
                    let class = pci_read_config_byte(bus, device, function, PCI_CLASS);
                    let subclass = pci_read_config_byte(bus, device, function, PCI_SUBCLASS);

                    return Some(PciDevice {
                        bus,
                        device,
                        function,
                        vendor_id: vid,
                        device_id: did,
                        class,
                        subclass,
                    });
                }

                // If not a multi-function device, skip other functions
                if function == 0 {
                    let header_type = pci_read_config_byte(bus, device, function, PCI_HEADER_TYPE);
                    if header_type & 0x80 == 0 {
                        break;
                    }
                }
            }
        }
    }
    None
}

/// Find an e1000 network card
pub fn find_e1000() -> Option<PciDevice> {
    // Try common e1000 device IDs
    if let Some(dev) = find_device(INTEL_VENDOR_ID, E1000_DEVICE_ID) {
        return Some(dev);
    }
    if let Some(dev) = find_device(INTEL_VENDOR_ID, E1000_DEVICE_ID_ALT) {
        return Some(dev);
    }
    if let Some(dev) = find_device(INTEL_VENDOR_ID, E1000_DEVICE_ID_I217) {
        return Some(dev);
    }
    None
}

// AHCI/SATA Controller IDs
pub const AHCI_CLASS: u8 = 0x01;        // Mass Storage
pub const AHCI_SUBCLASS: u8 = 0x06;     // SATA
pub const ICH9_AHCI_DEVICE_ID: u16 = 0x2922;  // Intel ICH9 (QEMU Q35)

/// Find device by class/subclass
pub fn find_device_by_class(class: u8, subclass: u8) -> Option<PciDevice> {
    for bus in 0..=255u8 {
        for device in 0..32u8 {
            for function in 0..8u8 {
                let vid = pci_read_config_word(bus, device, function, PCI_VENDOR_ID);

                if vid == 0xFFFF {
                    continue;
                }

                let dev_class = pci_read_config_byte(bus, device, function, PCI_CLASS);
                let dev_subclass = pci_read_config_byte(bus, device, function, PCI_SUBCLASS);

                if dev_class == class && dev_subclass == subclass {
                    let did = pci_read_config_word(bus, device, function, PCI_DEVICE_ID);
                    return Some(PciDevice {
                        bus,
                        device,
                        function,
                        vendor_id: vid,
                        device_id: did,
                        class: dev_class,
                        subclass: dev_subclass,
                    });
                }

                if function == 0 {
                    let header_type = pci_read_config_byte(bus, device, function, PCI_HEADER_TYPE);
                    if header_type & 0x80 == 0 {
                        break;
                    }
                }
            }
        }
    }
    None
}

/// Find AHCI controller
pub fn find_ahci() -> Option<PciDevice> {
    find_device_by_class(AHCI_CLASS, AHCI_SUBCLASS)
}

/// Scan all PCI devices and return list
pub fn scan_all_devices() -> ([PciDevice; 32], usize) {
    let mut devices = [PciDevice {
        bus: 0, device: 0, function: 0,
        vendor_id: 0, device_id: 0, class: 0, subclass: 0,
    }; 32];
    let mut count = 0;

    for bus in 0..=255u8 {
        for device in 0..32u8 {
            for function in 0..8u8 {
                let vid = pci_read_config_word(bus, device, function, PCI_VENDOR_ID);

                if vid == 0xFFFF {
                    continue;
                }

                if count < 32 {
                    let did = pci_read_config_word(bus, device, function, PCI_DEVICE_ID);
                    let class = pci_read_config_byte(bus, device, function, PCI_CLASS);
                    let subclass = pci_read_config_byte(bus, device, function, PCI_SUBCLASS);

                    devices[count] = PciDevice {
                        bus,
                        device,
                        function,
                        vendor_id: vid,
                        device_id: did,
                        class,
                        subclass,
                    };
                    count += 1;
                }

                if function == 0 {
                    let header_type = pci_read_config_byte(bus, device, function, PCI_HEADER_TYPE);
                    if header_type & 0x80 == 0 {
                        break;
                    }
                }
            }
        }
    }
    (devices, count)
}
