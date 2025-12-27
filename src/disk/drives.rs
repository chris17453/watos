//! Drive Manager - Flexible drive naming and volume management
//!
//! Manages named drives (can be single letters like "C" or names like "USB1")

extern crate alloc;
use super::ahci::AhciController;
use super::wfs::Wfs;
use super::vfs::{BoxedFs, create_vfs};
use alloc::string::String;
use alloc::vec::Vec;

/// Maximum number of drives
pub const MAX_DRIVES: usize = 64;

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
#[derive(Clone)]
pub struct DriveInfo {
    pub name: String,         // Drive name (e.g., "C", "USB1", "cocknballs")
    pub letter: u8,           // Legacy single letter (0 if name is longer)
    pub fs_type: FsType,
    pub disk_port: u8,        // AHCI port number
    pub partition: u8,        // Partition number (0 = whole disk/raw)
    pub start_lba: u64,       // Starting LBA of partition (0 for raw disk)
    pub total_sectors: u64,
    pub mounted: bool,
}

impl DriveInfo {
    pub fn new(name: &str) -> Self {
        let letter = if name.len() == 1 {
            name.as_bytes()[0].to_ascii_uppercase()
        } else {
            0
        };
        Self {
            name: String::from(name),
            letter,
            fs_type: FsType::None,
            disk_port: 0,
            partition: 0,
            start_lba: 0,
            total_sectors: 0,
            mounted: false,
        }
    }
}

/// Drive Manager - handles all mounted drives
pub struct DriveManager {
    drives: Vec<DriveInfo>,
    current_drive: String,  // Current drive name
}

impl DriveManager {
    pub fn new() -> Self {
        Self {
            drives: Vec::new(),
            current_drive: String::from("C"),
        }
    }

    /// Mount a drive with a specific name (raw disk, no partition)
    pub fn mount(&mut self, name: &str, port: u8, fs_type: FsType, total_sectors: u64) -> bool {
        self.mount_partition(name, port, 0, fs_type, total_sectors, 0)
    }

    /// Mount a partition with a specific name
    pub fn mount_partition(&mut self, name: &str, port: u8, partition: u8, fs_type: FsType, total_sectors: u64, start_lba: u64) -> bool {
        // Check if name already exists
        if self.get_drive_by_name(name).is_some() {
            return false;
        }

        if self.drives.len() >= MAX_DRIVES {
            return false;
        }

        let letter = if name.len() == 1 {
            name.as_bytes()[0].to_ascii_uppercase()
        } else {
            0
        };

        self.drives.push(DriveInfo {
            name: String::from(name),
            letter,
            fs_type,
            disk_port: port,
            partition,
            start_lba,
            total_sectors,
            mounted: true,
        });

        true
    }

    /// Scan for disks and auto-mount them with default letters
    pub fn scan_and_mount(&mut self) -> u8 {
        use super::partition::{PartitionTable, PartitionTableType};

        let mut mounted_count = 0u8;
        let mut next_letter = b'C'; // Start from C:

        // Scan AHCI ports 0-5
        for port in 0..6u8 {
            if let Some(mut ahci) = AhciController::new_port(port) {
                if let Some(info) = ahci.identify() {
                    // Read first sector to check for partition table
                    let mut sector = [0u8; 512];
                    if !ahci.read_sectors(0, 1, &mut sector) {
                        continue;
                    }

                    let pt = PartitionTable::parse(&sector);

                    match pt.table_type {
                        PartitionTableType::None => {
                            // Raw filesystem (no partition table)
                            let fs_type = self.detect_filesystem_at(&mut ahci, 0);
                            if next_letter <= b'Z' && fs_type != FsType::None && fs_type != FsType::Unknown {
                                let name = String::from_utf8(alloc::vec![next_letter]).unwrap_or_default();
                                if self.mount_partition(&name, port, 0, fs_type, info.sectors, 0) {
                                    next_letter += 1;
                                    mounted_count += 1;
                                }
                            }
                        }
                        PartitionTableType::Mbr => {
                            // MBR partition table - mount each partition
                            for part in &pt.partitions {
                                if next_letter > b'Z' {
                                    break;
                                }

                                // Detect filesystem on partition
                                let fs_type = self.detect_filesystem_at(&mut ahci, part.start_lba);
                                if fs_type != FsType::None && fs_type != FsType::Unknown {
                                    let name = String::from_utf8(alloc::vec![next_letter]).unwrap_or_default();
                                    if self.mount_partition(&name, port, part.index, fs_type, part.size_sectors, part.start_lba) {
                                        next_letter += 1;
                                        mounted_count += 1;
                                    }
                                }
                            }
                        }
                        PartitionTableType::Gpt => {
                            // GPT partition table - parse GPT entries
                            // GPT Header is in LBA 1
                            let mut gpt_header = [0u8; 512];
                            if !ahci.read_sectors(1, 1, &mut gpt_header) {
                                continue;
                            }
                            
                            // Verify GPT signature "EFI PART"
                            if &gpt_header[0..8] != b"EFI PART" {
                                continue;
                            }
                            
                            // Parse GPT header
                            let partition_entry_lba = u64::from_le_bytes([
                                gpt_header[72], gpt_header[73], gpt_header[74], gpt_header[75],
                                gpt_header[76], gpt_header[77], gpt_header[78], gpt_header[79],
                            ]);
                            let num_partition_entries = u32::from_le_bytes([
                                gpt_header[80], gpt_header[81], gpt_header[82], gpt_header[83],
                            ]);
                            let partition_entry_size = u32::from_le_bytes([
                                gpt_header[84], gpt_header[85], gpt_header[86], gpt_header[87],
                            ]);
                            
                            // Read partition entries (typically 128 bytes each, in LBA 2+)
                            let entries_per_sector = 512 / partition_entry_size;
                            let sectors_needed = (num_partition_entries + entries_per_sector - 1) / entries_per_sector;
                            
                            for sector_idx in 0..sectors_needed {
                                let mut sector = [0u8; 512];
                                if !ahci.read_sectors(partition_entry_lba + sector_idx as u64, 1, &mut sector) {
                                    break;
                                }
                                
                                for entry_idx in 0..entries_per_sector {
                                    if sector_idx * entries_per_sector + entry_idx >= num_partition_entries {
                                        break;
                                    }
                                    
                                    let offset = (entry_idx * partition_entry_size) as usize;
                                    if offset + 128 > 512 {
                                        break;
                                    }
                                    
                                    // Check if partition type GUID is non-zero (entry is used)
                                    let mut is_empty = true;
                                    for i in 0..16 {
                                        if sector[offset + i] != 0 {
                                            is_empty = false;
                                            break;
                                        }
                                    }
                                    
                                    if is_empty {
                                        continue;
                                    }
                                    
                                    // Parse partition entry
                                    let start_lba = u64::from_le_bytes([
                                        sector[offset + 32], sector[offset + 33], sector[offset + 34], sector[offset + 35],
                                        sector[offset + 36], sector[offset + 37], sector[offset + 38], sector[offset + 39],
                                    ]);
                                    let end_lba = u64::from_le_bytes([
                                        sector[offset + 40], sector[offset + 41], sector[offset + 42], sector[offset + 43],
                                        sector[offset + 44], sector[offset + 45], sector[offset + 46], sector[offset + 47],
                                    ]);
                                    let size_sectors = end_lba - start_lba + 1;
                                    
                                    // Detect filesystem on partition
                                    let fs_type = self.detect_filesystem_at(&mut ahci, start_lba);
                                    
                                    if next_letter <= b'Z' && fs_type != FsType::None && fs_type != FsType::Unknown {
                                        let name = String::from_utf8(alloc::vec![next_letter]).unwrap_or_default();
                                        let partition_idx = (sector_idx * entries_per_sector + entry_idx) as u8;
                                        if self.mount_partition(&name, port, partition_idx, fs_type, size_sectors, start_lba) {
                                            next_letter += 1;
                                            mounted_count += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Set current drive to first mounted drive
        if !self.drives.is_empty() {
            self.current_drive = self.drives[0].name.clone();
        }

        mounted_count
    }

    /// Detect filesystem type on a disk (at LBA 0)
    fn detect_filesystem(&self, ahci: &mut AhciController) -> FsType {
        self.detect_filesystem_at(ahci, 0)
    }

    /// Detect filesystem type at a specific LBA offset
    fn detect_filesystem_at(&self, ahci: &mut AhciController, lba: u64) -> FsType {
        let mut sector = [0u8; 512];

        if !ahci.read_sectors(lba, 1, &mut sector) {
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
                if sectors_per_cluster == 0 {
                    return FsType::Unknown;
                }
                let reserved = u16::from_le_bytes([sector[14], sector[15]]) as u32;
                let num_fats = sector[16] as u32;
                let fat_size = u16::from_le_bytes([sector[22], sector[23]]) as u32;
                let root_entries = u16::from_le_bytes([sector[17], sector[18]]) as u32;
                let root_sectors = ((root_entries * 32) + 511) / 512;

                let data_sectors = total_sectors.saturating_sub(reserved + (num_fats * fat_size) + root_sectors);
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

    /// Get drive info by name (case-insensitive)
    pub fn get_drive_by_name(&self, name: &str) -> Option<&DriveInfo> {
        let name_upper = name.to_ascii_uppercase();
        self.drives.iter().find(|d| d.mounted && d.name.to_ascii_uppercase() == name_upper)
    }

    /// Get drive info by single letter (legacy, for backwards compat)
    pub fn get_drive(&self, letter: u8) -> Option<&DriveInfo> {
        let letter = letter.to_ascii_uppercase();
        self.drives.iter().find(|d| d.mounted && d.letter == letter)
    }

    /// Get current drive name
    pub fn current_drive_name(&self) -> &str {
        &self.current_drive
    }

    /// Get current drive letter (legacy, returns first char or 'C')
    pub fn current_drive(&self) -> u8 {
        if self.current_drive.len() == 1 {
            self.current_drive.as_bytes()[0].to_ascii_uppercase()
        } else if !self.current_drive.is_empty() {
            self.current_drive.as_bytes()[0].to_ascii_uppercase()
        } else {
            b'C'
        }
    }

    /// Set current drive by name
    pub fn set_current_drive_name(&mut self, name: &str) -> bool {
        if self.get_drive_by_name(name).is_some() {
            self.current_drive = String::from(name).to_ascii_uppercase();
            true
        } else {
            false
        }
    }

    /// Set current drive by letter (legacy)
    pub fn set_current_drive(&mut self, letter: u8) -> bool {
        let letter = letter.to_ascii_uppercase();
        if self.get_drive(letter).is_some() {
            self.current_drive = String::from_utf8(alloc::vec![letter]).unwrap_or_default();
            true
        } else {
            false
        }
    }

    /// List all mounted drives
    pub fn list_drives(&self) -> impl Iterator<Item = &DriveInfo> {
        self.drives.iter().filter(|d| d.mounted)
    }

    /// Format a drive with WFS by name
    pub fn format_wfs_by_name(&mut self, name: &str) -> bool {
        let drive_info = if let Some(d) = self.get_drive_by_name(name) {
            (d.disk_port, d.mounted)
        } else {
            return false;
        };

        if !drive_info.1 {
            return false;
        }

        let port = drive_info.0;

        if let Some(ahci) = AhciController::new_port(port) {
            if let Some(_wfs) = Wfs::format(ahci, wfs_common::DEFAULT_MAX_FILES as u32) {
                // Update drive info
                if let Some(d) = self.drives.iter_mut().find(|d| d.name.to_ascii_uppercase() == name.to_ascii_uppercase()) {
                    d.fs_type = FsType::Wfs;
                }
                return true;
            }
        }

        false
    }

    /// Format a drive with WFS by letter (legacy)
    pub fn format_wfs(&mut self, letter: u8) -> bool {
        let name = String::from_utf8(alloc::vec![letter.to_ascii_uppercase()]).unwrap_or_default();
        self.format_wfs_by_name(&name)
    }

    /// Unmount a drive by name
    pub fn unmount(&mut self, name: &str) -> bool {
        let name_upper = name.to_ascii_uppercase();
        if let Some(pos) = self.drives.iter().position(|d| d.name.to_ascii_uppercase() == name_upper) {
            self.drives.remove(pos);
            // If we unmounted current drive, switch to first available
            if self.current_drive.to_ascii_uppercase() == name_upper && !self.drives.is_empty() {
                self.current_drive = self.drives[0].name.clone();
            }
            true
        } else {
            false
        }
    }

    /// Rename a drive
    pub fn rename_drive(&mut self, old_name: &str, new_name: &str) -> bool {
        let old_upper = old_name.to_ascii_uppercase();
        let _new_upper = new_name.to_ascii_uppercase();

        // Check new name doesn't exist
        if self.get_drive_by_name(new_name).is_some() {
            return false;
        }

        if let Some(drive) = self.drives.iter_mut().find(|d| d.name.to_ascii_uppercase() == old_upper) {
            drive.name = String::from(new_name);
            drive.letter = if new_name.len() == 1 {
                new_name.as_bytes()[0].to_ascii_uppercase()
            } else {
                0
            };

            // Update current drive if it was renamed
            if self.current_drive.to_ascii_uppercase() == old_upper {
                self.current_drive = String::from(new_name);
            }
            true
        } else {
            false
        }
    }

    /// Get a VFS filesystem for the named drive
    pub fn get_vfs(&self, name: &str) -> Option<BoxedFs> {
        let drive = self.get_drive_by_name(name)?;
        create_vfs(drive.fs_type, drive.disk_port, drive.start_lba)
    }

    /// Get a VFS filesystem for the current drive
    pub fn get_current_vfs(&self) -> Option<BoxedFs> {
        self.get_vfs(&self.current_drive)
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
static mut DRIVE_MANAGER: Option<DriveManager> = None;

/// Get the global drive manager
pub fn drive_manager() -> &'static mut DriveManager {
    unsafe {
        if DRIVE_MANAGER.is_none() {
            DRIVE_MANAGER = Some(DriveManager::new());
        }
        DRIVE_MANAGER.as_mut().unwrap()
    }
}

/// Initialize drive system - scan and mount all disks
pub fn init() -> u8 {
    drive_manager().scan_and_mount()
}
