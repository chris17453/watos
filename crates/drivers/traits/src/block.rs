//! Block Device Trait
//!
//! Implemented by storage drivers (AHCI, NVMe, IDE, etc.)
//! Used by filesystem drivers (FAT, WFS, etc.)

use crate::{DriverResult, DriverError};

/// Block device interface for storage drivers
pub trait BlockDevice: Send + Sync {
    /// Read sectors from the device
    ///
    /// # Arguments
    /// * `lba` - Logical Block Address (sector number)
    /// * `count` - Number of sectors to read
    /// * `buf` - Buffer to read into (must be at least count * sector_size bytes)
    fn read_sectors(&self, lba: u64, count: usize, buf: &mut [u8]) -> DriverResult<()>;

    /// Write sectors to the device
    ///
    /// # Arguments
    /// * `lba` - Logical Block Address (sector number)
    /// * `count` - Number of sectors to write
    /// * `buf` - Buffer to write from (must be at least count * sector_size bytes)
    fn write_sectors(&self, lba: u64, count: usize, buf: &[u8]) -> DriverResult<()>;

    /// Get the sector size in bytes (usually 512)
    fn sector_size(&self) -> usize;

    /// Get the total number of sectors
    fn sector_count(&self) -> u64;

    /// Flush any cached writes to the device
    fn flush(&self) -> DriverResult<()> {
        Ok(()) // Default: no caching
    }

    /// Get device information
    fn info(&self) -> BlockDeviceInfo;
}

/// Information about a block device
#[derive(Debug, Clone)]
pub struct BlockDeviceInfo {
    /// Device name/model
    pub name: &'static str,
    /// Serial number (if available)
    pub serial: Option<&'static str>,
    /// Total size in bytes
    pub size_bytes: u64,
    /// Sector size in bytes
    pub sector_size: usize,
    /// Is the device read-only?
    pub read_only: bool,
    /// Is the device removable?
    pub removable: bool,
}

/// Convenience methods for BlockDevice
pub trait BlockDeviceExt: BlockDevice {
    /// Read a single sector
    fn read_sector(&self, lba: u64, buf: &mut [u8]) -> DriverResult<()> {
        if buf.len() < self.sector_size() {
            return Err(DriverError::BufferTooSmall);
        }
        self.read_sectors(lba, 1, buf)
    }

    /// Write a single sector
    fn write_sector(&self, lba: u64, buf: &[u8]) -> DriverResult<()> {
        if buf.len() < self.sector_size() {
            return Err(DriverError::BufferTooSmall);
        }
        self.write_sectors(lba, 1, buf)
    }

    /// Get total device size in bytes
    fn size_bytes(&self) -> u64 {
        self.sector_count() * self.sector_size() as u64
    }
}

// Auto-implement BlockDeviceExt for all BlockDevice implementors
impl<T: BlockDevice + ?Sized> BlockDeviceExt for T {}
