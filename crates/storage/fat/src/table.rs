//! FAT table operations

use watos_vfs::{VfsError, VfsResult};
use watos_driver_framework::block::BlockDevice;

use crate::bpb::{BiosParameterBlock, FatType};

/// End of cluster chain marker
pub const EOC_FAT12: u32 = 0x0FF8;
pub const EOC_FAT16: u32 = 0xFFF8;
pub const EOC_FAT32: u32 = 0x0FFFFFF8;

/// Bad cluster marker
pub const BAD_FAT12: u32 = 0x0FF7;
pub const BAD_FAT16: u32 = 0xFFF7;
pub const BAD_FAT32: u32 = 0x0FFFFFF7;

/// Free cluster marker
pub const FREE_CLUSTER: u32 = 0x00000000;

/// Read a FAT entry
pub fn read_fat_entry<D: BlockDevice>(
    device: &mut D,
    bpb: &BiosParameterBlock,
    fat_type: FatType,
    cluster: u32,
) -> VfsResult<Option<u32>> {
    let fat_start = bpb.reserved_sector_count as u64;
    let bytes_per_sector = bpb.bytes_per_sector as u64;

    match fat_type {
        FatType::Fat12 => read_fat12_entry(device, fat_start, bytes_per_sector, cluster),
        FatType::Fat16 => read_fat16_entry(device, fat_start, bytes_per_sector, cluster),
        FatType::Fat32 => read_fat32_entry(device, fat_start, bytes_per_sector, cluster),
    }
}

/// Read FAT12 entry (12-bit entries, tricky byte alignment)
fn read_fat12_entry<D: BlockDevice>(
    device: &mut D,
    fat_start: u64,
    bytes_per_sector: u64,
    cluster: u32,
) -> VfsResult<Option<u32>> {
    // FAT12 entries are 1.5 bytes each
    let fat_offset = cluster + (cluster / 2);
    let sector = fat_start + (fat_offset as u64 / bytes_per_sector);
    let offset_in_sector = (fat_offset as usize) % (bytes_per_sector as usize);

    let mut sector_buf = [0u8; 512];
    device
        .read_sectors(sector, &mut sector_buf)
        .map_err(|_| VfsError::IoError)?;

    let value = if offset_in_sector == 511 {
        // Entry spans two sectors
        let mut next_sector_buf = [0u8; 512];
        device
            .read_sectors(sector + 1, &mut next_sector_buf)
            .map_err(|_| VfsError::IoError)?;

        let low = sector_buf[offset_in_sector] as u32;
        let high = next_sector_buf[0] as u32;
        if cluster & 1 != 0 {
            (low >> 4) | (high << 4)
        } else {
            low | ((high & 0x0F) << 8)
        }
    } else {
        let low = sector_buf[offset_in_sector] as u32;
        let high = sector_buf[offset_in_sector + 1] as u32;
        if cluster & 1 != 0 {
            (low >> 4) | (high << 4)
        } else {
            low | ((high & 0x0F) << 8)
        }
    };

    if value >= EOC_FAT12 {
        Ok(None) // End of chain
    } else if value == FREE_CLUSTER || value == BAD_FAT12 {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

/// Read FAT16 entry (16-bit entries)
fn read_fat16_entry<D: BlockDevice>(
    device: &mut D,
    fat_start: u64,
    bytes_per_sector: u64,
    cluster: u32,
) -> VfsResult<Option<u32>> {
    let fat_offset = (cluster * 2) as u64;
    let sector = fat_start + (fat_offset / bytes_per_sector);
    let offset_in_sector = (fat_offset % bytes_per_sector) as usize;

    let mut sector_buf = [0u8; 512];
    device
        .read_sectors(sector, &mut sector_buf)
        .map_err(|_| VfsError::IoError)?;

    let value = u16::from_le_bytes([
        sector_buf[offset_in_sector],
        sector_buf[offset_in_sector + 1],
    ]) as u32;

    if value >= EOC_FAT16 {
        Ok(None)
    } else if value == FREE_CLUSTER || value == BAD_FAT16 {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

/// Read FAT32 entry (28-bit entries, top 4 bits reserved)
fn read_fat32_entry<D: BlockDevice>(
    device: &mut D,
    fat_start: u64,
    bytes_per_sector: u64,
    cluster: u32,
) -> VfsResult<Option<u32>> {
    let fat_offset = (cluster * 4) as u64;
    let sector = fat_start + (fat_offset / bytes_per_sector);
    let offset_in_sector = (fat_offset % bytes_per_sector) as usize;

    let mut sector_buf = [0u8; 512];
    device
        .read_sectors(sector, &mut sector_buf)
        .map_err(|_| VfsError::IoError)?;

    let value = u32::from_le_bytes([
        sector_buf[offset_in_sector],
        sector_buf[offset_in_sector + 1],
        sector_buf[offset_in_sector + 2],
        sector_buf[offset_in_sector + 3],
    ]) & 0x0FFFFFFF; // Mask off reserved bits

    if value >= EOC_FAT32 {
        Ok(None)
    } else if value == FREE_CLUSTER || value == BAD_FAT32 {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

/// Check if a cluster value indicates end of chain
pub fn is_end_of_chain(fat_type: FatType, cluster: u32) -> bool {
    match fat_type {
        FatType::Fat12 => cluster >= EOC_FAT12,
        FatType::Fat16 => cluster >= EOC_FAT16,
        FatType::Fat32 => cluster >= EOC_FAT32,
    }
}

/// Check if a cluster is free
pub fn is_free(cluster: u32) -> bool {
    cluster == FREE_CLUSTER
}

/// Check if a cluster is marked as bad
pub fn is_bad(fat_type: FatType, cluster: u32) -> bool {
    match fat_type {
        FatType::Fat12 => cluster == BAD_FAT12,
        FatType::Fat16 => cluster == BAD_FAT16,
        FatType::Fat32 => cluster == BAD_FAT32,
    }
}
