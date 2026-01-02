//! WATOS AHCI (SATA) Driver
//!
//! Implements the BlockDevice trait for SATA drives via AHCI controller.

#![no_std]

// AHCI debug macro - only outputs when 'debug' feature is enabled
#[cfg(feature = "debug")]
macro_rules! ahci_debug {
    ($($arg:tt)*) => {{
        unsafe {
            $($arg)*
        }
    }};
}

#[cfg(not(feature = "debug"))]
macro_rules! ahci_debug {
    ($($arg:tt)*) => {};
}

use core::ptr::{read_volatile, write_volatile};
use watos_driver_traits::{Driver, DriverInfo, DriverState, DriverError};
use watos_driver_traits::block::{BlockDevice, BlockGeometry};
use watos_driver_traits::bus::PciAddress;
use watos_driver_pci::PciDriver;

// AHCI HBA Memory Registers
const HBA_CAP: u64 = 0x00;
const HBA_GHC: u64 = 0x04;
const HBA_IS: u64 = 0x08;
const HBA_PI: u64 = 0x0C;
const HBA_VS: u64 = 0x10;

// Port registers (offset from port base)
const PORT_CLB: u64 = 0x00;
const PORT_FB: u64 = 0x08;
const PORT_IS: u64 = 0x10;
const PORT_IE: u64 = 0x14;
const PORT_CMD: u64 = 0x18;
const PORT_TFD: u64 = 0x20;
const PORT_SIG: u64 = 0x24;
const PORT_SSTS: u64 = 0x28;
const PORT_SCTL: u64 = 0x2C;
const PORT_SERR: u64 = 0x30;
const PORT_CI: u64 = 0x38;

// Port CMD bits
const PORT_CMD_ST: u32 = 1 << 0;
const PORT_CMD_FRE: u32 = 1 << 4;
const PORT_CMD_FR: u32 = 1 << 14;
const PORT_CMD_CR: u32 = 1 << 15;

// FIS Types
const FIS_TYPE_REG_H2D: u8 = 0x27;

// ATA Commands
const ATA_CMD_READ_DMA_EXT: u8 = 0x25;
const ATA_CMD_WRITE_DMA_EXT: u8 = 0x35;
const ATA_CMD_IDENTIFY: u8 = 0xEC;

/// AHCI command header
#[repr(C, packed)]
struct CommandHeader {
    flags: u16,
    prdtl: u16,
    prdbc: u32,
    ctba: u64,
    reserved: [u32; 4],
}

/// AHCI command table
#[repr(C, packed)]
struct CommandTable {
    cfis: [u8; 64],
    acmd: [u8; 16],
    reserved: [u8; 48],
    prdt: [PrdtEntry; 8],
}

/// PRD Table entry
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct PrdtEntry {
    dba: u64,
    reserved: u32,
    dbc: u32,
}

/// FIS Register H2D
#[repr(C, packed)]
struct FisRegH2D {
    fis_type: u8,
    flags: u8,
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

/// Disk information from IDENTIFY command
#[derive(Debug, Clone)]
pub struct DiskInfo {
    /// Total sectors (LBA48)
    pub sectors: u64,
    /// Model string
    pub model: [u8; 40],
    /// Serial number
    pub serial: [u8; 20],
}

/// AHCI SATA Driver
pub struct AhciDriver {
    state: DriverState,
    mmio_base: u64,
    port: u8,
    cmd_list: u64,
    cmd_table: u64,
    fis_base: u64,
    lba_offset: u64,
    sector_size: usize,
    total_sectors: u64,
}

impl AhciDriver {
    /// Base memory address for AHCI structures
    /// Uses layout::PHYS_AHCI_DMA (0x700000) from the central memory layout
    const AHCI_MEM_BASE: u64 = watos_mem::layout::PHYS_AHCI_DMA;
    const PORT_MEM_SIZE: u64 = 0x4000;

    fn port_cmd_list(port: u8) -> u64 {
        Self::AHCI_MEM_BASE + (port as u64 * Self::PORT_MEM_SIZE)
    }

    fn port_fis_base(port: u8) -> u64 {
        Self::AHCI_MEM_BASE + (port as u64 * Self::PORT_MEM_SIZE) + 0x1000
    }

    fn port_cmd_table(port: u8) -> u64 {
        Self::AHCI_MEM_BASE + (port as u64 * Self::PORT_MEM_SIZE) + 0x2000
    }

    /// Probe for AHCI controller and create driver
    pub fn probe() -> Option<Self> {
        Self::probe_port(0xFF)
    }

    /// Probe for AHCI controller on specific port using provided PCI driver
    pub fn probe_with_pci(pci: &PciDriver, target_port: u8) -> Option<Self> {
        use watos_driver_traits::bus::{PciBus, PciBar, pci_class};

        let devices = pci.find_by_class(pci_class::MASS_STORAGE, pci_class::SATA);

        for dev in devices {
            let mmio_base = match dev.bars[5] {
                PciBar::Memory { address, .. } if address != 0 => address,
                _ => continue,
            };

            pci.enable_bus_master(dev.address);
            pci.enable_memory_space(dev.address);

            let pi = unsafe { read_volatile((mmio_base + HBA_PI) as *const u32) };

            for port in 0..32u8 {
                if pi & (1 << port) != 0 {
                    let port_base = mmio_base + 0x100 + (port as u64 * 0x80);
                    let ssts = unsafe { read_volatile((port_base + PORT_SSTS) as *const u32) };

                    let det = ssts & 0xF;
                    let ipm = (ssts >> 8) & 0xF;

                    if det == 3 && ipm == 1 {
                        if target_port == 0xFF || target_port == port {
                            return Some(Self {
                                state: DriverState::Loaded,
                                mmio_base,
                                port,
                                cmd_list: Self::port_cmd_list(port),
                                cmd_table: Self::port_cmd_table(port),
                                fis_base: Self::port_fis_base(port),
                                lba_offset: 0,
                                sector_size: 512,
                                total_sectors: 0,
                            });
                        }
                    }
                }
            }
        }

        None
    }

    /// Probe for AHCI controller on specific port
    pub fn probe_port(target_port: u8) -> Option<Self> {
        use watos_driver_traits::Driver;

        let mut pci = PciDriver::new();
        pci.init().ok()?;
        Self::probe_with_pci(&pci, target_port)
    }

    fn port_base(&self) -> u64 {
        self.mmio_base + 0x100 + (self.port as u64 * 0x80)
    }

    fn read_port(&self, offset: u64) -> u32 {
        unsafe { read_volatile((self.port_base() + offset) as *const u32) }
    }

    fn write_port(&self, offset: u64, value: u32) {
        unsafe { write_volatile((self.port_base() + offset) as *mut u32, value) }
    }

    fn init_port(&mut self) {
        ahci_debug! {
            watos_arch::serial_write(b"[AHCI init_port] port=");
            watos_arch::serial_hex(self.port as u64);
            watos_arch::serial_write(b" cmd_list=");
            watos_arch::serial_hex(self.cmd_list);
            watos_arch::serial_write(b" fis_base=");
            watos_arch::serial_hex(self.fis_base);
            watos_arch::serial_write(b" cmd_table=");
            watos_arch::serial_hex(self.cmd_table);
            watos_arch::serial_write(b"\r\n");
        }

        // Stop command engine
        let cmd = self.read_port(PORT_CMD);
        self.write_port(PORT_CMD, cmd & !(PORT_CMD_ST | PORT_CMD_FRE));

        // Wait for engine to stop
        for _ in 0..1000000 {
            if (self.read_port(PORT_CMD) & (PORT_CMD_CR | PORT_CMD_FR)) == 0 {
                break;
            }
        }

        // Clear memory
        unsafe {
            core::ptr::write_bytes(self.cmd_list as *mut u8, 0, 1024);
            core::ptr::write_bytes(self.fis_base as *mut u8, 0, 256);
            core::ptr::write_bytes(self.cmd_table as *mut u8, 0, 4096);
        }

        // Set command list and FIS base
        self.write_port(PORT_CLB, self.cmd_list as u32);
        unsafe {
            write_volatile((self.port_base() + PORT_CLB + 4) as *mut u32,
                          (self.cmd_list >> 32) as u32);
        }

        self.write_port(PORT_FB, self.fis_base as u32);
        unsafe {
            write_volatile((self.port_base() + PORT_FB + 4) as *mut u32,
                          (self.fis_base >> 32) as u32);
        }

        // Clear status
        self.write_port(PORT_IS, 0xFFFFFFFF);
        self.write_port(PORT_SERR, 0xFFFFFFFF);

        // Setup command header
        let cmd_header = self.cmd_list as *mut CommandHeader;
        unsafe {
            (*cmd_header).ctba = self.cmd_table;
            (*cmd_header).prdtl = 1;
        }

        // Start command engine
        let cmd = self.read_port(PORT_CMD);
        self.write_port(PORT_CMD, cmd | PORT_CMD_FRE | PORT_CMD_ST);
    }

    fn issue_command(&mut self, cmd: u8, lba: u64, count: u16, buffer_addr: u64, write: bool) -> Result<(), DriverError> {
        let cmd_header = self.cmd_list as *mut CommandHeader;
        unsafe {
            let flags: u16 = (core::mem::size_of::<FisRegH2D>() / 4) as u16;
            let flags = if write { flags | (1 << 6) } else { flags };
            (*cmd_header).flags = flags;
            (*cmd_header).prdtl = 1;
            (*cmd_header).prdbc = 0;
            (*cmd_header).ctba = self.cmd_table;
        }

        ahci_debug! {
            let hdr = cmd_header;
            watos_arch::serial_write(b"[AHCI] CommandHeader: flags=");
            watos_arch::serial_hex((*hdr).flags as u64);
            watos_arch::serial_write(b" prdtl=");
            watos_arch::serial_hex((*hdr).prdtl as u64);
            watos_arch::serial_write(b" ctba=");
            watos_arch::serial_hex((*hdr).ctba);
            watos_arch::serial_write(b"\r\n");
        }

        let cmd_table = self.cmd_table as *mut CommandTable;
        unsafe {
            core::ptr::write_bytes((*cmd_table).cfis.as_mut_ptr(), 0, 64);

            let fis = (*cmd_table).cfis.as_mut_ptr() as *mut FisRegH2D;
            (*fis).fis_type = FIS_TYPE_REG_H2D;
            (*fis).flags = 0x80;
            (*fis).command = cmd;
            (*fis).device = 0x40;

            (*fis).lba0 = lba as u8;
            (*fis).lba1 = (lba >> 8) as u8;
            (*fis).lba2 = (lba >> 16) as u8;
            (*fis).lba3 = (lba >> 24) as u8;
            (*fis).lba4 = (lba >> 32) as u8;
            (*fis).lba5 = (lba >> 40) as u8;

            (*fis).count_low = count as u8;
            (*fis).count_high = (count >> 8) as u8;

            (*cmd_table).prdt[0].dba = buffer_addr;
            (*cmd_table).prdt[0].dbc = ((count as u32 * 512) - 1) | (1 << 31);
        }

        ahci_debug! {
            watos_arch::serial_write(b"[AHCI] FIS: cmd=");
            watos_arch::serial_hex(cmd as u64);
            watos_arch::serial_write(b" lba=");
            watos_arch::serial_hex(lba);
            watos_arch::serial_write(b" count=");
            watos_arch::serial_hex(count as u64);
            watos_arch::serial_write(b"\r\n");
        }

        self.write_port(PORT_IS, 0xFFFFFFFF);
        self.write_port(PORT_CI, 1);

        // Wait for completion
        for i in 0..10000000 {
            if self.read_port(PORT_CI) & 1 == 0 {
                let tfd = self.read_port(PORT_TFD);
                if (tfd & 0x01) != 0 {
                    ahci_debug! {
                        watos_arch::serial_write(b"[AHCI] TFD error\r\n");
                    }
                    return Err(DriverError::IoError);
                }
                return Ok(());
            }

            if (self.read_port(PORT_TFD) & 0x01) != 0 {
                ahci_debug! {
                    watos_arch::serial_write(b"[AHCI] TFD error during wait\r\n");
                }
                return Err(DriverError::IoError);
            }
        }

        ahci_debug! {
            watos_arch::serial_write(b"[AHCI] TIMEOUT\r\n");
        }

        Err(DriverError::Timeout)
    }

    /// Get disk info via IDENTIFY command
    pub fn identify(&mut self) -> Result<DiskInfo, DriverError> {
        let buffer = [0u8; 512];

        self.issue_command(ATA_CMD_IDENTIFY, 0, 1, buffer.as_ptr() as u64, false)?;

        let words = unsafe {
            core::slice::from_raw_parts(buffer.as_ptr() as *const u16, 256)
        };

        let sectors = if words[83] & (1 << 10) != 0 {
            (words[100] as u64) |
            ((words[101] as u64) << 16) |
            ((words[102] as u64) << 32) |
            ((words[103] as u64) << 48)
        } else {
            (words[60] as u64) | ((words[61] as u64) << 16)
        };

        let mut model = [0u8; 40];
        let mut serial = [0u8; 20];

        for i in 0..20 {
            model[i * 2] = (words[27 + i] >> 8) as u8;
            model[i * 2 + 1] = words[27 + i] as u8;
        }
        for i in 0..10 {
            serial[i * 2] = (words[10 + i] >> 8) as u8;
            serial[i * 2 + 1] = words[10 + i] as u8;
        }

        Ok(DiskInfo { sectors, model, serial })
    }
}

impl Driver for AhciDriver {
    fn info(&self) -> DriverInfo {
        DriverInfo {
            name: "ahci",
            version: "0.1.0",
            author: "WATOS",
            description: "AHCI SATA controller driver",
        }
    }

    fn state(&self) -> DriverState {
        self.state
    }

    fn init(&mut self) -> Result<(), DriverError> {
        if self.state != DriverState::Loaded {
            return Err(DriverError::InvalidState);
        }

        self.init_port();
        self.state = DriverState::Ready;
        Ok(())
    }

    fn start(&mut self) -> Result<(), DriverError> {
        if self.state != DriverState::Ready {
            return Err(DriverError::InvalidState);
        }

        if let Ok(info) = self.identify() {
            self.total_sectors = info.sectors;
        }

        self.state = DriverState::Active;
        Ok(())
    }

    fn stop(&mut self) -> Result<(), DriverError> {
        if self.state != DriverState::Active {
            return Err(DriverError::InvalidState);
        }

        let cmd = self.read_port(PORT_CMD);
        self.write_port(PORT_CMD, cmd & !(PORT_CMD_ST | PORT_CMD_FRE));

        self.state = DriverState::Ready;
        Ok(())
    }
}

impl BlockDevice for AhciDriver {
    fn geometry(&self) -> BlockGeometry {
        BlockGeometry {
            sector_size: self.sector_size as u32,
            total_sectors: self.total_sectors,
            optimal_transfer: 128,
        }
    }

    fn read_sectors(&mut self, start: u64, buffer: &mut [u8]) -> Result<usize, DriverError> {
        ahci_debug! {
            let cr3: u64;
            core::arch::asm!("mov {}, cr3", out(reg) cr3);
            watos_arch::serial_write(b"[AHCI] read_sectors start=");
            watos_arch::serial_hex(start);
            watos_arch::serial_write(b" len=");
            watos_arch::serial_hex(buffer.len() as u64);
            watos_arch::serial_write(b"\r\n");
        }

        if self.state != DriverState::Active {
            return Err(DriverError::InvalidState);
        }

        let sectors = buffer.len() / self.sector_size;
        if sectors == 0 {
            return Ok(0);
        }

        let absolute_lba = start + self.lba_offset;
        let buffer_addr = buffer.as_ptr() as u64;

        self.issue_command(ATA_CMD_READ_DMA_EXT, absolute_lba, sectors as u16, buffer_addr, false)?;

        Ok(sectors * self.sector_size)
    }

    fn write_sectors(&mut self, start: u64, buffer: &[u8]) -> Result<usize, DriverError> {
        if self.state != DriverState::Active {
            return Err(DriverError::InvalidState);
        }

        let sectors = buffer.len() / self.sector_size;
        if sectors == 0 {
            return Ok(0);
        }

        let absolute_lba = start + self.lba_offset;
        self.issue_command(ATA_CMD_WRITE_DMA_EXT, absolute_lba, sectors as u16, buffer.as_ptr() as u64, true)?;

        Ok(sectors * self.sector_size)
    }

    fn flush(&mut self) -> Result<(), DriverError> {
        Ok(())
    }
}
