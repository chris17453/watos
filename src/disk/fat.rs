//! FAT Filesystem Driver
//!
//! Support for FAT12, FAT16, and FAT32 filesystems
//! Read-only for DOS compatibility (mounting disk images, floppies, etc.)

use super::ahci::AhciController;
use alloc::string::String;
use alloc::vec::Vec;

// FAT types
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FatType {
    Fat12,
    Fat16,
    Fat32,
}

// Directory entry attributes
pub const ATTR_READ_ONLY: u8 = 0x01;
pub const ATTR_HIDDEN: u8 = 0x02;
pub const ATTR_SYSTEM: u8 = 0x04;
pub const ATTR_VOLUME_ID: u8 = 0x08;
pub const ATTR_DIRECTORY: u8 = 0x10;
pub const ATTR_ARCHIVE: u8 = 0x20;
pub const ATTR_LONG_NAME: u8 = 0x0F;

/// FAT Boot Sector / BPB
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct FatBootSector {
    jmp_boot: [u8; 3],
    oem_name: [u8; 8],
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sectors: u16,
    num_fats: u8,
    root_entry_count: u16,    // 0 for FAT32
    total_sectors_16: u16,    // 0 for FAT32
    media_type: u8,
    fat_size_16: u16,         // 0 for FAT32
    sectors_per_track: u16,
    num_heads: u16,
    hidden_sectors: u32,
    total_sectors_32: u32,
}

/// FAT32 Extended Boot Sector
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct Fat32ExtBoot {
    fat_size_32: u32,
    ext_flags: u16,
    fs_version: u16,
    root_cluster: u32,
    fs_info: u16,
    backup_boot_sector: u16,
    reserved: [u8; 12],
    drive_number: u8,
    reserved1: u8,
    boot_sig: u8,
    volume_id: u32,
    volume_label: [u8; 11],
    fs_type: [u8; 8],
}

/// FAT Directory Entry (8.3 format)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct FatDirEntry {
    pub name: [u8; 11],       // 8.3 filename
    pub attr: u8,
    pub nt_reserved: u8,
    pub create_time_tenth: u8,
    pub create_time: u16,
    pub create_date: u16,
    pub access_date: u16,
    pub first_cluster_hi: u16,
    pub write_time: u16,
    pub write_date: u16,
    pub first_cluster_lo: u16,
    pub file_size: u32,
}

impl FatDirEntry {
    pub fn is_free(&self) -> bool {
        self.name[0] == 0x00 || self.name[0] == 0xE5
    }

    pub fn is_directory(&self) -> bool {
        self.attr & ATTR_DIRECTORY != 0
    }

    pub fn is_volume_label(&self) -> bool {
        self.attr & ATTR_VOLUME_ID != 0
    }

    pub fn is_long_name(&self) -> bool {
        (self.attr & ATTR_LONG_NAME) == ATTR_LONG_NAME
    }

    pub fn first_cluster(&self) -> u32 {
        ((self.first_cluster_hi as u32) << 16) | (self.first_cluster_lo as u32)
    }

    /// Get 8.3 filename as string
    pub fn short_name(&self) -> String {
        let mut name = String::new();

        // Name part (first 8 chars)
        for i in 0..8 {
            if self.name[i] == 0x20 {
                break;
            }
            name.push(self.name[i] as char);
        }

        // Extension (last 3 chars)
        if self.name[8] != 0x20 {
            name.push('.');
            for i in 8..11 {
                if self.name[i] == 0x20 {
                    break;
                }
                name.push(self.name[i] as char);
            }
        }

        name
    }
}

pub struct FatFs {
    disk: AhciController,
    fat_type: FatType,
    bytes_per_sector: u32,
    sectors_per_cluster: u32,
    reserved_sectors: u32,
    num_fats: u32,
    fat_size: u32,           // Sectors per FAT
    root_dir_sectors: u32,   // FAT12/16 only
    root_cluster: u32,       // FAT32 only
    first_data_sector: u32,
    total_clusters: u32,
    partition_offset: u64,   // LBA offset if using partition
}

impl FatFs {
    /// Mount a FAT filesystem
    /// partition_offset: LBA offset (0 for raw disk, or partition start)
    pub fn mount(mut disk: AhciController, partition_offset: u64) -> Option<Self> {
        let mut sector = [0u8; 512];

        if !disk.read_sectors(partition_offset, 1, &mut sector) {
            return None;
        }

        // Parse boot sector
        let bs = unsafe { &*(sector.as_ptr() as *const FatBootSector) };

        // Validate
        if bs.bytes_per_sector < 512 || bs.bytes_per_sector > 4096 {
            return None;
        }
        if bs.sectors_per_cluster == 0 {
            return None;
        }
        if bs.num_fats == 0 {
            return None;
        }

        let bytes_per_sector = bs.bytes_per_sector as u32;
        let sectors_per_cluster = bs.sectors_per_cluster as u32;
        let reserved_sectors = bs.reserved_sectors as u32;
        let num_fats = bs.num_fats as u32;
        let root_entry_count = bs.root_entry_count as u32;

        let total_sectors = if bs.total_sectors_16 != 0 {
            bs.total_sectors_16 as u32
        } else {
            bs.total_sectors_32
        };

        let fat_size = if bs.fat_size_16 != 0 {
            bs.fat_size_16 as u32
        } else {
            // FAT32 - read extended boot record
            let ext = unsafe {
                &*((sector.as_ptr().add(36)) as *const Fat32ExtBoot)
            };
            ext.fat_size_32
        };

        // Calculate root directory sectors (0 for FAT32)
        let root_dir_sectors = ((root_entry_count * 32) + (bytes_per_sector - 1)) / bytes_per_sector;

        // First data sector
        let first_data_sector = reserved_sectors + (num_fats * fat_size) + root_dir_sectors;

        // Total data sectors
        let data_sectors = total_sectors - first_data_sector;
        let total_clusters = data_sectors / sectors_per_cluster;

        // Determine FAT type by cluster count
        let fat_type = if total_clusters < 4085 {
            FatType::Fat12
        } else if total_clusters < 65525 {
            FatType::Fat16
        } else {
            FatType::Fat32
        };

        // Get root cluster for FAT32
        let root_cluster = if fat_type == FatType::Fat32 {
            let ext = unsafe {
                &*((sector.as_ptr().add(36)) as *const Fat32ExtBoot)
            };
            ext.root_cluster
        } else {
            0
        };

        Some(Self {
            disk,
            fat_type,
            bytes_per_sector,
            sectors_per_cluster,
            reserved_sectors,
            num_fats,
            fat_size,
            root_dir_sectors,
            root_cluster,
            first_data_sector,
            total_clusters,
            partition_offset,
        })
    }

    /// Read a sector from the filesystem
    fn read_sector(&mut self, sector: u32, buffer: &mut [u8]) -> bool {
        let lba = self.partition_offset + sector as u64;
        self.disk.read_sectors(lba, 1, buffer)
    }

    /// Get the first sector of a cluster
    fn cluster_to_sector(&self, cluster: u32) -> u32 {
        self.first_data_sector + (cluster - 2) * self.sectors_per_cluster
    }

    /// Read FAT entry for a cluster
    fn read_fat_entry(&mut self, cluster: u32) -> Option<u32> {
        let mut sector_buf = [0u8; 512];

        match self.fat_type {
            FatType::Fat12 => {
                let fat_offset = cluster + (cluster / 2);
                let fat_sector = self.reserved_sectors + (fat_offset / self.bytes_per_sector);
                let offset = (fat_offset % self.bytes_per_sector) as usize;

                if !self.read_sector(fat_sector, &mut sector_buf) {
                    return None;
                }

                let mut value = sector_buf[offset] as u32;

                // May span sectors
                if offset == (self.bytes_per_sector - 1) as usize {
                    if !self.read_sector(fat_sector + 1, &mut sector_buf) {
                        return None;
                    }
                    value |= (sector_buf[0] as u32) << 8;
                } else {
                    value |= (sector_buf[offset + 1] as u32) << 8;
                }

                if cluster & 1 != 0 {
                    value >>= 4;
                } else {
                    value &= 0x0FFF;
                }

                Some(value)
            }
            FatType::Fat16 => {
                let fat_offset = cluster * 2;
                let fat_sector = self.reserved_sectors + (fat_offset / self.bytes_per_sector);
                let offset = (fat_offset % self.bytes_per_sector) as usize;

                if !self.read_sector(fat_sector, &mut sector_buf) {
                    return None;
                }

                let value = u16::from_le_bytes([sector_buf[offset], sector_buf[offset + 1]]);
                Some(value as u32)
            }
            FatType::Fat32 => {
                let fat_offset = cluster * 4;
                let fat_sector = self.reserved_sectors + (fat_offset / self.bytes_per_sector);
                let offset = (fat_offset % self.bytes_per_sector) as usize;

                if !self.read_sector(fat_sector, &mut sector_buf) {
                    return None;
                }

                let value = u32::from_le_bytes([
                    sector_buf[offset], sector_buf[offset + 1],
                    sector_buf[offset + 2], sector_buf[offset + 3]
                ]) & 0x0FFFFFFF;
                Some(value)
            }
        }
    }

    /// Check if cluster is end of chain
    fn is_end_of_chain(&self, cluster: u32) -> bool {
        match self.fat_type {
            FatType::Fat12 => cluster >= 0x0FF8,
            FatType::Fat16 => cluster >= 0xFFF8,
            FatType::Fat32 => cluster >= 0x0FFFFFF8,
        }
    }

    /// List root directory entries
    pub fn list_root(&mut self) -> Vec<FatDirEntry> {
        let mut entries = Vec::new();
        let mut sector_buf = [0u8; 512];

        if self.fat_type == FatType::Fat32 {
            // FAT32: root directory is a cluster chain
            let mut cluster = self.root_cluster;
            while !self.is_end_of_chain(cluster) {
                let sector = self.cluster_to_sector(cluster);
                for s in 0..self.sectors_per_cluster {
                    if !self.read_sector(sector + s, &mut sector_buf) {
                        break;
                    }
                    self.parse_dir_entries(&sector_buf, &mut entries);
                }
                cluster = self.read_fat_entry(cluster).unwrap_or(0x0FFFFFFF);
            }
        } else {
            // FAT12/16: fixed root directory
            let root_dir_start = self.reserved_sectors + (self.num_fats * self.fat_size);
            for s in 0..self.root_dir_sectors {
                if !self.read_sector(root_dir_start + s, &mut sector_buf) {
                    break;
                }
                self.parse_dir_entries(&sector_buf, &mut entries);
            }
        }

        entries
    }

    fn parse_dir_entries(&self, sector: &[u8], entries: &mut Vec<FatDirEntry>) {
        let num_entries = self.bytes_per_sector as usize / 32;

        for i in 0..num_entries {
            let offset = i * 32;
            let entry = unsafe {
                *(sector.as_ptr().add(offset) as *const FatDirEntry)
            };

            if entry.name[0] == 0x00 {
                // End of directory
                break;
            }

            if entry.is_free() || entry.is_long_name() || entry.is_volume_label() {
                continue;
            }

            entries.push(entry);
        }
    }

    /// Read file contents
    pub fn read_file(&mut self, entry: &FatDirEntry) -> Option<Vec<u8>> {
        if entry.is_directory() {
            return None;
        }

        let mut data = Vec::with_capacity(entry.file_size as usize);
        let mut remaining = entry.file_size as usize;
        let mut cluster = entry.first_cluster();
        let mut sector_buf = [0u8; 512];

        let bytes_per_cluster = (self.sectors_per_cluster * self.bytes_per_sector) as usize;

        while remaining > 0 && !self.is_end_of_chain(cluster) && cluster >= 2 {
            let sector = self.cluster_to_sector(cluster);

            for s in 0..self.sectors_per_cluster {
                if remaining == 0 {
                    break;
                }

                if !self.read_sector(sector + s, &mut sector_buf) {
                    return None;
                }

                let to_copy = remaining.min(self.bytes_per_sector as usize);
                data.extend_from_slice(&sector_buf[..to_copy]);
                remaining = remaining.saturating_sub(self.bytes_per_sector as usize);
            }

            // Get next cluster
            cluster = self.read_fat_entry(cluster).unwrap_or(0x0FFFFFFF);
        }

        data.truncate(entry.file_size as usize);
        Some(data)
    }

    /// Find a file in root directory by name
    pub fn find_file(&mut self, name: &str) -> Option<FatDirEntry> {
        let entries = self.list_root();
        let name_upper = name.to_uppercase();

        for entry in entries {
            if entry.short_name().to_uppercase() == name_upper {
                return Some(entry);
            }
        }
        None
    }

    /// Get filesystem info
    pub fn info(&self) -> FatInfo {
        FatInfo {
            fat_type: self.fat_type,
            bytes_per_sector: self.bytes_per_sector,
            sectors_per_cluster: self.sectors_per_cluster,
            total_clusters: self.total_clusters,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FatInfo {
    pub fat_type: FatType,
    pub bytes_per_sector: u32,
    pub sectors_per_cluster: u32,
    pub total_clusters: u32,
}
