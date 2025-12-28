//! WATOS Intel e1000 Network Driver
//!
//! Implements the NetworkDevice trait for Intel 82545EM and compatible NICs.
//! Works on QEMU, VMware, VirtualBox, Hyper-V.

#![no_std]

use core::ptr::{read_volatile, write_volatile};
use watos_driver_traits::{Driver, DriverInfo, DriverState, DriverError};
use watos_driver_traits::net::{NetworkDevice, MacAddress, LinkStatus, LinkSpeed, NetCapabilities};
use watos_driver_pci::{PciDriver, PciBar};

// e1000 Register offsets
const REG_CTRL: u32 = 0x0000;
const REG_STATUS: u32 = 0x0008;
const REG_ICR: u32 = 0x00C0;
const REG_IMS: u32 = 0x00D0;
const REG_IMC: u32 = 0x00D8;
const REG_RCTL: u32 = 0x0100;
const REG_TCTL: u32 = 0x0400;
const REG_RDBAL: u32 = 0x2800;
const REG_RDBAH: u32 = 0x2804;
const REG_RDLEN: u32 = 0x2808;
const REG_RDH: u32 = 0x2810;
const REG_RDT: u32 = 0x2818;
const REG_TDBAL: u32 = 0x3800;
const REG_TDBAH: u32 = 0x3804;
const REG_TDLEN: u32 = 0x3808;
const REG_TDH: u32 = 0x3810;
const REG_TDT: u32 = 0x3818;
const REG_RAL0: u32 = 0x5400;
const REG_RAH0: u32 = 0x5404;

// Control bits
const CTRL_SLU: u32 = 1 << 6;
const CTRL_RST: u32 = 1 << 26;

// Receive control bits
const RCTL_EN: u32 = 1 << 1;
const RCTL_BAM: u32 = 1 << 15;
const RCTL_SECRC: u32 = 1 << 26;

// Transmit control bits
const TCTL_EN: u32 = 1 << 1;
const TCTL_PSP: u32 = 1 << 3;

// Descriptor bits
const DESC_DD: u8 = 1 << 0;
const DESC_EOP: u8 = 1 << 1;
const TDESC_CMD_EOP: u8 = 1 << 0;
const TDESC_CMD_IFCS: u8 = 1 << 1;
const TDESC_CMD_RS: u8 = 1 << 3;

// Ring sizes
const NUM_RX_DESC: usize = 32;
const NUM_TX_DESC: usize = 32;
const BUFFER_SIZE: usize = 2048;

/// RX Descriptor
#[repr(C, align(16))]
#[derive(Clone, Copy, Default)]
struct RxDesc {
    addr: u64,
    length: u16,
    checksum: u16,
    status: u8,
    errors: u8,
    special: u16,
}

/// TX Descriptor
#[repr(C, align(16))]
#[derive(Clone, Copy, Default)]
struct TxDesc {
    addr: u64,
    length: u16,
    cso: u8,
    cmd: u8,
    status: u8,
    css: u8,
    special: u16,
}

/// Intel e1000 Network Driver
pub struct E1000Driver {
    state: DriverState,
    mmio_base: u64,
    mac_addr: [u8; 6],
    rx_cur: usize,
    tx_cur: usize,
}

// Fixed memory for descriptor rings
const DESC_RING_BASE: u64 = 0x500000; // 5MB
const RX_DESC_BASE: u64 = DESC_RING_BASE;
const TX_DESC_BASE: u64 = DESC_RING_BASE + 0x1000;
const RX_BUFFER_BASE: u64 = DESC_RING_BASE + 0x2000;
const TX_BUFFER_BASE: u64 = RX_BUFFER_BASE + (NUM_RX_DESC * BUFFER_SIZE) as u64;

impl E1000Driver {
    /// Intel vendor ID
    const VENDOR_INTEL: u16 = 0x8086;
    /// e1000 device IDs
    const DEVICE_82545EM: u16 = 0x100F;
    const DEVICE_82540EM: u16 = 0x100E;

    /// Probe for e1000 NIC
    pub fn probe() -> Option<Self> {
        let mut pci = PciDriver::new();
        pci.init().ok()?;
        Self::probe_with_pci(&pci)
    }

    /// Probe for e1000 NIC with provided PCI driver
    pub fn probe_with_pci(pci: &PciDriver) -> Option<Self> {
        // Look for Intel e1000 devices
        for dev_id in [Self::DEVICE_82545EM, Self::DEVICE_82540EM] {
            if let Some(dev) = pci.find_by_id(Self::VENDOR_INTEL, dev_id) {
                // Get BAR0 (MMIO)
                let mmio_base = match dev.bars[0] {
                    PciBar::Memory { address, .. } if address != 0 => address,
                    _ => continue,
                };

                // Enable bus master and memory
                pci.enable_bus_master(dev.address);
                pci.enable_memory_space(dev.address);

                return Some(Self {
                    state: DriverState::Loaded,
                    mmio_base,
                    mac_addr: [0; 6],
                    rx_cur: 0,
                    tx_cur: 0,
                });
            }
        }
        None
    }

    fn read_reg(&self, reg: u32) -> u32 {
        unsafe { read_volatile((self.mmio_base + reg as u64) as *const u32) }
    }

    fn write_reg(&self, reg: u32, value: u32) {
        unsafe { write_volatile((self.mmio_base + reg as u64) as *mut u32, value) }
    }

    fn reset(&mut self) {
        self.write_reg(REG_IMC, 0xFFFFFFFF);
        self.write_reg(REG_CTRL, CTRL_RST);

        for _ in 0..10000 {
            if self.read_reg(REG_CTRL) & CTRL_RST == 0 {
                break;
            }
        }

        self.write_reg(REG_IMC, 0xFFFFFFFF);
        let _ = self.read_reg(REG_ICR);
    }

    fn read_mac(&mut self) {
        let ral = self.read_reg(REG_RAL0);
        let rah = self.read_reg(REG_RAH0);

        if ral != 0 && ral != 0xFFFFFFFF {
            self.mac_addr[0] = ral as u8;
            self.mac_addr[1] = (ral >> 8) as u8;
            self.mac_addr[2] = (ral >> 16) as u8;
            self.mac_addr[3] = (ral >> 24) as u8;
            self.mac_addr[4] = rah as u8;
            self.mac_addr[5] = (rah >> 8) as u8;
        } else {
            // Default QEMU MAC
            self.mac_addr = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
            self.program_mac();
        }
    }

    fn program_mac(&self) {
        let ral = (self.mac_addr[0] as u32)
            | ((self.mac_addr[1] as u32) << 8)
            | ((self.mac_addr[2] as u32) << 16)
            | ((self.mac_addr[3] as u32) << 24);
        let rah = (self.mac_addr[4] as u32)
            | ((self.mac_addr[5] as u32) << 8)
            | (1 << 31); // Address Valid

        self.write_reg(REG_RAL0, ral);
        self.write_reg(REG_RAH0, rah);
    }

    fn init_rx(&mut self) {
        // Clear descriptor memory
        unsafe {
            core::ptr::write_bytes(RX_DESC_BASE as *mut u8, 0, NUM_RX_DESC * 16);
            core::ptr::write_bytes(RX_BUFFER_BASE as *mut u8, 0, NUM_RX_DESC * BUFFER_SIZE);
        }

        // Setup descriptors
        let descs = RX_DESC_BASE as *mut RxDesc;
        for i in 0..NUM_RX_DESC {
            unsafe {
                (*descs.add(i)).addr = RX_BUFFER_BASE + (i * BUFFER_SIZE) as u64;
            }
        }

        // Program descriptor ring
        self.write_reg(REG_RDBAL, RX_DESC_BASE as u32);
        self.write_reg(REG_RDBAH, (RX_DESC_BASE >> 32) as u32);
        self.write_reg(REG_RDLEN, (NUM_RX_DESC * 16) as u32);
        self.write_reg(REG_RDH, 0);
        self.write_reg(REG_RDT, (NUM_RX_DESC - 1) as u32);

        // Enable receiver
        self.write_reg(REG_RCTL, RCTL_EN | RCTL_BAM | RCTL_SECRC);

        self.rx_cur = 0;
    }

    fn init_tx(&mut self) {
        // Clear descriptor memory
        unsafe {
            core::ptr::write_bytes(TX_DESC_BASE as *mut u8, 0, NUM_TX_DESC * 16);
            core::ptr::write_bytes(TX_BUFFER_BASE as *mut u8, 0, NUM_TX_DESC * BUFFER_SIZE);
        }

        // Setup descriptors
        let descs = TX_DESC_BASE as *mut TxDesc;
        for i in 0..NUM_TX_DESC {
            unsafe {
                (*descs.add(i)).addr = TX_BUFFER_BASE + (i * BUFFER_SIZE) as u64;
                (*descs.add(i)).status = DESC_DD; // Mark as done
            }
        }

        // Program descriptor ring
        self.write_reg(REG_TDBAL, TX_DESC_BASE as u32);
        self.write_reg(REG_TDBAH, (TX_DESC_BASE >> 32) as u32);
        self.write_reg(REG_TDLEN, (NUM_TX_DESC * 16) as u32);
        self.write_reg(REG_TDH, 0);
        self.write_reg(REG_TDT, 0);

        // Enable transmitter
        self.write_reg(REG_TCTL, TCTL_EN | TCTL_PSP | (0x10 << 4) | (0x40 << 12));

        self.tx_cur = 0;
    }

    fn link_up(&self) {
        let ctrl = self.read_reg(REG_CTRL);
        self.write_reg(REG_CTRL, ctrl | CTRL_SLU);
    }
}

impl Driver for E1000Driver {
    fn info(&self) -> DriverInfo {
        DriverInfo {
            name: "e1000",
            version: "0.1.0",
            author: "WATOS",
            description: "Intel e1000 network driver",
        }
    }

    fn state(&self) -> DriverState {
        self.state
    }

    fn init(&mut self) -> Result<(), DriverError> {
        if self.state != DriverState::Loaded {
            return Err(DriverError::InvalidState);
        }

        self.reset();
        self.read_mac();
        self.init_rx();
        self.init_tx();
        self.link_up();

        self.state = DriverState::Ready;
        Ok(())
    }

    fn start(&mut self) -> Result<(), DriverError> {
        if self.state != DriverState::Ready {
            return Err(DriverError::InvalidState);
        }
        self.state = DriverState::Active;
        Ok(())
    }

    fn stop(&mut self) -> Result<(), DriverError> {
        if self.state != DriverState::Active {
            return Err(DriverError::InvalidState);
        }

        // Disable RX/TX
        self.write_reg(REG_RCTL, 0);
        self.write_reg(REG_TCTL, 0);

        self.state = DriverState::Ready;
        Ok(())
    }
}

impl NetworkDevice for E1000Driver {
    fn mac_address(&self) -> MacAddress {
        MacAddress(self.mac_addr)
    }

    fn link_status(&self) -> LinkStatus {
        let status = self.read_reg(REG_STATUS);
        if status & (1 << 1) != 0 { // Link up bit
            LinkStatus::Up
        } else {
            LinkStatus::Down
        }
    }

    fn link_speed(&self) -> LinkSpeed {
        let status = self.read_reg(REG_STATUS);
        match (status >> 6) & 0x3 {
            0b00 => LinkSpeed::Mbps10,
            0b01 => LinkSpeed::Mbps100,
            0b10 | 0b11 => LinkSpeed::Gbps1,
            _ => LinkSpeed::Unknown,
        }
    }

    fn capabilities(&self) -> NetCapabilities {
        NetCapabilities {
            mtu: 1500,
            checksum_offload: true,
            scatter_gather: false,
            promiscuous: true,
        }
    }

    fn has_packet(&self) -> bool {
        let descs = RX_DESC_BASE as *const RxDesc;
        let desc = unsafe { &*descs.add(self.rx_cur) };
        desc.status & DESC_DD != 0
    }

    fn set_promiscuous(&mut self, enabled: bool) -> Result<(), DriverError> {
        let rctl = self.read_reg(REG_RCTL);
        if enabled {
            self.write_reg(REG_RCTL, rctl | (1 << 3) | (1 << 4)); // UPE + MPE
        } else {
            self.write_reg(REG_RCTL, rctl & !((1 << 3) | (1 << 4)));
        }
        Ok(())
    }

    fn send(&mut self, packet: &[u8]) -> Result<(), DriverError> {
        if self.state != DriverState::Active {
            return Err(DriverError::InvalidState);
        }

        if packet.len() > BUFFER_SIZE {
            return Err(DriverError::ResourceError);
        }

        let descs = TX_DESC_BASE as *mut TxDesc;
        let idx = self.tx_cur;

        // Wait for descriptor to be available
        for _ in 0..1000000 {
            if unsafe { (*descs.add(idx)).status & DESC_DD != 0 } {
                break;
            }
        }

        // Copy packet to buffer
        let buf_addr = TX_BUFFER_BASE + (idx * BUFFER_SIZE) as u64;
        unsafe {
            core::ptr::copy_nonoverlapping(packet.as_ptr(), buf_addr as *mut u8, packet.len());

            // Setup descriptor
            let desc = &mut *descs.add(idx);
            desc.length = packet.len() as u16;
            desc.cmd = TDESC_CMD_EOP | TDESC_CMD_IFCS | TDESC_CMD_RS;
            desc.status = 0;
        }

        // Advance tail
        self.tx_cur = (self.tx_cur + 1) % NUM_TX_DESC;
        self.write_reg(REG_TDT, self.tx_cur as u32);

        Ok(())
    }

    fn receive(&mut self, buffer: &mut [u8]) -> Result<usize, DriverError> {
        if self.state != DriverState::Active {
            return Err(DriverError::InvalidState);
        }

        let descs = RX_DESC_BASE as *mut RxDesc;
        let idx = self.rx_cur;

        // Check if packet available
        let desc = unsafe { &*descs.add(idx) };
        if desc.status & DESC_DD == 0 {
            return Ok(0); // No packet
        }

        if desc.status & DESC_EOP == 0 {
            // Partial packet, skip
            self.rx_cur = (self.rx_cur + 1) % NUM_RX_DESC;
            self.write_reg(REG_RDT, idx as u32);
            return Ok(0);
        }

        let len = desc.length as usize;
        if len > buffer.len() {
            return Err(DriverError::ResourceError);
        }

        // Copy packet from buffer
        let buf_addr = RX_BUFFER_BASE + (idx * BUFFER_SIZE) as u64;
        unsafe {
            core::ptr::copy_nonoverlapping(buf_addr as *const u8, buffer.as_mut_ptr(), len);

            // Reset descriptor
            let desc = &mut *descs.add(idx);
            desc.status = 0;
        }

        // Advance tail
        self.rx_cur = (self.rx_cur + 1) % NUM_RX_DESC;
        self.write_reg(REG_RDT, idx as u32);

        Ok(len)
    }
}
