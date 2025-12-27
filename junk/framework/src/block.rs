//! Block device driver abstraction
//!
//! For storage devices: AHCI, NVMe, floppy, etc.

use crate::{Driver, DriverError};

/// Block device geometry
#[derive(Debug, Clone, Copy)]
pub struct BlockGeometry {
    /// Bytes per sector
    pub sector_size: u32,
    /// Total number of sectors
    pub total_sectors: u64,
    /// Optimal transfer size in sectors
    pub optimal_transfer: u32,
}

/// Block device trait
pub trait BlockDevice: Driver {
    /// Get device geometry
    fn geometry(&self) -> BlockGeometry;

    /// Read sectors from the device
    ///
    /// # Arguments
    /// * `start_sector` - First sector to read
    /// * `buffer` - Buffer to read into (must be sector-aligned size)
    fn read_sectors(&mut self, start_sector: u64, buffer: &mut [u8]) -> Result<usize, DriverError>;

    /// Write sectors to the device
    ///
    /// # Arguments
    /// * `start_sector` - First sector to write
    /// * `buffer` - Data to write (must be sector-aligned size)
    fn write_sectors(&mut self, start_sector: u64, buffer: &[u8]) -> Result<usize, DriverError>;

    /// Flush any cached writes to the device
    fn flush(&mut self) -> Result<(), DriverError>;

    /// Check if device is read-only
    fn is_readonly(&self) -> bool {
        false
    }

    /// Check if device is removable
    fn is_removable(&self) -> bool {
        false
    }
}

/// Block device identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockDeviceId(pub u32);
