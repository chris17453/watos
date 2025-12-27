//! AHCI (SATA) Driver
//!
//! Minimal driver for reading/writing sectors from SATA drives

use crate::net::pci;
use core::ptr::{read_volatile, write_volatile};

// AHCI HBA Memory Registers
const HBA_CAP: usize = 0x00;        // Host Capabilities
const HBA_GHC: usize = 0x04;        // Global Host Control
const HBA_IS: usize = 0x08;         // Interrupt Status
const HBA_PI: usize = 0x0C;         // Ports Implemented
const HBA_VS: usize = 0x10;         // Version

// Port registers (offset from port base)
const PORT_CLB: usize = 0x00;       // Command List Base
const PORT_FB: usize = 0x08;        // FIS Base
const PORT_IS: usize = 0x10;        // Interrupt Status
const PORT_IE: usize = 0x14;        // Interrupt Enable
const PORT_CMD: usize = 0x18;       // Command
const PORT_TFD: usize = 0x20;       // Task File Data
const PORT_SIG: usize = 0x24;       // Signature
const PORT_SSTS: usize = 0x28;      // SATA Status
const PORT_SCTL: usize = 0x2C;      // SATA Control
const PORT_SERR: usize = 0x30;      // SATA Error
const PORT_SACT: usize = 0x34;      // SATA Active
const PORT_CI: usize = 0x38;        // Command Issue

// Port CMD bits
const PORT_CMD_ST: u32 = 1 << 0;    // Start
const PORT_CMD_FRE: u32 = 1 << 4;   // FIS Receive Enable
const PORT_CMD_FR: u32 = 1 << 14;   // FIS Receive Running
const PORT_CMD_CR: u32 = 1 << 15;   // Command List Running

// FIS Types
const FIS_TYPE_REG_H2D: u8 = 0x27;  // Host to Device

// ATA Commands
const ATA_CMD_READ_DMA_EXT: u8 = 0x25;
const ATA_CMD_WRITE_DMA_EXT: u8 = 0x35;
const ATA_CMD_IDENTIFY: u8 = 0xEC;

// Command header
#[repr(C, packed)]
struct CommandHeader {
    flags: u16,         // CFL, A, W, P, R, B, C, PMPort
    prdtl: u16,         // PRDT Length
    prdbc: u32,         // PRD Byte Count
    ctba: u64,          // Command Table Base Address
    reserved: [u32; 4],
}

// Command table
#[repr(C, packed)]
struct CommandTable {
    cfis: [u8; 64],     // Command FIS
    acmd: [u8; 16],     // ATAPI Command
    reserved: [u8; 48],
    prdt: [PrdtEntry; 8], // PRD Table entries
}

// PRD Table entry
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct PrdtEntry {
    dba: u64,           // Data Base Address
    reserved: u32,
    dbc: u32,           // Byte Count (bit 31 = interrupt on completion)
}

// FIS Register H2D
#[repr(C, packed)]
struct FisRegH2D {
    fis_type: u8,
    flags: u8,          // PM Port, C bit
    command: u8,
    feature_low: u8,
    lba0: u8,
    lba1: u8,
    lba2: u8,
    device: u8,
    lba3: u8,
    lba4: u8,
    lba5: u8,
    feature_high: u8,
    count_low: u8,
    count_high: u8,
    icc: u8,
    control: u8,
    reserved: [u8; 4],
}

pub struct AhciController {
    mmio_base: u64,
    port: u8,
    // Memory for command structures (allocated in low memory)
    cmd_list: u64,
    cmd_table: u64,
    fis_base: u64,
    // LBA offset for partition support (0 for raw disk)
    lba_offset: u64,
}

impl AhciController {
    /// Find and initialize AHCI controller (first available port)
    pub fn new() -> Option<Self> {
        Self::new_port(0xFF) // 0xFF = find first available
    }

    /// Initialize AHCI controller for a specific port
    /// port = 0xFF means find first available port
    pub fn new_port(target_port: u8) -> Option<Self> {
        let pci_dev = pci::find_ahci()?;
        pci_dev.enable();

        let mmio_base = pci_dev.ahci_base();  // AHCI uses BAR5 (ABAR)
        if mmio_base == 0 {
            return None;
        }

        // Find implemented port with a drive
        let pi = unsafe { read_volatile((mmio_base + HBA_PI as u64) as *const u32) };

        let mut active_port = None;
        for port in 0..32u8 {
            if pi & (1 << port) != 0 {
                let port_base = mmio_base + 0x100 + (port as u64 * 0x80);
                let ssts = unsafe { read_volatile((port_base + PORT_SSTS as u64) as *const u32) };

                // Check device detection (bits 0-3) and interface power (bits 8-11)
                let det = ssts & 0xF;
                let ipm = (ssts >> 8) & 0xF;

                if det == 3 && ipm == 1 {
                    // Device present and active
                    if target_port == 0xFF || target_port == port {
                        active_port = Some(port);
                        break;
                    }
                }
            }
        }

        let port = active_port?;

        // Allocate memory for command structures
        // Use fixed addresses in low memory (after heap)
        let cmd_list = 0x400000u64;   // 4MB - Command List (1KB aligned)
        let fis_base = 0x401000u64;   // 4MB + 4KB - FIS (256 byte aligned)
        let cmd_table = 0x402000u64;  // 4MB + 8KB - Command Table (128 byte aligned)

        let mut ctrl = Self {
            mmio_base,
            port,
            cmd_list,
            cmd_table,
            fis_base,
            lba_offset: 0,
        };

        ctrl.init_port();
        Some(ctrl)
    }

    /// Initialize AHCI controller for a specific port with LBA offset (for partitions)
    pub fn new_port_at(target_port: u8, lba_offset: u64) -> Option<Self> {
        let mut ctrl = Self::new_port(target_port)?;
        ctrl.lba_offset = lba_offset;
        Some(ctrl)
    }

    /// Get the port number this controller is using
    pub fn port(&self) -> u8 {
        self.port
    }

    fn port_base(&self) -> u64 {
        self.mmio_base + 0x100 + (self.port as u64 * 0x80)
    }

    fn read_port(&self, offset: usize) -> u32 {
        unsafe { read_volatile((self.port_base() + offset as u64) as *const u32) }
    }

    fn write_port(&self, offset: usize, value: u32) {
        unsafe { write_volatile((self.port_base() + offset as u64) as *mut u32, value) }
    }

    fn init_port(&mut self) {
        // Stop command engine
        let cmd = self.read_port(PORT_CMD);
        self.write_port(PORT_CMD, cmd & !(PORT_CMD_ST | PORT_CMD_FRE));

        // Wait for engine to stop
        for _ in 0..1000000 {
            let cmd = self.read_port(PORT_CMD);
            if (cmd & (PORT_CMD_CR | PORT_CMD_FR)) == 0 {
                break;
            }
        }

        // Clear memory
        unsafe {
            core::ptr::write_bytes(self.cmd_list as *mut u8, 0, 1024);
            core::ptr::write_bytes(self.fis_base as *mut u8, 0, 256);
            core::ptr::write_bytes(self.cmd_table as *mut u8, 0, 4096);
        }

        // Set command list and FIS base addresses
        self.write_port(PORT_CLB, self.cmd_list as u32);
        unsafe {
            write_volatile((self.port_base() + PORT_CLB as u64 + 4) as *mut u32,
                          (self.cmd_list >> 32) as u32);
        }

        self.write_port(PORT_FB, self.fis_base as u32);
        unsafe {
            write_volatile((self.port_base() + PORT_FB as u64 + 4) as *mut u32,
                          (self.fis_base >> 32) as u32);
        }

        // Clear interrupt status and error
        self.write_port(PORT_IS, 0xFFFFFFFF);
        self.write_port(PORT_SERR, 0xFFFFFFFF);

        // Setup command header 0 to point to command table
        let cmd_header = self.cmd_list as *mut CommandHeader;
        unsafe {
            (*cmd_header).ctba = self.cmd_table;
            (*cmd_header).prdtl = 1;
        }

        // Start command engine
        let cmd = self.read_port(PORT_CMD);
        self.write_port(PORT_CMD, cmd | PORT_CMD_FRE | PORT_CMD_ST);
    }

    /// Read sectors from disk
    /// lba: starting sector (512 bytes each), relative to partition start
    /// count: number of sectors to read
    /// buffer: destination buffer (must be count * 512 bytes)
    pub fn read_sectors(&mut self, lba: u64, count: u16, buffer: &mut [u8]) -> bool {
        if buffer.len() < (count as usize * 512) {
            return false;
        }

        let absolute_lba = lba + self.lba_offset;
        self.issue_command(ATA_CMD_READ_DMA_EXT, absolute_lba, count, buffer.as_ptr() as u64, false)
    }

    /// Write sectors to disk
    pub fn write_sectors(&mut self, lba: u64, count: u16, buffer: &[u8]) -> bool {
        if buffer.len() < (count as usize * 512) {
            return false;
        }

        let absolute_lba = lba + self.lba_offset;
        self.issue_command(ATA_CMD_WRITE_DMA_EXT, absolute_lba, count, buffer.as_ptr() as u64, true)
    }

    fn issue_command(&mut self, cmd: u8, lba: u64, count: u16, buffer_addr: u64, write: bool) -> bool {
        // Setup command header
        let cmd_header = self.cmd_list as *mut CommandHeader;
        unsafe {
            let flags: u16 = (core::mem::size_of::<FisRegH2D>() / 4) as u16; // CFL in dwords
            let flags = if write { flags | (1 << 6) } else { flags }; // W bit
            (*cmd_header).flags = flags;
            (*cmd_header).prdtl = 1;
            (*cmd_header).prdbc = 0;
        }

        // Setup command table with FIS
        let cmd_table = self.cmd_table as *mut CommandTable;
        unsafe {
            // Clear command FIS area
            core::ptr::write_bytes((*cmd_table).cfis.as_mut_ptr(), 0, 64);

            // Build FIS
            let fis = (*cmd_table).cfis.as_mut_ptr() as *mut FisRegH2D;
            (*fis).fis_type = FIS_TYPE_REG_H2D;
            (*fis).flags = 0x80; // Command bit set
            (*fis).command = cmd;
            (*fis).device = 0x40; // LBA mode

            (*fis).lba0 = lba as u8;
            (*fis).lba1 = (lba >> 8) as u8;
            (*fis).lba2 = (lba >> 16) as u8;
            (*fis).lba3 = (lba >> 24) as u8;
            (*fis).lba4 = (lba >> 32) as u8;
            (*fis).lba5 = (lba >> 40) as u8;

            (*fis).count_low = count as u8;
            (*fis).count_high = (count >> 8) as u8;

            // Setup PRDT entry
            (*cmd_table).prdt[0].dba = buffer_addr;
            (*cmd_table).prdt[0].dbc = ((count as u32 * 512) - 1) | (1 << 31); // Byte count, interrupt
        }

        // Clear interrupt status
        self.write_port(PORT_IS, 0xFFFFFFFF);

        // Issue command (slot 0)
        self.write_port(PORT_CI, 1);

        // Wait for completion
        for _ in 0..10000000 {
            let ci = self.read_port(PORT_CI);
            if ci & 1 == 0 {
                // Check for errors
                let is = self.read_port(PORT_IS);
                let tfd = self.read_port(PORT_TFD);

                if (tfd & 0x01) != 0 || (is & (1 << 30)) != 0 {
                    // Error
                    return false;
                }
                return true;
            }

            // Check for error
            let tfd = self.read_port(PORT_TFD);
            if (tfd & 0x01) != 0 {
                return false;
            }
        }

        false // Timeout
    }

    /// Get disk size in sectors
    pub fn identify(&mut self) -> Option<DiskInfo> {
        let mut buffer = [0u8; 512];

        // Setup identify command
        let cmd_header = self.cmd_list as *mut CommandHeader;
        unsafe {
            (*cmd_header).flags = (core::mem::size_of::<FisRegH2D>() / 4) as u16;
            (*cmd_header).prdtl = 1;
            (*cmd_header).prdbc = 0;
        }

        let cmd_table = self.cmd_table as *mut CommandTable;
        unsafe {
            core::ptr::write_bytes((*cmd_table).cfis.as_mut_ptr(), 0, 64);

            let fis = (*cmd_table).cfis.as_mut_ptr() as *mut FisRegH2D;
            (*fis).fis_type = FIS_TYPE_REG_H2D;
            (*fis).flags = 0x80;
            (*fis).command = ATA_CMD_IDENTIFY;
            (*fis).device = 0;

            (*cmd_table).prdt[0].dba = buffer.as_ptr() as u64;
            (*cmd_table).prdt[0].dbc = 511 | (1 << 31);
        }

        self.write_port(PORT_IS, 0xFFFFFFFF);
        self.write_port(PORT_CI, 1);

        // Wait
        for _ in 0..10000000 {
            if self.read_port(PORT_CI) & 1 == 0 {
                break;
            }
        }

        // Parse identify data
        let words = unsafe {
            core::slice::from_raw_parts(buffer.as_ptr() as *const u16, 256)
        };

        // LBA48 sector count at words 100-103
        let sectors = (words[100] as u64)
            | ((words[101] as u64) << 16)
            | ((words[102] as u64) << 32)
            | ((words[103] as u64) << 48);

        if sectors > 0 {
            Some(DiskInfo {
                sectors,
                sector_size: 512,
            })
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DiskInfo {
    pub sectors: u64,
    pub sector_size: u16,
}
