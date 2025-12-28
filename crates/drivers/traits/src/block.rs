//! Block Device Trait
//!
//! Implemented by storage drivers (AHCI, NVMe, IDE, etc.)
//! Used by filesystem drivers (FAT, WFS, etc.)

use crate::DriverError;

/// Block device geometry
#[derive(Debug, Clone, Copy)]
pub struct BlockGeometry {
    /// Sector size in bytes (usually 512)
    pub sector_size: u32,
    /// Total number of sectors
    pub total_sectors: u64,
    /// Optimal transfer size in sectors
    pub optimal_transfer: u32,
}

/// Block device interface for storage drivers
pub trait BlockDevice {
    /// Get device geometry
    fn geometry(&self) -> BlockGeometry;

    /// Read sectors from the device
    ///
    /// # Arguments
    /// * `start` - Starting sector (LBA)
    /// * `buffer` - Buffer to read into (size determines sector count)
    ///
    /// # Returns
    /// Number of bytes read on success
    fn read_sectors(&mut self, start: u64, buffer: &mut [u8]) -> Result<usize, DriverError>;

    /// Write sectors to the device
    ///
    /// # Arguments
    /// * `start` - Starting sector (LBA)
    /// * `buffer` - Buffer to write from (size determines sector count)
    ///
    /// # Returns
    /// Number of bytes written on success
    fn write_sectors(&mut self, start: u64, buffer: &[u8]) -> Result<usize, DriverError>;

    /// Flush any cached writes to the device
    fn flush(&mut self) -> Result<(), DriverError> {
        Ok(()) // Default: no caching
    }
}

/// Convenience methods for BlockDevice
pub trait BlockDeviceExt: BlockDevice {
    /// Get sector size
    fn sector_size(&self) -> u32 {
        self.geometry().sector_size
    }

    /// Get total sectors
    fn total_sectors(&self) -> u64 {
        self.geometry().total_sectors
    }

    /// Get total device size in bytes
    fn size_bytes(&self) -> u64 {
        self.geometry().total_sectors * self.geometry().sector_size as u64
    }

    /// Read a single sector
    fn read_sector(&mut self, lba: u64, buf: &mut [u8]) -> Result<usize, DriverError> {
        let sector_size = self.sector_size() as usize;
        if buf.len() < sector_size {
            return Err(DriverError::BufferTooSmall);
        }
        self.read_sectors(lba, &mut buf[..sector_size])
    }

    /// Write a single sector
    fn write_sector(&mut self, lba: u64, buf: &[u8]) -> Result<usize, DriverError> {
        let sector_size = self.sector_size() as usize;
        if buf.len() < sector_size {
            return Err(DriverError::BufferTooSmall);
        }
        self.write_sectors(lba, &buf[..sector_size])
    }
}

// Auto-implement BlockDeviceExt for all BlockDevice implementors
impl<T: BlockDevice + ?Sized> BlockDeviceExt for T {}
