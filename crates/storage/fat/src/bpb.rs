//! BIOS Parameter Block parsing

use watos_vfs::{VfsError, VfsResult};

/// FAT type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FatType {
    Fat12,
    Fat16,
    Fat32,
}

/// BIOS Parameter Block - common to all FAT variants
#[derive(Debug, Clone)]
pub struct BiosParameterBlock {
    /// Bytes per sector (usually 512)
    pub bytes_per_sector: u16,
    /// Sectors per cluster (power of 2)
    pub sectors_per_cluster: u8,
    /// Reserved sector count (including boot sector)
    pub reserved_sector_count: u16,
    /// Number of FATs (usually 2)
    pub num_fats: u8,
    /// Root entry count (0 for FAT32)
    pub root_entry_count: u16,
    /// Total sectors (16-bit, 0 if using 32-bit field)
    pub total_sectors_16: u16,
    /// Media type
    pub media_type: u8,
    /// FAT size in sectors (16-bit, 0 for FAT32)
    pub fat_size_16: u16,
    /// Sectors per track
    pub sectors_per_track: u16,
    /// Number of heads
    pub num_heads: u16,
    /// Hidden sectors
    pub hidden_sectors: u32,
    /// Total sectors (32-bit)
    pub total_sectors_32: u32,
    /// FAT32: FAT size in sectors
    pub fat_size_32: u32,
    /// FAT32: Root cluster
    pub root_cluster: u32,
    /// FAT32: FSInfo sector
    pub fs_info_sector: u16,
    /// FAT32: Backup boot sector
    pub backup_boot_sector: u16,
    /// Volume label
    pub volume_label: [u8; 11],
    /// Filesystem type string
    pub fs_type: [u8; 8],
}

impl BiosParameterBlock {
    /// Parse BPB from boot sector
    pub fn parse(boot_sector: &[u8]) -> VfsResult<Self> {
        if boot_sector.len() < 512 {
            return Err(VfsError::InvalidArgument);
        }

        // Check boot signature
        if boot_sector[510] != 0x55 || boot_sector[511] != 0xAA {
            return Err(VfsError::InvalidArgument);
        }

        let bytes_per_sector = u16::from_le_bytes([boot_sector[11], boot_sector[12]]);
        let sectors_per_cluster = boot_sector[13];
        let reserved_sector_count = u16::from_le_bytes([boot_sector[14], boot_sector[15]]);
        let num_fats = boot_sector[16];
        let root_entry_count = u16::from_le_bytes([boot_sector[17], boot_sector[18]]);
        let total_sectors_16 = u16::from_le_bytes([boot_sector[19], boot_sector[20]]);
        let media_type = boot_sector[21];
        let fat_size_16 = u16::from_le_bytes([boot_sector[22], boot_sector[23]]);
        let sectors_per_track = u16::from_le_bytes([boot_sector[24], boot_sector[25]]);
        let num_heads = u16::from_le_bytes([boot_sector[26], boot_sector[27]]);
        let hidden_sectors = u32::from_le_bytes([
            boot_sector[28],
            boot_sector[29],
            boot_sector[30],
            boot_sector[31],
        ]);
        let total_sectors_32 = u32::from_le_bytes([
            boot_sector[32],
            boot_sector[33],
            boot_sector[34],
            boot_sector[35],
        ]);

        // FAT32 extended fields
        let (fat_size_32, root_cluster, fs_info_sector, backup_boot_sector, volume_label, fs_type) =
            if fat_size_16 == 0 {
                // FAT32
                let fat_size_32 = u32::from_le_bytes([
                    boot_sector[36],
                    boot_sector[37],
                    boot_sector[38],
                    boot_sector[39],
                ]);
                let root_cluster = u32::from_le_bytes([
                    boot_sector[44],
                    boot_sector[45],
                    boot_sector[46],
                    boot_sector[47],
                ]);
                let fs_info_sector = u16::from_le_bytes([boot_sector[48], boot_sector[49]]);
                let backup_boot_sector = u16::from_le_bytes([boot_sector[50], boot_sector[51]]);

                let mut volume_label = [0u8; 11];
                volume_label.copy_from_slice(&boot_sector[71..82]);

                let mut fs_type = [0u8; 8];
                fs_type.copy_from_slice(&boot_sector[82..90]);

                (
                    fat_size_32,
                    root_cluster,
                    fs_info_sector,
                    backup_boot_sector,
                    volume_label,
                    fs_type,
                )
            } else {
                // FAT12/16
                let mut volume_label = [0u8; 11];
                volume_label.copy_from_slice(&boot_sector[43..54]);

                let mut fs_type = [0u8; 8];
                fs_type.copy_from_slice(&boot_sector[54..62]);

                (0, 0, 0, 0, volume_label, fs_type)
            };

        // Validate basic fields
        if bytes_per_sector == 0
            || sectors_per_cluster == 0
            || num_fats == 0
            || reserved_sector_count == 0
        {
            return Err(VfsError::InvalidArgument);
        }

        Ok(BiosParameterBlock {
            bytes_per_sector,
            sectors_per_cluster,
            reserved_sector_count,
            num_fats,
            root_entry_count,
            total_sectors_16,
            media_type,
            fat_size_16,
            sectors_per_track,
            num_heads,
            hidden_sectors,
            total_sectors_32,
            fat_size_32,
            root_cluster,
            fs_info_sector,
            backup_boot_sector,
            volume_label,
            fs_type,
        })
    }

    /// Determine FAT type based on BPB fields and cluster count
    pub fn fat_type(&self) -> FatType {
        // FAT32 is definitively indicated by fat_size_16 == 0
        // This is more reliable than cluster count alone
        if self.fat_size_16 == 0 {
            return FatType::Fat32;
        }

        // For FAT12/16, use cluster count to distinguish
        let root_dir_sectors = ((self.root_entry_count as u32 * 32)
            + (self.bytes_per_sector as u32 - 1))
            / self.bytes_per_sector as u32;

        let fat_size = self.fat_size_16 as u32;

        let total_sectors = if self.total_sectors_16 != 0 {
            self.total_sectors_16 as u32
        } else {
            self.total_sectors_32
        };

        let data_sectors = total_sectors
            - self.reserved_sector_count as u32
            - (self.num_fats as u32 * fat_size)
            - root_dir_sectors;

        let cluster_count = data_sectors / self.sectors_per_cluster as u32;

        if cluster_count < 4085 {
            FatType::Fat12
        } else {
            FatType::Fat16
        }
    }

    /// Get volume label as string
    pub fn volume_label_str(&self) -> &str {
        let end = self
            .volume_label
            .iter()
            .rposition(|&c| c != 0x20 && c != 0)
            .map(|i| i + 1)
            .unwrap_or(0);

        core::str::from_utf8(&self.volume_label[..end]).unwrap_or("")
    }
}
