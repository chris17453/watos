//! Partition table support (MBR and GPT)
//!
//! Parses partition tables to find filesystems on disks

extern crate alloc;
use alloc::vec::Vec;

/// MBR partition entry (16 bytes)
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct MbrPartition {
    pub boot_flag: u8,
    pub chs_start: [u8; 3],
    pub partition_type: u8,
    pub chs_end: [u8; 3],
    pub lba_start: u32,
    pub lba_size: u32,
}

/// Known partition types
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PartitionType {
    Empty,
    Fat12,
    Fat16Small,    // < 32MB
    Fat16Large,    // >= 32MB
    Fat32,
    Fat32Lba,
    Fat16Lba,
    LinuxNative,
    LinuxSwap,
    Ntfs,
    ExtendedChs,
    ExtendedLba,
    GptProtective,
    Unknown(u8),
}

impl From<u8> for PartitionType {
    fn from(type_byte: u8) -> Self {
        match type_byte {
            0x00 => PartitionType::Empty,
            0x01 => PartitionType::Fat12,
            0x04 => PartitionType::Fat16Small,
            0x06 => PartitionType::Fat16Large,
            0x0B => PartitionType::Fat32,
            0x0C => PartitionType::Fat32Lba,
            0x0E => PartitionType::Fat16Lba,
            0x07 => PartitionType::Ntfs,
            0x05 => PartitionType::ExtendedChs,
            0x0F => PartitionType::ExtendedLba,
            0x83 => PartitionType::LinuxNative,
            0x82 => PartitionType::LinuxSwap,
            0xEE => PartitionType::GptProtective,
            other => PartitionType::Unknown(other),
        }
    }
}

impl PartitionType {
    /// Check if this partition type is a FAT variant
    pub fn is_fat(&self) -> bool {
        matches!(
            self,
            PartitionType::Fat12
                | PartitionType::Fat16Small
                | PartitionType::Fat16Large
                | PartitionType::Fat32
                | PartitionType::Fat32Lba
                | PartitionType::Fat16Lba
        )
    }
}

/// Parsed partition information
#[derive(Debug, Clone)]
pub struct Partition {
    pub index: u8,           // Partition number (1-4 for primary)
    pub partition_type: PartitionType,
    pub bootable: bool,
    pub start_lba: u64,      // Starting sector
    pub size_sectors: u64,   // Size in sectors
}

/// Partition table type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PartitionTableType {
    None,        // No partition table (raw filesystem)
    Mbr,         // Master Boot Record
    Gpt,         // GUID Partition Table
}

/// Result of parsing a partition table
#[derive(Debug)]
pub struct PartitionTable {
    pub table_type: PartitionTableType,
    pub partitions: Vec<Partition>,
}

impl PartitionTable {
    /// Parse partition table from first sector of disk
    pub fn parse(sector: &[u8; 512]) -> Self {
        // Check for MBR signature
        if sector[510] != 0x55 || sector[511] != 0xAA {
            // No valid boot signature - might be raw filesystem
            return PartitionTable {
                table_type: PartitionTableType::None,
                partitions: Vec::new(),
            };
        }

        // Check if this is GPT (protective MBR)
        let first_entry = Self::read_mbr_entry(sector, 0);
        if first_entry.partition_type == 0xEE {
            // GPT detected - would need to read LBA 1 for GPT header
            // For now, return as GPT but don't parse partitions
            return PartitionTable {
                table_type: PartitionTableType::Gpt,
                partitions: Vec::new(),
            };
        }

        // Parse MBR partition entries
        let mut partitions = Vec::new();
        for i in 0..4 {
            let entry = Self::read_mbr_entry(sector, i);

            if entry.partition_type != 0 && entry.lba_size > 0 {
                partitions.push(Partition {
                    index: i as u8 + 1,
                    partition_type: PartitionType::from(entry.partition_type),
                    bootable: entry.boot_flag == 0x80,
                    start_lba: entry.lba_start as u64,
                    size_sectors: entry.lba_size as u64,
                });
            }
        }

        // Check if what we thought was MBR might actually be a VBR (Volume Boot Record)
        // This happens with raw FAT filesystems that have boot signature but no partition table
        if partitions.is_empty() {
            // Check for FAT filesystem signatures
            let fat16_sig = &sector[54..62];
            let fat32_sig = &sector[82..90];

            if fat32_sig == b"FAT32   " || fat16_sig.starts_with(b"FAT") {
                // This is a raw FAT filesystem, not a partition table
                return PartitionTable {
                    table_type: PartitionTableType::None,
                    partitions: Vec::new(),
                };
            }
        }

        PartitionTable {
            table_type: PartitionTableType::Mbr,
            partitions,
        }
    }

    fn read_mbr_entry(sector: &[u8], index: usize) -> MbrPartition {
        let offset = 446 + index * 16;
        MbrPartition {
            boot_flag: sector[offset],
            chs_start: [sector[offset + 1], sector[offset + 2], sector[offset + 3]],
            partition_type: sector[offset + 4],
            chs_end: [sector[offset + 5], sector[offset + 6], sector[offset + 7]],
            lba_start: u32::from_le_bytes([
                sector[offset + 8],
                sector[offset + 9],
                sector[offset + 10],
                sector[offset + 11],
            ]),
            lba_size: u32::from_le_bytes([
                sector[offset + 12],
                sector[offset + 13],
                sector[offset + 14],
                sector[offset + 15],
            ]),
        }
    }
}
