//! Bus interface traits and types
//!
//! Provides abstractions for PCI and other bus types.

use alloc::vec::Vec;

/// PCI device address (bus:device.function)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PciAddress {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
}

impl PciAddress {
    pub const fn new(bus: u8, device: u8, function: u8) -> Self {
        PciAddress { bus, device, function }
    }
}

/// PCI device ID (vendor:device)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PciDeviceId {
    pub vendor: u16,
    pub device: u16,
    pub class: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub revision: u8,
}

/// PCI Base Address Register
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PciBar {
    /// Memory-mapped I/O
    Memory {
        address: u64,
        size: u64,
        prefetchable: bool,
        is_64bit: bool,
    },
    /// I/O port
    Io {
        port: u32,
        size: u32,
    },
    /// Not present
    None,
}

impl Default for PciBar {
    fn default() -> Self {
        PciBar::None
    }
}

/// Complete PCI device information
#[derive(Debug, Clone, Default)]
pub struct PciDeviceInfo {
    pub address: PciAddress,
    pub id: PciDeviceId,
    pub bars: [PciBar; 6],
    pub interrupt_line: u8,
    pub interrupt_pin: u8,
}

/// PCI bus operations trait
pub trait PciBus {
    /// Enumerate all PCI devices
    fn enumerate(&mut self) -> Vec<PciDeviceInfo>;

    /// Find devices by class
    fn find_by_class(&self, class: u8, subclass: u8) -> Vec<PciDeviceInfo>;

    /// Find device by vendor and device ID
    fn find_by_id(&self, vendor: u16, device: u16) -> Option<PciDeviceInfo>;

    /// Enable bus mastering for a device
    fn enable_bus_master(&self, addr: PciAddress);

    /// Enable memory space access for a device
    fn enable_memory_space(&self, addr: PciAddress);

    /// Enable I/O space access for a device
    fn enable_io_space(&self, addr: PciAddress);

    /// Read from PCI configuration space
    fn config_read(&self, addr: PciAddress, offset: u8) -> u32;

    /// Write to PCI configuration space
    fn config_write(&self, addr: PciAddress, offset: u8, value: u32);
}

/// PCI class codes
pub mod pci_class {
    /// Mass storage controller
    pub const MASS_STORAGE: u8 = 0x01;
    /// SATA controller (subclass of MASS_STORAGE)
    pub const SATA: u8 = 0x06;
    /// IDE controller
    pub const IDE: u8 = 0x01;
    /// Network controller
    pub const NETWORK: u8 = 0x02;
    /// Ethernet controller (subclass of NETWORK)
    pub const ETHERNET: u8 = 0x00;
    /// Display controller
    pub const DISPLAY: u8 = 0x03;
    /// VGA compatible
    pub const VGA: u8 = 0x00;
    /// Multimedia controller
    pub const MULTIMEDIA: u8 = 0x04;
    /// Audio device
    pub const AUDIO: u8 = 0x01;
    /// Bridge device
    pub const BRIDGE: u8 = 0x06;
    /// Serial bus controller
    pub const SERIAL_BUS: u8 = 0x0C;
    /// USB controller
    pub const USB: u8 = 0x03;
}
