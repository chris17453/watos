//! FAT12/16/32 Filesystem implementation for WATOS
//!
//! Provides a VFS-compatible FAT filesystem driver supporting:
//! - FAT12 (floppy disks, small volumes)
//! - FAT16 (small to medium volumes)
//! - FAT32 (large volumes)

#![no_std]

extern crate alloc;

mod bpb;
mod cluster;
mod dir;
mod file;
mod table;

use alloc::boxed::Box;
use alloc::vec::Vec;

use watos_vfs::{
    DirEntry, FileMode, FileOperations, FileStat, Filesystem, FsStats,
    VfsError, VfsResult,
};
use watos_driver_framework::block::BlockDevice;

pub use bpb::{BiosParameterBlock, FatType};
pub use dir::{FatDirEntry, DirEntryIterator};

/// FAT filesystem driver
pub struct FatFilesystem<D: BlockDevice> {
    /// Underlying block device
    device: D,
    /// BIOS Parameter Block (parsed from boot sector)
    bpb: BiosParameterBlock,
    /// FAT type (12, 16, or 32)
    fat_type: FatType,
    /// First data sector
    first_data_sector: u64,
    /// Sectors per cluster
    sectors_per_cluster: u32,
    /// Sector size
    sector_size: u32,
}

impl<D: BlockDevice> FatFilesystem<D> {
    /// Create a new FAT filesystem from a block device
    pub fn new(mut device: D) -> VfsResult<Self> {
        // Read boot sector
        let mut boot_sector = [0u8; 512];
        device
            .read_sectors(0, &mut boot_sector)
            .map_err(|_| VfsError::IoError)?;

        // Parse BPB
        let bpb = BiosParameterBlock::parse(&boot_sector)?;
        let fat_type = bpb.fat_type();

        // Calculate first data sector
        let root_dir_sectors = if bpb.root_entry_count > 0 {
            ((bpb.root_entry_count as u32 * 32) + (bpb.bytes_per_sector as u32 - 1))
                / bpb.bytes_per_sector as u32
        } else {
            0
        };

        let fat_size = if bpb.fat_size_16 != 0 {
            bpb.fat_size_16 as u32
        } else {
            bpb.fat_size_32
        };

        let first_data_sector = bpb.reserved_sector_count as u64
            + (bpb.num_fats as u64 * fat_size as u64)
            + root_dir_sectors as u64;

        let sectors_per_cluster = bpb.sectors_per_cluster as u32;
        let sector_size = bpb.bytes_per_sector as u32;

        Ok(FatFilesystem {
            device,
            bpb,
            fat_type,
            first_data_sector,
            sectors_per_cluster,
            sector_size,
        })
    }

    /// Get the FAT type
    pub fn fat_type(&self) -> FatType {
        self.fat_type
    }

    /// Convert cluster number to sector number
    fn cluster_to_sector(&self, cluster: u32) -> u64 {
        self.first_data_sector + ((cluster as u64 - 2) * self.sectors_per_cluster as u64)
    }

    /// Read a cluster into a buffer
    fn read_cluster(&mut self, cluster: u32, buffer: &mut [u8]) -> VfsResult<()> {
        let sector = self.cluster_to_sector(cluster);
        let sectors = self.sectors_per_cluster;

        for i in 0..sectors {
            let offset = (i * self.sector_size) as usize;
            let end = offset + self.sector_size as usize;
            if end > buffer.len() {
                break;
            }
            self.device
                .read_sectors(sector + i as u64, &mut buffer[offset..end])
                .map_err(|_| VfsError::IoError)?;
        }

        Ok(())
    }

    /// Write a cluster from a buffer
    fn write_cluster(&mut self, cluster: u32, buffer: &[u8]) -> VfsResult<()> {
        let sector = self.cluster_to_sector(cluster);
        let sectors = self.sectors_per_cluster;

        for i in 0..sectors {
            let offset = (i * self.sector_size) as usize;
            let end = offset + self.sector_size as usize;
            if end > buffer.len() {
                break;
            }
            self.device
                .write_sectors(sector + i as u64, &buffer[offset..end])
                .map_err(|_| VfsError::IoError)?;
        }

        Ok(())
    }

    /// Read next cluster from FAT
    fn next_cluster(&mut self, cluster: u32) -> VfsResult<Option<u32>> {
        table::read_fat_entry(&mut self.device, &self.bpb, self.fat_type, cluster)
    }

    /// Find a file/directory by path
    fn find_entry(&mut self, path: &str) -> VfsResult<FatDirEntry> {
        let path = path.trim_start_matches('/');
        if path.is_empty() {
            // Return root directory pseudo-entry
            return Ok(FatDirEntry::root_dir(&self.bpb, self.fat_type));
        }

        let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = FatDirEntry::root_dir(&self.bpb, self.fat_type);

        for component in components {
            if !current.is_directory() {
                return Err(VfsError::NotADirectory);
            }

            current = self.find_in_directory(&current, component)?;
        }

        Ok(current)
    }

    /// Find an entry in a directory
    fn find_in_directory(&mut self, dir: &FatDirEntry, name: &str) -> VfsResult<FatDirEntry> {
        let cluster_size = (self.sectors_per_cluster * self.sector_size) as usize;
        let mut buffer = alloc::vec![0u8; cluster_size];

        let mut cluster = dir.first_cluster();
        if cluster == 0 && self.fat_type != FatType::Fat32 {
            // FAT12/16 root directory
            return self.find_in_root_dir(name);
        }

        while cluster >= 2 {
            self.read_cluster(cluster, &mut buffer)?;

            for entry in DirEntryIterator::new(&buffer) {
                if entry.matches_name(name) {
                    return Ok(entry);
                }
            }

            match self.next_cluster(cluster)? {
                Some(next) => cluster = next,
                None => break,
            }
        }

        Err(VfsError::NotFound)
    }

    /// Find entry in FAT12/16 root directory
    fn find_in_root_dir(&mut self, name: &str) -> VfsResult<FatDirEntry> {
        let root_dir_sectors = ((self.bpb.root_entry_count as u32 * 32)
            + (self.bpb.bytes_per_sector as u32 - 1))
            / self.bpb.bytes_per_sector as u32;

        let root_start = self.bpb.reserved_sector_count as u64
            + (self.bpb.num_fats as u64 * self.bpb.fat_size_16 as u64);

        let mut sector_buf = [0u8; 512];

        for i in 0..root_dir_sectors {
            self.device
                .read_sectors(root_start + i as u64, &mut sector_buf)
                .map_err(|_| VfsError::IoError)?;

            for entry in DirEntryIterator::new(&sector_buf) {
                if entry.matches_name(name) {
                    return Ok(entry);
                }
            }
        }

        Err(VfsError::NotFound)
    }

    /// Read directory entries
    fn read_directory(&mut self, dir: &FatDirEntry) -> VfsResult<Vec<DirEntry>> {
        let cluster_size = (self.sectors_per_cluster * self.sector_size) as usize;
        let mut buffer = alloc::vec![0u8; cluster_size];
        let mut entries = Vec::new();

        let mut cluster = dir.first_cluster();
        if cluster == 0 && self.fat_type != FatType::Fat32 {
            // FAT12/16 root directory
            return self.read_root_dir_entries();
        }

        while cluster >= 2 {
            self.read_cluster(cluster, &mut buffer)?;

            for fat_entry in DirEntryIterator::new(&buffer) {
                if let Some(vfs_entry) = fat_entry.to_vfs_entry() {
                    entries.push(vfs_entry);
                }
            }

            match self.next_cluster(cluster)? {
                Some(next) => cluster = next,
                None => break,
            }
        }

        Ok(entries)
    }

    /// Read FAT12/16 root directory entries
    fn read_root_dir_entries(&mut self) -> VfsResult<Vec<DirEntry>> {
        let root_dir_sectors = ((self.bpb.root_entry_count as u32 * 32)
            + (self.bpb.bytes_per_sector as u32 - 1))
            / self.bpb.bytes_per_sector as u32;

        let root_start = self.bpb.reserved_sector_count as u64
            + (self.bpb.num_fats as u64 * self.bpb.fat_size_16 as u64);

        let mut sector_buf = [0u8; 512];
        let mut entries = Vec::new();

        for i in 0..root_dir_sectors {
            self.device
                .read_sectors(root_start + i as u64, &mut sector_buf)
                .map_err(|_| VfsError::IoError)?;

            for fat_entry in DirEntryIterator::new(&sector_buf) {
                if let Some(vfs_entry) = fat_entry.to_vfs_entry() {
                    entries.push(vfs_entry);
                }
            }
        }

        Ok(entries)
    }
}

impl<D: BlockDevice + Send + Sync + 'static> Filesystem for FatFilesystem<D> {
    fn name(&self) -> &'static str {
        match self.fat_type {
            FatType::Fat12 => "FAT12",
            FatType::Fat16 => "FAT16",
            FatType::Fat32 => "FAT32",
        }
    }

    fn open(&self, path: &str, mode: FileMode) -> VfsResult<Box<dyn FileOperations>> {
        // Need interior mutability for find_entry
        // For now, return error - real implementation needs RefCell or similar
        let _ = (path, mode);
        Err(VfsError::IoError) // TODO: implement with interior mutability
    }

    fn stat(&self, path: &str) -> VfsResult<FileStat> {
        // Similar issue - need interior mutability
        let _ = path;
        Err(VfsError::IoError) // TODO: implement
    }

    fn mkdir(&self, path: &str) -> VfsResult<()> {
        let _ = path;
        Err(VfsError::ReadOnly) // TODO: implement write support
    }

    fn unlink(&self, path: &str) -> VfsResult<()> {
        let _ = path;
        Err(VfsError::ReadOnly)
    }

    fn rmdir(&self, path: &str) -> VfsResult<()> {
        let _ = path;
        Err(VfsError::ReadOnly)
    }

    fn readdir(&self, path: &str) -> VfsResult<Vec<DirEntry>> {
        let _ = path;
        Err(VfsError::IoError) // TODO: implement with interior mutability
    }

    fn rename(&self, old_path: &str, new_path: &str) -> VfsResult<()> {
        let _ = (old_path, new_path);
        Err(VfsError::ReadOnly)
    }

    fn sync(&self) -> VfsResult<()> {
        Ok(())
    }

    fn statfs(&self) -> VfsResult<FsStats> {
        let total_sectors = if self.bpb.total_sectors_16 != 0 {
            self.bpb.total_sectors_16 as u64
        } else {
            self.bpb.total_sectors_32 as u64
        };

        let fat_size = if self.bpb.fat_size_16 != 0 {
            self.bpb.fat_size_16 as u64
        } else {
            self.bpb.fat_size_32 as u64
        };

        let data_sectors = total_sectors
            - self.bpb.reserved_sector_count as u64
            - (self.bpb.num_fats as u64 * fat_size);

        let total_clusters = data_sectors / self.sectors_per_cluster as u64;

        Ok(FsStats {
            total_blocks: total_clusters,
            free_blocks: 0, // TODO: count free clusters
            block_size: self.sectors_per_cluster * self.sector_size,
            total_inodes: 0,
            free_inodes: 0,
            max_name_len: 255, // LFN support
        })
    }
}
