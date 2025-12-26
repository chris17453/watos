//! Virtual File System (VFS) - Abstract interface for all filesystems
//!
//! Provides a unified interface so shell commands work with any filesystem.
//! Inspired by Linux's VFS design.

extern crate alloc;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use super::ahci::AhciController;
use super::wfs::{Wfs, MountResult as WfsMountResult};
use super::fat::{FatFs, FatType, ATTR_READ_ONLY, ATTR_HIDDEN, ATTR_SYSTEM, ATTR_ARCHIVE};
use wfs_common::FLAG_EXEC;

/// File type (like Linux's mode_t file type bits)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileType {
    Regular,
    Directory,
    Device,
    Symlink,
}

/// File attributes/flags
#[derive(Debug, Clone, Copy, Default)]
pub struct FileAttr {
    pub readonly: bool,
    pub hidden: bool,
    pub system: bool,
    pub archive: bool,
    pub executable: bool,
}

/// Directory entry - returned when listing directories
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub file_type: FileType,
    pub size: u64,
    pub attr: FileAttr,
    /// Modification date (DOS format: bits 15-9=year-1980, 8-5=month, 4-0=day)
    pub mdate: u16,
    /// Modification time (DOS format: bits 15-11=hour, 10-5=min, 4-0=sec/2)
    pub mtime: u16,
}

impl DirEntry {
    pub fn new(name: &str, file_type: FileType, size: u64) -> Self {
        Self {
            name: String::from(name),
            file_type,
            size,
            attr: FileAttr::default(),
            mdate: 0,
            mtime: 0,
        }
    }

    pub fn is_dir(&self) -> bool {
        self.file_type == FileType::Directory
    }

    /// Get year from DOS date
    pub fn year(&self) -> u16 {
        1980 + ((self.mdate >> 9) & 0x7F)
    }

    /// Get month from DOS date (1-12)
    pub fn month(&self) -> u8 {
        ((self.mdate >> 5) & 0x0F) as u8
    }

    /// Get day from DOS date (1-31)
    pub fn day(&self) -> u8 {
        (self.mdate & 0x1F) as u8
    }

    /// Get hour from DOS time
    pub fn hour(&self) -> u8 {
        ((self.mtime >> 11) & 0x1F) as u8
    }

    /// Get minute from DOS time
    pub fn minute(&self) -> u8 {
        ((self.mtime >> 5) & 0x3F) as u8
    }
}

/// Filesystem information
#[derive(Debug, Clone)]
pub struct FsInfo {
    pub fs_type: &'static str,  // "WFS", "FAT16", etc.
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub block_size: u32,
}

/// Error types for filesystem operations
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FsError {
    NotFound,
    PermissionDenied,
    NotADirectory,
    NotAFile,
    IoError,
    NoSpace,
    InvalidName,
    AlreadyExists,
    NotSupported,
    Corrupted,
}

pub type FsResult<T> = Result<T, FsError>;

/// The main VFS trait - all filesystems implement this
pub trait FileSystem: Send {
    /// Get filesystem type name
    fn fs_type(&self) -> &'static str;

    /// Get filesystem info (total space, free space, etc.)
    fn info(&self) -> FsInfo;

    /// List directory contents
    /// path: "/" for root, "/subdir" for subdirectories
    fn read_dir(&mut self, path: &str) -> FsResult<Vec<DirEntry>>;

    /// Read entire file into memory
    fn read_file(&mut self, path: &str) -> FsResult<Vec<u8>>;

    /// Write file (create or overwrite)
    fn write_file(&mut self, path: &str, data: &[u8]) -> FsResult<()>;

    /// Delete a file
    fn delete(&mut self, path: &str) -> FsResult<()>;

    /// Create a directory
    fn mkdir(&mut self, path: &str) -> FsResult<()>;

    /// Check if path exists
    fn exists(&mut self, path: &str) -> bool;

    /// Get info about a specific file/directory
    fn stat(&mut self, path: &str) -> FsResult<DirEntry>;
}

/// Boxed filesystem for dynamic dispatch
pub type BoxedFs = Box<dyn FileSystem>;

// ============================================================================
// VFS Wrapper for WFS
// ============================================================================

/// VFS wrapper around WFS filesystem
pub struct WfsVfs {
    wfs: Wfs,
}

impl WfsVfs {
    /// Mount a WFS filesystem at the given port and LBA offset
    pub fn mount(port: u8, start_lba: u64) -> Option<Self> {
        let ahci = AhciController::new_port_at(port, start_lba)?;
        match Wfs::try_mount(ahci) {
            WfsMountResult::Ok(wfs) => Some(Self { wfs }),
            _ => None,
        }
    }
}

impl FileSystem for WfsVfs {
    fn fs_type(&self) -> &'static str {
        "WFS"
    }

    fn info(&self) -> FsInfo {
        let info = self.wfs.info();
        FsInfo {
            fs_type: "WFS",
            total_bytes: info.total_blocks * info.block_size as u64,
            free_bytes: info.free_blocks * info.block_size as u64,
            block_size: info.block_size,
        }
    }

    fn read_dir(&mut self, path: &str) -> FsResult<Vec<DirEntry>> {
        // WFS v2 only supports root directory
        if path != "/" && path != "" && path != "\\" {
            return Err(FsError::NotSupported);
        }

        let mut entries = Vec::new();
        for file in self.wfs.list_files() {
            let file_type = if file.is_directory() {
                FileType::Directory
            } else {
                FileType::Regular
            };

            let mut entry = DirEntry::new(file.name_str(), file_type, file.size);
            entry.attr.executable = (file.flags & FLAG_EXEC) != 0;
            // WFS uses u64 timestamps, convert to DOS date/time if needed
            // For now, just use 0 (no date stored in DOS format)
            entries.push(entry);
        }
        Ok(entries)
    }

    fn read_file(&mut self, path: &str) -> FsResult<Vec<u8>> {
        // Strip leading slash
        let name = path.trim_start_matches('/').trim_start_matches('\\');

        let entry = self.wfs.find_file(name).ok_or(FsError::NotFound)?;

        let mut buffer = Vec::new();
        buffer.resize(entry.size as usize, 0);

        self.wfs.read_file(&entry, &mut buffer)
            .ok_or(FsError::IoError)?;

        Ok(buffer)
    }

    fn write_file(&mut self, path: &str, data: &[u8]) -> FsResult<()> {
        let name = path.trim_start_matches('/').trim_start_matches('\\');

        if self.wfs.write_file(name, data, 0) {
            Ok(())
        } else {
            Err(FsError::IoError)
        }
    }

    fn delete(&mut self, _path: &str) -> FsResult<()> {
        // WFS v2 doesn't support deletion yet
        Err(FsError::NotSupported)
    }

    fn mkdir(&mut self, _path: &str) -> FsResult<()> {
        // WFS v2 doesn't support subdirectories yet
        Err(FsError::NotSupported)
    }

    fn exists(&mut self, path: &str) -> bool {
        let name = path.trim_start_matches('/').trim_start_matches('\\');
        if name.is_empty() {
            return true; // Root always exists
        }
        self.wfs.find_file(name).is_some()
    }

    fn stat(&mut self, path: &str) -> FsResult<DirEntry> {
        let name = path.trim_start_matches('/').trim_start_matches('\\');

        if name.is_empty() {
            // Root directory
            return Ok(DirEntry::new("/", FileType::Directory, 0));
        }

        let file = self.wfs.find_file(name).ok_or(FsError::NotFound)?;
        let file_type = if file.is_directory() {
            FileType::Directory
        } else {
            FileType::Regular
        };

        let mut entry = DirEntry::new(file.name_str(), file_type, file.size);
        entry.attr.executable = (file.flags & FLAG_EXEC) != 0;
        // WFS uses u64 timestamps, not DOS format
        Ok(entry)
    }
}

// ============================================================================
// VFS Wrapper for FAT
// ============================================================================

/// VFS wrapper around FAT filesystem
pub struct FatVfs {
    fat: FatFs,
    fat_type_name: &'static str,
}

impl FatVfs {
    /// Mount a FAT filesystem at the given port and LBA offset
    pub fn mount(port: u8, start_lba: u64) -> Option<Self> {
        let ahci = AhciController::new_port(port)?;
        let fat = FatFs::mount(ahci, start_lba)?;

        let fat_type_name = match fat.info().fat_type {
            FatType::Fat12 => "FAT12",
            FatType::Fat16 => "FAT16",
            FatType::Fat32 => "FAT32",
        };

        Some(Self { fat, fat_type_name })
    }
}

impl FileSystem for FatVfs {
    fn fs_type(&self) -> &'static str {
        self.fat_type_name
    }

    fn info(&self) -> FsInfo {
        let info = self.fat.info();
        let bytes_per_cluster = info.bytes_per_sector * info.sectors_per_cluster;
        FsInfo {
            fs_type: self.fat_type_name,
            total_bytes: info.total_clusters as u64 * bytes_per_cluster as u64,
            free_bytes: 0, // FAT read-only for now, no free space tracking
            block_size: bytes_per_cluster,
        }
    }

    fn read_dir(&mut self, path: &str) -> FsResult<Vec<DirEntry>> {
        // FAT currently only supports root directory
        if path != "/" && path != "" && path != "\\" {
            return Err(FsError::NotSupported);
        }

        let fat_entries = self.fat.list_root();
        let mut entries = Vec::new();

        for fe in fat_entries {
            let file_type = if fe.is_directory() {
                FileType::Directory
            } else {
                FileType::Regular
            };

            let mut entry = DirEntry::new(&fe.short_name(), file_type, fe.file_size as u64);
            entry.attr.readonly = (fe.attr & ATTR_READ_ONLY) != 0;
            entry.attr.hidden = (fe.attr & ATTR_HIDDEN) != 0;
            entry.attr.system = (fe.attr & ATTR_SYSTEM) != 0;
            entry.attr.archive = (fe.attr & ATTR_ARCHIVE) != 0;
            entry.mdate = fe.write_date;
            entry.mtime = fe.write_time;
            entries.push(entry);
        }

        Ok(entries)
    }

    fn read_file(&mut self, path: &str) -> FsResult<Vec<u8>> {
        let name = path.trim_start_matches('/').trim_start_matches('\\');

        let entry = self.fat.find_file(name).ok_or(FsError::NotFound)?;

        if entry.is_directory() {
            return Err(FsError::NotAFile);
        }

        self.fat.read_file(&entry).ok_or(FsError::IoError)
    }

    fn write_file(&mut self, _path: &str, _data: &[u8]) -> FsResult<()> {
        // FAT is read-only for now
        Err(FsError::NotSupported)
    }

    fn delete(&mut self, _path: &str) -> FsResult<()> {
        Err(FsError::NotSupported)
    }

    fn mkdir(&mut self, _path: &str) -> FsResult<()> {
        Err(FsError::NotSupported)
    }

    fn exists(&mut self, path: &str) -> bool {
        let name = path.trim_start_matches('/').trim_start_matches('\\');
        if name.is_empty() {
            return true; // Root always exists
        }
        self.fat.find_file(name).is_some()
    }

    fn stat(&mut self, path: &str) -> FsResult<DirEntry> {
        let name = path.trim_start_matches('/').trim_start_matches('\\');

        if name.is_empty() {
            return Ok(DirEntry::new("/", FileType::Directory, 0));
        }

        let fe = self.fat.find_file(name).ok_or(FsError::NotFound)?;

        let file_type = if fe.is_directory() {
            FileType::Directory
        } else {
            FileType::Regular
        };

        let mut entry = DirEntry::new(&fe.short_name(), file_type, fe.file_size as u64);
        entry.attr.readonly = (fe.attr & ATTR_READ_ONLY) != 0;
        entry.attr.hidden = (fe.attr & ATTR_HIDDEN) != 0;
        entry.attr.system = (fe.attr & ATTR_SYSTEM) != 0;
        entry.attr.archive = (fe.attr & ATTR_ARCHIVE) != 0;
        entry.mdate = fe.write_date;
        entry.mtime = fe.write_time;
        Ok(entry)
    }
}

// ============================================================================
// Factory function to create VFS from drive info
// ============================================================================

use super::drives::FsType;

/// Create a VFS filesystem for a drive
pub fn create_vfs(fs_type: FsType, port: u8, start_lba: u64) -> Option<BoxedFs> {
    match fs_type {
        FsType::Wfs => {
            WfsVfs::mount(port, start_lba).map(|v| Box::new(v) as BoxedFs)
        }
        FsType::Fat12 | FsType::Fat16 | FsType::Fat32 => {
            FatVfs::mount(port, start_lba).map(|v| Box::new(v) as BoxedFs)
        }
        _ => None,
    }
}
