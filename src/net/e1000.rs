//! Intel e1000 Network Driver
//!
//! Supports Intel 82545EM and compatible NICs
//! Works on QEMU, VMware, VirtualBox, Hyper-V

use super::pci::{self, PciDevice};
use core::ptr::{read_volatile, write_volatile};

// e1000 Register offsets
const REG_CTRL: u32 = 0x0000;        // Device Control
const REG_STATUS: u32 = 0x0008;      // Device Status
const REG_EECD: u32 = 0x0010;        // EEPROM Control
const REG_EERD: u32 = 0x0014;        // EEPROM Read
const REG_ICR: u32 = 0x00C0;         // Interrupt Cause Read
const REG_IMS: u32 = 0x00D0;         // Interrupt Mask Set
const REG_IMC: u32 = 0x00D8;         // Interrupt Mask Clear
const REG_RCTL: u32 = 0x0100;        // Receive Control
const REG_TCTL: u32 = 0x0400;        // Transmit Control
const REG_RDBAL: u32 = 0x2800;       // RX Descriptor Base Low
const REG_RDBAH: u32 = 0x2804;       // RX Descriptor Base High
const REG_RDLEN: u32 = 0x2808;       // RX Descriptor Length
const REG_RDH: u32 = 0x2810;         // RX Descriptor Head
const REG_RDT: u32 = 0x2818;         // RX Descriptor Tail
const REG_TDBAL: u32 = 0x3800;       // TX Descriptor Base Low
const REG_TDBAH: u32 = 0x3804;       // TX Descriptor Base High
const REG_TDLEN: u32 = 0x3808;       // TX Descriptor Length
const REG_TDH: u32 = 0x3810;         // TX Descriptor Head
const REG_TDT: u32 = 0x3818;         // TX Descriptor Tail
const REG_MTA: u32 = 0x5200;         // Multicast Table Array
const REG_RAL0: u32 = 0x5400;        // Receive Address Low
const REG_RAH0: u32 = 0x5404;        // Receive Address High

// Control Register bits
const CTRL_SLU: u32 = 1 << 6;        // Set Link Up
const CTRL_ASDE: u32 = 1 << 5;       // Auto-Speed Detection Enable
const CTRL_RST: u32 = 1 << 26;       // Device Reset

// Receive Control bits
const RCTL_EN: u32 = 1 << 1;         // Receiver Enable
const RCTL_SBP: u32 = 1 << 2;        // Store Bad Packets
const RCTL_UPE: u32 = 1 << 3;        // Unicast Promiscuous
const RCTL_MPE: u32 = 1 << 4;        // Multicast Promiscuous
const RCTL_LBM_NONE: u32 = 0 << 6;   // No Loopback
const RCTL_RDMTS_HALF: u32 = 0 << 8; // RX Desc Min Threshold
const RCTL_BAM: u32 = 1 << 15;       // Broadcast Accept Mode
const RCTL_BSIZE_2048: u32 = 0 << 16; // Buffer Size 2048
const RCTL_SECRC: u32 = 1 << 26;     // Strip Ethernet CRC

// Transmit Control bits
const TCTL_EN: u32 = 1 << 1;         // Transmitter Enable
const TCTL_PSP: u32 = 1 << 3;        // Pad Short Packets
const TCTL_CT_SHIFT: u32 = 4;        // Collision Threshold
const TCTL_COLD_SHIFT: u32 = 12;     // Collision Distance

// TX Descriptor command bits
const TDESC_CMD_EOP: u8 = 1 << 0;    // End of Packet
const TDESC_CMD_IFCS: u8 = 1 << 1;   // Insert FCS
const TDESC_CMD_RS: u8 = 1 << 3;     // Report Status

// TX Descriptor status bits
const TDESC_STA_DD: u8 = 1 << 0;     // Descriptor Done

// RX Descriptor status bits
const RDESC_STA_DD: u8 = 1 << 0;     // Descriptor Done
const RDESC_STA_EOP: u8 = 1 << 1;    // End of Packet

// Descriptor ring sizes (must be multiple of 8)
const NUM_RX_DESC: usize = 32;
const NUM_TX_DESC: usize = 32;
const BUFFER_SIZE: usize = 2048;

/// RX Descriptor (legacy format)
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct RxDesc {
    pub addr: u64,      // Buffer address
    pub length: u16,    // Packet length
    pub checksum: u16,  // Packet checksum
    pub status: u8,     // Status
    pub errors: u8,     // Errors
    pub special: u16,   // Special (VLAN tag)
}

/// TX Descriptor (legacy format)
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct TxDesc {
    pub addr: u64,      // Buffer address
    pub length: u16,    // Packet length
    pub cso: u8,        // Checksum offset
    pub cmd: u8,        // Command
    pub status: u8,     // Status
    pub css: u8,        // Checksum start
    pub special: u16,   // Special (VLAN tag)
}

impl Default for RxDesc {
    fn default() -> Self {
        Self {
            addr: 0,
            length: 0,
            checksum: 0,
            status: 0,
            errors: 0,
            special: 0,
        }
    }
}

impl Default for TxDesc {
    fn default() -> Self {
        Self {
            addr: 0,
            length: 0,
            cso: 0,
            cmd: 0,
            status: 0,
            css: 0,
            special: 0,
        }
    }
}

/// e1000 Network Interface Controller
pub struct E1000 {
    mmio_base: u64,
    mac_addr: [u8; 6],
    rx_descs: &'static mut [RxDesc; NUM_RX_DESC],
    tx_descs: &'static mut [TxDesc; NUM_TX_DESC],
    rx_buffers: [[u8; BUFFER_SIZE]; NUM_RX_DESC],
    tx_buffers: [[u8; BUFFER_SIZE]; NUM_TX_DESC],
    rx_cur: usize,
    tx_cur: usize,
}

// Static storage for descriptors (must be aligned and in known location)
static mut RX_DESC_RING: [RxDesc; NUM_RX_DESC] = [RxDesc {
    addr: 0, length: 0, checksum: 0, status: 0, errors: 0, special: 0
}; NUM_RX_DESC];

static mut TX_DESC_RING: [TxDesc; NUM_TX_DESC] = [TxDesc {
    addr: 0, length: 0, cso: 0, cmd: 0, status: 0, css: 0, special: 0
}; NUM_TX_DESC];

static mut RX_BUFFERS: [[u8; BUFFER_SIZE]; NUM_RX_DESC] = [[0u8; BUFFER_SIZE]; NUM_RX_DESC];
static mut TX_BUFFERS: [[u8; BUFFER_SIZE]; NUM_TX_DESC] = [[0u8; BUFFER_SIZE]; NUM_TX_DESC];

impl E1000 {
    /// Detect and initialize e1000 NIC
    pub fn new() -> Option<Self> {
        // Find the device on PCI bus
        let pci_dev = pci::find_e1000()?;

        // Enable PCI bus mastering and memory access
        pci_dev.enable();

        // Get MMIO base address
        let mmio_base = pci_dev.mmio_base();
        if mmio_base == 0 {
            return None;
        }

        // Create the driver instance
        let mut nic = unsafe {
            E1000 {
                mmio_base,
                mac_addr: [0; 6],
                rx_descs: &mut RX_DESC_RING,
                tx_descs: &mut TX_DESC_RING,
                rx_buffers: [[0; BUFFER_SIZE]; NUM_RX_DESC],
                tx_buffers: [[0; BUFFER_SIZE]; NUM_TX_DESC],
                rx_cur: 0,
                tx_cur: 0,
            }
        };

        // Initialize the hardware
        nic.reset();
        nic.read_mac_address();
        nic.init_rx();
        nic.init_tx();
        nic.enable_interrupts();
        nic.link_up();

        Some(nic)
    }

    /// Read a register
    fn read_reg(&self, reg: u32) -> u32 {
        unsafe {
            let ptr = (self.mmio_base + reg as u64) as *const u32;
            read_volatile(ptr)
        }
    }

    /// Write a register
    fn write_reg(&self, reg: u32, value: u32) {
        unsafe {
            let ptr = (self.mmio_base + reg as u64) as *mut u32;
            write_volatile(ptr, value);
        }
    }

    /// Reset the device
    fn reset(&mut self) {
        // Disable interrupts
        self.write_reg(REG_IMC, 0xFFFFFFFF);

        // Reset device
        self.write_reg(REG_CTRL, CTRL_RST);

        // Wait for reset to complete
        for _ in 0..10000 {
            if self.read_reg(REG_CTRL) & CTRL_RST == 0 {
                break;
            }
        }

        // Disable interrupts again after reset
        self.write_reg(REG_IMC, 0xFFFFFFFF);

        // Clear pending interrupts
        let _ = self.read_reg(REG_ICR);
    }

    /// Read MAC address from EEPROM
    fn read_mac_address(&mut self) {
        // Try reading from RAL0/RAH0 first (may be pre-programmed)
        let ral = self.read_reg(REG_RAL0);
        let rah = self.read_reg(REG_RAH0);

        if ral != 0 && ral != 0xFFFFFFFF {
            self.mac_addr[0] = (ral >> 0) as u8;
            self.mac_addr[1] = (ral >> 8) as u8;
            self.mac_addr[2] = (ral >> 16) as u8;
            self.mac_addr[3] = (ral >> 24) as u8;
            self.mac_addr[4] = (rah >> 0) as u8;
            self.mac_addr[5] = (rah >> 8) as u8;
        } else {
            // Use a default MAC address (52:54:00:xx:xx:xx is QEMU's range)
            self.mac_addr = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];

            // Program it into the hardware
            let ral = (self.mac_addr[0] as u32)
                | ((self.mac_addr[1] as u32) << 8)
                | ((self.mac_addr[2] as u32) << 16)
                | ((self.mac_addr[3] as u32) << 24);
            let rah = (self.mac_addr[4] as u32)
                | ((self.mac_addr[5] as u32) << 8)
                | (1 << 31); // Address Valid bit

            self.write_reg(REG_RAL0, ral);
            self.write_reg(REG_RAH0, rah);
        }

        // Clear multicast table
        for i in 0..128 {
            self.write_reg(REG_MTA + (i * 4), 0);
        }
    }

    /// Initialize RX descriptor ring
    fn init_rx(&mut self) {
        // Set up RX buffers
        for i in 0..NUM_RX_DESC {
            unsafe {
                self.rx_descs[i].addr = RX_BUFFERS[i].as_ptr() as u64;
                self.rx_descs[i].status = 0;
            }
        }

        // Program RX descriptor ring address
        let rx_ring_addr = self.rx_descs.as_ptr() as u64;
        self.write_reg(REG_RDBAL, rx_ring_addr as u32);
        self.write_reg(REG_RDBAH, (rx_ring_addr >> 32) as u32);

        // Program RX descriptor ring length
        self.write_reg(REG_RDLEN, (NUM_RX_DESC * core::mem::size_of::<RxDesc>()) as u32);

        // Set head and tail
        self.write_reg(REG_RDH, 0);
        self.write_reg(REG_RDT, (NUM_RX_DESC - 1) as u32);

        // Enable receiver
        self.write_reg(REG_RCTL,
            RCTL_EN |           // Enable
            RCTL_BAM |          // Accept broadcast
            RCTL_BSIZE_2048 |   // 2KB buffers
            RCTL_SECRC          // Strip CRC
        );
    }

    /// Initialize TX descriptor ring
    fn init_tx(&mut self) {
        // Set up TX descriptors
        for i in 0..NUM_TX_DESC {
            unsafe {
                self.tx_descs[i].addr = TX_BUFFERS[i].as_ptr() as u64;
                self.tx_descs[i].status = TDESC_STA_DD; // Mark as done
                self.tx_descs[i].cmd = 0;
            }
        }

        // Program TX descriptor ring address
        let tx_ring_addr = self.tx_descs.as_ptr() as u64;
        self.write_reg(REG_TDBAL, tx_ring_addr as u32);
        self.write_reg(REG_TDBAH, (tx_ring_addr >> 32) as u32);

        // Program TX descriptor ring length
        self.write_reg(REG_TDLEN, (NUM_TX_DESC * core::mem::size_of::<TxDesc>()) as u32);

        // Set head and tail
        self.write_reg(REG_TDH, 0);
        self.write_reg(REG_TDT, 0);

        // Enable transmitter
        self.write_reg(REG_TCTL,
            TCTL_EN |                       // Enable
            TCTL_PSP |                      // Pad short packets
            (15 << TCTL_CT_SHIFT) |         // Collision threshold
            (64 << TCTL_COLD_SHIFT)         // Collision distance
        );
    }

    /// Enable interrupts (for future interrupt-driven mode)
    fn enable_interrupts(&self) {
        // For now, we'll use polling, but set up interrupts for later
        // self.write_reg(REG_IMS, 0x1F6DC); // Enable common interrupts
    }

    /// Bring the link up
    fn link_up(&self) {
        let ctrl = self.read_reg(REG_CTRL);
        self.write_reg(REG_CTRL, ctrl | CTRL_SLU | CTRL_ASDE);
    }

    /// Get MAC address
    pub fn mac_address(&self) -> [u8; 6] {
        self.mac_addr
    }

    /// Check if link is up
    pub fn link_status(&self) -> bool {
        self.read_reg(REG_STATUS) & 2 != 0
    }

    /// Send a packet
    pub fn send(&mut self, data: &[u8]) -> bool {
        if data.len() > BUFFER_SIZE {
            return false;
        }

        let idx = self.tx_cur;

        // Wait for descriptor to be available
        unsafe {
            if self.tx_descs[idx].status & TDESC_STA_DD == 0 {
                // Still in use
                return false;
            }

            // Copy data to TX buffer
            TX_BUFFERS[idx][..data.len()].copy_from_slice(data);

            // Set up descriptor
            self.tx_descs[idx].addr = TX_BUFFERS[idx].as_ptr() as u64;
            self.tx_descs[idx].length = data.len() as u16;
            self.tx_descs[idx].cmd = TDESC_CMD_EOP | TDESC_CMD_IFCS | TDESC_CMD_RS;
            self.tx_descs[idx].status = 0;
        }

        // Update tail
        self.tx_cur = (idx + 1) % NUM_TX_DESC;
        self.write_reg(REG_TDT, self.tx_cur as u32);

        true
    }

    /// Receive a packet (polling mode)
    /// Returns the number of bytes received, or 0 if no packet available
    pub fn recv(&mut self, buffer: &mut [u8]) -> usize {
        let idx = self.rx_cur;

        unsafe {
            // Check if packet is available
            if self.rx_descs[idx].status & RDESC_STA_DD == 0 {
                return 0;
            }

            let length = self.rx_descs[idx].length as usize;

            // Copy data from RX buffer
            let copy_len = core::cmp::min(length, buffer.len());
            buffer[..copy_len].copy_from_slice(&RX_BUFFERS[idx][..copy_len]);

            // Reset descriptor for reuse
            self.rx_descs[idx].status = 0;

            // Update tail
            let old_cur = self.rx_cur;
            self.rx_cur = (idx + 1) % NUM_RX_DESC;
            self.write_reg(REG_RDT, old_cur as u32);

            copy_len
        }
    }

    /// Check if there's a packet waiting
    pub fn has_packet(&self) -> bool {
        unsafe {
            self.rx_descs[self.rx_cur].status & RDESC_STA_DD != 0
        }
    }

    /// Get MMIO base address (for debugging)
    pub fn get_mmio_base(&self) -> u64 {
        self.mmio_base
    }

    /// Read device status register (for debugging)
    pub fn get_status(&self) -> u32 {
        self.read_reg(REG_STATUS)
    }

    /// Read TX descriptor head (for debugging)
    pub fn get_tdh(&self) -> u32 {
        self.read_reg(REG_TDH)
    }

    /// Read TX descriptor tail (for debugging)
    pub fn get_tdt(&self) -> u32 {
        self.read_reg(REG_TDT)
    }
}
