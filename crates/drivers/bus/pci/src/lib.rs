//! PCI Bus Driver for WATOS
//!
//! Provides PCI configuration space access and device enumeration.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use core::arch::asm;
use watos_driver_traits::{Driver, DriverError, DriverInfo, DriverState};
use watos_driver_traits::bus::{PciAddress, PciBar, PciDeviceId, PciDeviceInfo, PciBus, pci_class};

// PCI configuration ports
const PCI_CONFIG_ADDRESS: u16 = 0xCF8;
const PCI_CONFIG_DATA: u16 = 0xCFC;

/// PCI bus driver implementation
pub struct PciDriver {
    state: DriverState,
    devices: Vec<PciDeviceInfo>,
}

impl PciDriver {
    pub const fn new() -> Self {
        PciDriver {
            state: DriverState::Loaded,
            devices: Vec::new(),
        }
    }

    /// Build configuration address for PCI access
    fn config_address(addr: PciAddress, offset: u8) -> u32 {
        0x80000000
            | ((addr.bus as u32) << 16)
            | ((addr.device as u32) << 11)
            | ((addr.function as u32) << 8)
            | ((offset as u32) & 0xFC)
    }

    /// Read 32-bit value from PCI configuration space
    fn pci_read(&self, addr: PciAddress, offset: u8) -> u32 {
        let address = Self::config_address(addr, offset);
        unsafe {
            asm!("out dx, eax", in("dx") PCI_CONFIG_ADDRESS, in("eax") address, options(nostack));
            let value: u32;
            asm!("in eax, dx", in("dx") PCI_CONFIG_DATA, out("eax") value, options(nostack));
            value
        }
    }

    /// Write 32-bit value to PCI configuration space
    fn pci_write(&self, addr: PciAddress, offset: u8, value: u32) {
        let address = Self::config_address(addr, offset);
        unsafe {
            asm!("out dx, eax", in("dx") PCI_CONFIG_ADDRESS, in("eax") address, options(nostack));
            asm!("out dx, eax", in("dx") PCI_CONFIG_DATA, in("eax") value, options(nostack));
        }
    }

    /// Check if a device exists at the given address
    fn device_exists(&self, addr: PciAddress) -> bool {
        let vendor = self.pci_read(addr, 0) & 0xFFFF;
        vendor != 0xFFFF
    }

    /// Read device information
    fn read_device_info(&self, addr: PciAddress) -> Option<PciDeviceInfo> {
        if !self.device_exists(addr) {
            return None;
        }

        let reg0 = self.pci_read(addr, 0);
        let reg2 = self.pci_read(addr, 8);
        let reg3c = self.pci_read(addr, 0x3C);

        let id = PciDeviceId {
            vendor: (reg0 & 0xFFFF) as u16,
            device: ((reg0 >> 16) & 0xFFFF) as u16,
            class: ((reg2 >> 24) & 0xFF) as u8,
            subclass: ((reg2 >> 16) & 0xFF) as u8,
            prog_if: ((reg2 >> 8) & 0xFF) as u8,
            revision: (reg2 & 0xFF) as u8,
        };

        // Read BARs
        let mut bars = [PciBar::None; 6];
        let mut i = 0;
        while i < 6 {
            let bar_offset = 0x10 + (i as u8 * 4);
            let bar = self.pci_read(addr, bar_offset);

            if bar == 0 {
                i += 1;
                continue;
            }

            if bar & 1 == 1 {
                // I/O BAR
                bars[i] = PciBar::Io {
                    port: bar & 0xFFFFFFFC,
                    size: 0, // TODO: probe size
                };
            } else {
                // Memory BAR
                let is_64bit = (bar >> 1) & 3 == 2;
                let prefetchable = (bar >> 3) & 1 == 1;

                let address = if is_64bit && i < 5 {
                    let high = self.pci_read(addr, bar_offset + 4);
                    ((high as u64) << 32) | ((bar & 0xFFFFFFF0) as u64)
                } else {
                    (bar & 0xFFFFFFF0) as u64
                };

                bars[i] = PciBar::Memory {
                    address,
                    size: 0, // TODO: probe size
                    prefetchable,
                    is_64bit,
                };

                if is_64bit {
                    i += 1; // Skip next BAR (used for high bits)
                }
            }
            i += 1;
        }

        Some(PciDeviceInfo {
            address: addr,
            id,
            bars,
            interrupt_line: (reg3c & 0xFF) as u8,
            interrupt_pin: ((reg3c >> 8) & 0xFF) as u8,
        })
    }

    /// Scan all PCI buses
    fn scan_buses(&mut self) {
        self.devices.clear();

        for bus in 0..=255u8 {
            for device in 0..32u8 {
                for function in 0..8u8 {
                    let addr = PciAddress::new(bus, device, function);
                    if let Some(info) = self.read_device_info(addr) {
                        self.devices.push(info);

                        // If function 0 is not multi-function, skip other functions
                        if function == 0 {
                            let header = self.pci_read(addr, 0x0C);
                            if (header >> 16) & 0x80 == 0 {
                                break;
                            }
                        }
                    } else if function == 0 {
                        break; // No device at function 0 means no device at all
                    }
                }
            }
        }
    }
}

impl Driver for PciDriver {
    fn info(&self) -> DriverInfo {
        DriverInfo {
            name: "pci",
            version: "0.1.0",
            author: "WATOS Team",
            description: "PCI bus driver",
        }
    }

    fn state(&self) -> DriverState {
        self.state
    }

    fn init(&mut self) -> Result<(), DriverError> {
        self.scan_buses();
        self.state = DriverState::Ready;
        Ok(())
    }

    fn start(&mut self) -> Result<(), DriverError> {
        self.state = DriverState::Active;
        Ok(())
    }

    fn stop(&mut self) -> Result<(), DriverError> {
        self.state = DriverState::Ready;
        Ok(())
    }
}

impl PciBus for PciDriver {
    fn enumerate(&mut self) -> Vec<PciDeviceInfo> {
        self.devices.clone()
    }

    fn find_by_class(&self, class: u8, subclass: u8) -> Vec<PciDeviceInfo> {
        self.devices
            .iter()
            .filter(|d| d.id.class == class && d.id.subclass == subclass)
            .cloned()
            .collect()
    }

    fn find_by_id(&self, vendor: u16, device: u16) -> Option<PciDeviceInfo> {
        self.devices
            .iter()
            .find(|d| d.id.vendor == vendor && d.id.device == device)
            .cloned()
    }

    fn config_read(&self, addr: PciAddress, offset: u8) -> u32 {
        self.pci_read(addr, offset)
    }

    fn config_write(&self, addr: PciAddress, offset: u8, value: u32) {
        self.pci_write(addr, offset, value);
    }

    fn enable_bus_master(&self, addr: PciAddress) {
        let cmd = self.pci_read(addr, 4);
        self.pci_write(addr, 4, cmd | 0x04);
    }

    fn enable_memory_space(&self, addr: PciAddress) {
        let cmd = self.pci_read(addr, 4);
        self.pci_write(addr, 4, cmd | 0x02);
    }

    fn enable_io_space(&self, addr: PciAddress) {
        let cmd = self.pci_read(addr, 4);
        self.pci_write(addr, 4, cmd | 0x01);
    }
}

/// Find AHCI controller on PCI bus
pub fn find_ahci(pci: &dyn PciBus) -> Option<PciDeviceInfo> {
    let devices = pci.find_by_class(pci_class::MASS_STORAGE, pci_class::SATA);
    devices.into_iter().find(|d| d.id.prog_if == 0x01) // AHCI 1.0
}

/// Find Intel e1000 NIC on PCI bus
pub fn find_e1000(pci: &dyn PciBus) -> Option<PciDeviceInfo> {
    // Intel e1000 vendor ID
    const INTEL_VENDOR: u16 = 0x8086;
    // Common e1000 device IDs
    const E1000_DEVICES: &[u16] = &[
        0x100E, // 82540EM (QEMU default)
        0x100F, // 82545EM
        0x10D3, // 82574L
        0x153A, // I217-LM
    ];

    for &device_id in E1000_DEVICES {
        if let Some(info) = pci.find_by_id(INTEL_VENDOR, device_id) {
            return Some(info);
        }
    }
    None
}
