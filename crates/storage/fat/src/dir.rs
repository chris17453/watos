//! FAT directory entry handling

use alloc::string::String;
use watos_vfs::{DirEntry, FileType};

use crate::bpb::{BiosParameterBlock, FatType};

/// FAT directory entry attributes
pub mod attrs {
    pub const READ_ONLY: u8 = 0x01;
    pub const HIDDEN: u8 = 0x02;
    pub const SYSTEM: u8 = 0x04;
    pub const VOLUME_ID: u8 = 0x08;
    pub const DIRECTORY: u8 = 0x10;
    pub const ARCHIVE: u8 = 0x20;
    pub const LONG_NAME: u8 = READ_ONLY | HIDDEN | SYSTEM | VOLUME_ID;
}

/// FAT directory entry (32 bytes)
#[derive(Debug, Clone)]
pub struct FatDirEntry {
    /// 8.3 filename
    pub name: [u8; 11],
    /// File attributes
    pub attributes: u8,
    /// Reserved
    pub nt_reserved: u8,
    /// Creation time tenths
    pub creation_time_tenths: u8,
    /// Creation time
    pub creation_time: u16,
    /// Creation date
    pub creation_date: u16,
    /// Last access date
    pub last_access_date: u16,
    /// High 16 bits of first cluster (FAT32)
    pub first_cluster_high: u16,
    /// Last modification time
    pub modification_time: u16,
    /// Last modification date
    pub modification_date: u16,
    /// Low 16 bits of first cluster
    pub first_cluster_low: u16,
    /// File size in bytes
    pub file_size: u32,
}

impl FatDirEntry {
    /// Parse directory entry from 32-byte buffer
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 32 {
            return None;
        }

        // Check for end of directory
        if data[0] == 0x00 {
            return None;
        }

        // Check for deleted entry
        if data[0] == 0xE5 {
            return None;
        }

        let mut name = [0u8; 11];
        name.copy_from_slice(&data[0..11]);

        Some(FatDirEntry {
            name,
            attributes: data[11],
            nt_reserved: data[12],
            creation_time_tenths: data[13],
            creation_time: u16::from_le_bytes([data[14], data[15]]),
            creation_date: u16::from_le_bytes([data[16], data[17]]),
            last_access_date: u16::from_le_bytes([data[18], data[19]]),
            first_cluster_high: u16::from_le_bytes([data[20], data[21]]),
            modification_time: u16::from_le_bytes([data[22], data[23]]),
            modification_date: u16::from_le_bytes([data[24], data[25]]),
            first_cluster_low: u16::from_le_bytes([data[26], data[27]]),
            file_size: u32::from_le_bytes([data[28], data[29], data[30], data[31]]),
        })
    }

    /// Create a pseudo-entry for the root directory
    pub fn root_dir(bpb: &BiosParameterBlock, fat_type: FatType) -> Self {
        FatDirEntry {
            name: *b"           ",
            attributes: attrs::DIRECTORY,
            nt_reserved: 0,
            creation_time_tenths: 0,
            creation_time: 0,
            creation_date: 0,
            last_access_date: 0,
            first_cluster_high: if fat_type == FatType::Fat32 {
                (bpb.root_cluster >> 16) as u16
            } else {
                0
            },
            modification_time: 0,
            modification_date: 0,
            first_cluster_low: if fat_type == FatType::Fat32 {
                bpb.root_cluster as u16
            } else {
                0
            },
            file_size: 0,
        }
    }

    /// Check if this is a directory
    pub fn is_directory(&self) -> bool {
        self.attributes & attrs::DIRECTORY != 0
    }

    /// Check if this is a volume label
    pub fn is_volume_label(&self) -> bool {
        self.attributes & attrs::VOLUME_ID != 0
    }

    /// Check if this is a long filename entry
    pub fn is_long_name(&self) -> bool {
        self.attributes & attrs::LONG_NAME == attrs::LONG_NAME
    }

    /// Check if this is a hidden entry
    pub fn is_hidden(&self) -> bool {
        self.attributes & attrs::HIDDEN != 0
    }

    /// Get the first cluster number
    pub fn first_cluster(&self) -> u32 {
        ((self.first_cluster_high as u32) << 16) | (self.first_cluster_low as u32)
    }

    /// Get the 8.3 filename as a string
    pub fn short_name(&self) -> String {
        let name_part = &self.name[0..8];
        let ext_part = &self.name[8..11];

        // Trim trailing spaces from name
        let name_end = name_part
            .iter()
            .rposition(|&c| c != 0x20)
            .map(|i| i + 1)
            .unwrap_or(0);

        // Trim trailing spaces from extension
        let ext_end = ext_part
            .iter()
            .rposition(|&c| c != 0x20)
            .map(|i| i + 1)
            .unwrap_or(0);

        let mut result = String::new();

        // Handle special first character (0x05 means 0xE5)
        if name_end > 0 {
            let first = if name_part[0] == 0x05 {
                0xE5
            } else {
                name_part[0]
            };
            result.push(first as char);
            for &c in &name_part[1..name_end] {
                result.push(c as char);
            }
        }

        if ext_end > 0 {
            result.push('.');
            for &c in &ext_part[0..ext_end] {
                result.push(c as char);
            }
        }

        result
    }

    /// Check if the entry matches a given name (case-insensitive)
    pub fn matches_name(&self, name: &str) -> bool {
        if self.is_volume_label() || self.is_long_name() {
            return false;
        }

        let short = self.short_name();
        short.eq_ignore_ascii_case(name)
    }

    /// Get file type for VFS
    pub fn file_type(&self) -> FileType {
        if self.is_directory() {
            FileType::Directory
        } else {
            FileType::Regular
        }
    }

    /// Convert to VFS DirEntry
    pub fn to_vfs_entry(&self) -> Option<DirEntry> {
        // Skip volume labels, LFN entries, and special entries
        if self.is_volume_label() || self.is_long_name() {
            return None;
        }

        let name = self.short_name();
        if name == "." || name == ".." {
            return None; // Skip . and .. entries
        }

        Some(DirEntry {
            name,
            file_type: self.file_type(),
            size: self.file_size as u64,
            inode: self.first_cluster() as u64,
        })
    }
}

/// Iterator over directory entries in a buffer
pub struct DirEntryIterator<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> DirEntryIterator<'a> {
    /// Create a new iterator over directory entries
    pub fn new(data: &'a [u8]) -> Self {
        DirEntryIterator { data, offset: 0 }
    }
}

impl<'a> Iterator for DirEntryIterator<'a> {
    type Item = FatDirEntry;

    fn next(&mut self) -> Option<Self::Item> {
        while self.offset + 32 <= self.data.len() {
            let entry_data = &self.data[self.offset..self.offset + 32];
            self.offset += 32;

            // End of directory marker
            if entry_data[0] == 0x00 {
                return None;
            }

            // Try to parse (skips deleted entries)
            if let Some(entry) = FatDirEntry::parse(entry_data) {
                // Skip LFN entries for now
                if !entry.is_long_name() {
                    return Some(entry);
                }
            }
        }

        None
    }
}
