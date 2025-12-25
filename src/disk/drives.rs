//! Drive Manager - DOS-style drive letters and volume management
//!
//! Manages drive letters (A:-Z:) and mounted filesystems

use super::ahci::AhciController;
use super::wfs::Wfs;
use super::fat::FatFs;
use alloc::string::String;

/// Maximum number of drives (A-Z)
pub const MAX_DRIVES: usize = 26;

/// Type of filesystem mounted on a drive
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FsType {
    None,       // No filesystem / not mounted
    Wfs,        // WATOS File System
    Fat12,
    Fat16,
    Fat32,
    Unknown,    // Has data but unrecognized format
}

/// Information about a mounted drive
#[derive(Clone, Copy)]
pub struct DriveInfo {
    pub letter: u8,           // 'A' to 'Z'
    pub fs_type: FsType,
    pub disk_port: u8,        // AHCI port number
    pub partition: u8,        // Partition number (0 = whole disk)
    pub total_sectors: u64,
    pub mounted: bool,
}

impl Default for DriveInfo {
    fn default() -> Self {
        Self {
            letter: 0,
            fs_type: FsType::None,
            disk_port: 0,
            partition: 0,
            total_sectors: 0,
            mounted: false,
        }
    }
}

/// Drive Manager - handles all mounted drives
pub struct DriveManager {
    drives: [DriveInfo; MAX_DRIVES],
    current_drive: u8,  // Current drive letter ('C', 'D', etc.)
}

impl DriveManager {
    pub const fn new() -> Self {
        const DEFAULT_DRIVE: DriveInfo = DriveInfo {
            letter: 0,
            fs_type: FsType::None,
            disk_port: 0,
            partition: 0,
            total_sectors: 0,
            mounted: false,
        };

        Self {
            drives: [DEFAULT_DRIVE; MAX_DRIVES],
            current_drive: b'C',
        }
    }

    /// Scan for disks and auto-mount them
    pub fn scan_and_mount(&mut self) -> u8 {
        let mut mounted_count = 0u8;
        let mut next_letter = b'C'; // Start from C:

        // Scan AHCI ports 0-5
        for port in 0..6u8 {
            if let Some(mut ahci) = AhciController::new_port(port) {
                if let Some(info) = ahci.identify() {
                    // Found a disk, try to detect filesystem
                    let fs_type = self.detect_filesystem(&mut ahci);

                    if next_letter <= b'Z' {
                        let idx = (next_letter - b'A') as usize;
                        self.drives[idx] = DriveInfo {
                            letter: next_letter,
                            fs_type,
                            disk_port: port,
                            partition: 0,
                            total_sectors: info.sectors,
                            mounted: true,
                        };
                        next_letter += 1;
                        mounted_count += 1;
                    }
                }
            }
        }

        mounted_count
    }

    /// Detect filesystem type on a disk
    fn detect_filesystem(&self, ahci: &mut AhciController) -> FsType {
        let mut sector = [0u8; 512];

        if !ahci.read_sectors(0, 1, &mut sector) {
            return FsType::Unknown;
        }

        // Check for WFS magic (at start of sector)
        let wfs_magic = u32::from_le_bytes([sector[0], sector[1], sector[2], sector[3]]);
        if wfs_magic == 0x57465332 || wfs_magic == 0x57465331 { // "WFS2" or "WFS1"
            return FsType::Wfs;
        }

        // Check for FAT (boot sector signature and BPB)
        if sector[510] == 0x55 && sector[511] == 0xAA {
            // Check for FAT filesystem strings
            let fat32_sig = &sector[82..90];
            let fat16_sig = &sector[54..62];

            if fat32_sig == b"FAT32   " {
                return FsType::Fat32;
            } else if fat16_sig == b"FAT16   " || fat16_sig == b"FAT12   " {
                // Determine FAT12 vs FAT16 by cluster count
                let total_sectors = if sector[19] != 0 || sector[20] != 0 {
                    u16::from_le_bytes([sector[19], sector[20]]) as u32
                } else {
                    u32::from_le_bytes([sector[32], sector[33], sector[34], sector[35]])
                };

                let sectors_per_cluster = sector[13] as u32;
                let reserved = u16::from_le_bytes([sector[14], sector[15]]) as u32;
                let num_fats = sector[16] as u32;
                let fat_size = u16::from_le_bytes([sector[22], sector[23]]) as u32;
                let root_entries = u16::from_le_bytes([sector[17], sector[18]]) as u32;
                let root_sectors = ((root_entries * 32) + 511) / 512;

                let data_sectors = total_sectors - reserved - (num_fats * fat_size) - root_sectors;
                let clusters = data_sectors / sectors_per_cluster;

                if clusters < 4085 {
                    return FsType::Fat12;
                } else {
                    return FsType::Fat16;
                }
            }
        }

        // Check if disk has any data at all
        let has_data = sector.iter().any(|&b| b != 0);
        if has_data {
            FsType::Unknown
        } else {
            FsType::None
        }
    }

    /// Get drive info by letter
    pub fn get_drive(&self, letter: u8) -> Option<&DriveInfo> {
        let letter = letter.to_ascii_uppercase();
        if letter >= b'A' && letter <= b'Z' {
            let idx = (letter - b'A') as usize;
            if self.drives[idx].mounted {
                return Some(&self.drives[idx]);
            }
        }
        None
    }

    /// Get current drive
    pub fn current_drive(&self) -> u8 {
        self.current_drive
    }

    /// Set current drive
    pub fn set_current_drive(&mut self, letter: u8) -> bool {
        let letter = letter.to_ascii_uppercase();
        if let Some(_) = self.get_drive(letter) {
            self.current_drive = letter;
            true
        } else {
            false
        }
    }

    /// List all mounted drives
    pub fn list_drives(&self) -> impl Iterator<Item = &DriveInfo> {
        self.drives.iter().filter(|d| d.mounted)
    }

    /// Format a drive with WFS
    pub fn format_wfs(&mut self, letter: u8) -> bool {
        let letter = letter.to_ascii_uppercase();
        if letter < b'A' || letter > b'Z' {
            return false;
        }

        let idx = (letter - b'A') as usize;
        let drive = &self.drives[idx];

        if !drive.mounted {
            return false;
        }

        let port = drive.disk_port;

        if let Some(ahci) = AhciController::new_port(port) {
            // Use default max files (4096)
            if let Some(_wfs) = Wfs::format(ahci, wfs_common::DEFAULT_MAX_FILES as u32) {
                // Update drive info
                self.drives[idx].fs_type = FsType::Wfs;
                return true;
            }
        }

        false
    }

    /// Get total size of a drive in bytes
    pub fn drive_size(&self, letter: u8) -> Option<u64> {
        self.get_drive(letter).map(|d| d.total_sectors * 512)
    }
}

impl DriveInfo {
    pub fn size_mb(&self) -> u64 {
        (self.total_sectors * 512) / (1024 * 1024)
    }

    pub fn fs_type_str(&self) -> &'static str {
        match self.fs_type {
            FsType::None => "None",
            FsType::Wfs => "WFS",
            FsType::Fat12 => "FAT12",
            FsType::Fat16 => "FAT16",
            FsType::Fat32 => "FAT32",
            FsType::Unknown => "Unknown",
        }
    }
}

/// Global drive manager instance
static mut DRIVE_MANAGER: DriveManager = DriveManager::new();

/// Get the global drive manager
pub fn drive_manager() -> &'static mut DriveManager {
    unsafe { &mut DRIVE_MANAGER }
}

/// Initialize drive system - scan and mount all disks
pub fn init() -> u8 {
    drive_manager().scan_and_mount()
}
