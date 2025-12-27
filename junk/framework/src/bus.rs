//! Bus driver abstraction
//!
//! For PCI, USB, and other bus controllers

use crate::Driver;
use alloc::vec::Vec;

/// PCI device address
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciAddress {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
}

impl PciAddress {
    pub fn new(bus: u8, device: u8, function: u8) -> Self {
        PciAddress { bus, device, function }
    }
}

/// PCI device identification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciDeviceId {
    pub vendor: u16,
    pub device: u16,
    pub class: u8,
    pub subclass: u8,
    pub prog_if: u8,
}

/// PCI Base Address Register (BAR)
#[derive(Debug, Clone, Copy)]
pub enum PciBar {
    /// Memory-mapped BAR
    Memory {
        address: u64,
        size: u64,
        prefetchable: bool,
        is_64bit: bool,
    },
    /// I/O port BAR
    Io {
        port: u16,
        size: u16,
    },
    /// BAR not present
    None,
}

/// PCI device information
#[derive(Debug, Clone)]
pub struct PciDeviceInfo {
    pub address: PciAddress,
    pub id: PciDeviceId,
    pub bars: [PciBar; 6],
    pub interrupt_line: u8,
    pub interrupt_pin: u8,
}

/// PCI bus driver trait
pub trait PciBus: Driver {
    /// Enumerate all devices on the bus
    fn enumerate(&self) -> Vec<PciDeviceInfo>;

    /// Find devices by class
    fn find_by_class(&self, class: u8, subclass: u8) -> Vec<PciDeviceInfo>;

    /// Find device by vendor/device ID
    fn find_by_id(&self, vendor: u16, device: u16) -> Option<PciDeviceInfo>;

    /// Read configuration space
    fn config_read(&self, addr: PciAddress, offset: u8) -> u32;

    /// Write configuration space
    fn config_write(&self, addr: PciAddress, offset: u8, value: u32);

    /// Enable bus mastering for a device
    fn enable_bus_master(&self, addr: PciAddress);

    /// Enable memory space access for a device
    fn enable_memory_space(&self, addr: PciAddress);

    /// Enable I/O space access for a device
    fn enable_io_space(&self, addr: PciAddress);
}

/// Common PCI class codes
pub mod pci_class {
    pub const MASS_STORAGE: u8 = 0x01;
    pub const NETWORK: u8 = 0x02;
    pub const DISPLAY: u8 = 0x03;
    pub const MULTIMEDIA: u8 = 0x04;
    pub const BRIDGE: u8 = 0x06;
    pub const SERIAL_BUS: u8 = 0x0C;

    // Mass storage subclasses
    pub const SATA: u8 = 0x06;
    pub const NVME: u8 = 0x08;

    // Network subclasses
    pub const ETHERNET: u8 = 0x00;

    // Serial bus subclasses
    pub const USB: u8 = 0x03;
}
