//! WATOS Virtual File System
//!
//! Provides a unified interface for filesystem operations across
//! different filesystem implementations (WFS, FAT, etc).
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────────────────────────────┐
//! │           User Applications          │
//! └──────────────────┬───────────────────┘
//!                    │ open/read/write/close
//! ┌──────────────────▼───────────────────┐
//! │              VFS Layer               │
//! │  - Mount table                       │
//! │  - Path resolution                   │
//! │  - File handle management            │
//! └──────────────────┬───────────────────┘
//!                    │ Filesystem trait
//! ┌─────────┬────────┴────────┬──────────┐
//! │   WFS   │       FAT       │   ...    │
//! └─────────┴─────────────────┴──────────┘
//! ```

#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::vec;
use spin::Mutex;

pub mod path;
pub mod file;
pub mod mount;
pub mod error;
pub mod pipe;
pub mod symlink;
pub mod metadata;

// Re-export universal path utilities for new code
// TODO: Migrate VFS path module to use watos-path completely
pub use watos_path as core_path;

pub use error::{VfsError, VfsResult};
pub use file::{FileHandle, FileMode, FileType, FileStat};
pub use mount::{MountPoint, MountTable, DriveMount, MAX_DRIVES};
pub use path::{Path, PathType, ParsedPath, parse as parse_path, is_drive_letter};
pub use pipe::{create_pipe, create_pipe_with_capacity, NamedPipe, PIPE_BUF_SIZE};
pub use symlink::{SymlinkFilesystem, SymlinkTarget, SymlinkResolver, ResolvedPath, ResolveOptions, MAX_SYMLINK_DEPTH};
pub use metadata::{ExtendedMetadata, ExtendedMetadataFs, FileColor, FileIcon, icon_from_extension, color_from_file};

/// Maximum path length
pub const MAX_PATH: usize = 256;

/// Maximum filename length
pub const MAX_FILENAME: usize = 255;

/// Maximum number of open files per process
pub const MAX_OPEN_FILES: usize = 64;

/// Maximum number of mount points
pub const MAX_MOUNTS: usize = 16;

/// Directory entry
#[derive(Debug, Clone)]
pub struct DirEntry {
    /// Entry name
    pub name: String,
    /// Entry type
    pub file_type: FileType,
    /// File size in bytes
    pub size: u64,
    /// Inode number (filesystem-specific)
    pub inode: u64,
}

/// Filesystem trait - must be implemented by all filesystem drivers
pub trait Filesystem: Send + Sync {
    /// Get filesystem name
    fn name(&self) -> &'static str;

    /// Open a file
    fn open(&self, path: &str, mode: FileMode) -> VfsResult<Box<dyn FileOperations>>;

    /// Get file statistics
    fn stat(&self, path: &str) -> VfsResult<FileStat>;

    /// Create a directory
    fn mkdir(&self, path: &str) -> VfsResult<()>;

    /// Remove a file
    fn unlink(&self, path: &str) -> VfsResult<()>;

    /// Remove a directory
    fn rmdir(&self, path: &str) -> VfsResult<()>;

    /// Read directory entries
    fn readdir(&self, path: &str) -> VfsResult<Vec<DirEntry>>;

    /// Rename/move a file
    fn rename(&self, old_path: &str, new_path: &str) -> VfsResult<()>;

    /// Sync filesystem to disk
    fn sync(&self) -> VfsResult<()>;

    /// Get filesystem statistics
    fn statfs(&self) -> VfsResult<FsStats>;

    // Compatibility methods for legacy code

    /// Check if a file exists
    fn exists(&self, path: &str) -> bool {
        self.stat(path).is_ok()
    }

    /// Read entire file contents (convenience method)
    fn read_file(&self, path: &str) -> VfsResult<Vec<u8>> {
        let mut file = self.open(path, FileMode::READ)?;
        let stat = file.stat()?;
        let mut buffer = vec![0u8; stat.size as usize];
        file.read(&mut buffer)?;
        Ok(buffer)
    }

    /// Write entire file contents (convenience method)
    fn write_file(&self, path: &str, data: &[u8]) -> VfsResult<()> {
        let mut file = self.open(path, FileMode::WRITE)?;
        file.write(data)?;
        Ok(())
    }

    /// Read directory (alias for readdir)
    fn read_dir(&self, path: &str) -> VfsResult<Vec<DirEntry>> {
        self.readdir(path)
    }
}

/// File operations trait - returned by Filesystem::open
pub trait FileOperations: Send + Sync {
    /// Read data from file
    fn read(&mut self, buffer: &mut [u8]) -> VfsResult<usize>;

    /// Write data to file
    fn write(&mut self, buffer: &[u8]) -> VfsResult<usize>;

    /// Seek to position
    fn seek(&mut self, offset: i64, whence: SeekFrom) -> VfsResult<u64>;

    /// Get current position
    fn tell(&self) -> u64;

    /// Sync file to disk
    fn sync(&mut self) -> VfsResult<()>;

    /// Get file statistics
    fn stat(&self) -> VfsResult<FileStat>;

    /// Truncate file to size
    fn truncate(&mut self, size: u64) -> VfsResult<()>;
}

/// Seek origin
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeekFrom {
    /// From start of file
    Start,
    /// From current position
    Current,
    /// From end of file
    End,
}

/// Filesystem statistics
#[derive(Debug, Clone, Copy)]
pub struct FsStats {
    /// Total blocks
    pub total_blocks: u64,
    /// Free blocks
    pub free_blocks: u64,
    /// Block size
    pub block_size: u32,
    /// Total inodes
    pub total_inodes: u64,
    /// Free inodes
    pub free_inodes: u64,
    /// Maximum filename length
    pub max_name_len: u32,
}

/// Global VFS instance
static VFS: Mutex<Option<Vfs>> = Mutex::new(None);

/// Virtual File System manager
pub struct Vfs {
    mounts: MountTable,
}

impl Vfs {
    /// Create a new VFS instance
    pub fn new() -> Self {
        Vfs {
            mounts: MountTable::new(),
        }
    }

    // ========== Path Mounts ==========

    /// Mount a filesystem at a path
    pub fn mount(&mut self, path: &str, fs: Box<dyn Filesystem>) -> VfsResult<()> {
        self.mounts.mount(path, fs)
    }

    /// Unmount a filesystem
    pub fn unmount(&mut self, path: &str) -> VfsResult<()> {
        self.mounts.unmount(path)
    }

    // ========== Drive Mounts ==========

    /// Mount a filesystem as a drive letter (e.g., 'C', 'D')
    pub fn mount_drive(&mut self, letter: char, fs: Box<dyn Filesystem>) -> VfsResult<()> {
        self.mounts.mount_drive(letter, fs)
    }

    /// Mount a filesystem as a drive letter with a label
    pub fn mount_drive_labeled(&mut self, letter: char, fs: Box<dyn Filesystem>, label: &str) -> VfsResult<()> {
        self.mounts.mount_drive_labeled(letter, fs, label)
    }

    /// Unmount a drive letter
    pub fn unmount_drive(&mut self, letter: char) -> VfsResult<()> {
        self.mounts.unmount_drive(letter)
    }

    /// Get drive mount info
    pub fn get_drive(&self, letter: char) -> Option<&DriveMount> {
        self.mounts.get_drive(letter)
    }

    /// List all mounted drives
    pub fn list_drives(&self) -> impl Iterator<Item = &DriveMount> {
        self.mounts.list_drives()
    }

    /// List all path mounts
    pub fn list_mounts(&self) -> &[MountPoint] {
        self.mounts.list()
    }

    // ========== Resolution ==========

    /// Resolve path to filesystem and relative path
    /// Automatically handles both Unix paths and drive letter paths
    fn resolve(&self, path: &str) -> VfsResult<(&dyn Filesystem, String)> {
        self.mounts.resolve(path)
    }

    /// Open a file
    pub fn open(&self, path: &str, mode: FileMode) -> VfsResult<Box<dyn FileOperations>> {
        let (fs, rel_path) = self.resolve(path)?;
        fs.open(&rel_path, mode)
    }

    /// Get file statistics
    pub fn stat(&self, path: &str) -> VfsResult<FileStat> {
        let (fs, rel_path) = self.resolve(path)?;
        fs.stat(&rel_path)
    }

    /// Create a directory
    pub fn mkdir(&self, path: &str) -> VfsResult<()> {
        let (fs, rel_path) = self.resolve(path)?;
        fs.mkdir(&rel_path)
    }

    /// Remove a file
    pub fn unlink(&self, path: &str) -> VfsResult<()> {
        let (fs, rel_path) = self.resolve(path)?;
        fs.unlink(&rel_path)
    }

    /// Remove a directory
    pub fn rmdir(&self, path: &str) -> VfsResult<()> {
        let (fs, rel_path) = self.resolve(path)?;
        fs.rmdir(&rel_path)
    }

    /// Read directory entries
    pub fn readdir(&self, path: &str) -> VfsResult<Vec<DirEntry>> {
        let (fs, rel_path) = self.resolve(path)?;
        fs.readdir(&rel_path)
    }

    /// Rename a file
    pub fn rename(&self, old_path: &str, new_path: &str) -> VfsResult<()> {
        let (old_fs, old_rel) = self.resolve(old_path)?;
        let (new_fs, new_rel) = self.resolve(new_path)?;

        // Check same filesystem
        if !core::ptr::eq(old_fs, new_fs) {
            return Err(VfsError::CrossDevice);
        }

        old_fs.rename(&old_rel, &new_rel)
    }
}

impl Default for Vfs {
    fn default() -> Self {
        Self::new()
    }
}

/// Initialize the global VFS
pub fn init() {
    let mut vfs = VFS.lock();
    *vfs = Some(Vfs::new());
}

/// Get a reference to the global VFS
pub fn vfs() -> spin::MutexGuard<'static, Option<Vfs>> {
    VFS.lock()
}

/// Mount a filesystem at a path
pub fn mount(path: &str, fs: Box<dyn Filesystem>) -> VfsResult<()> {
    let mut vfs = VFS.lock();
    match vfs.as_mut() {
        Some(v) => v.mount(path, fs),
        None => Err(VfsError::NotInitialized),
    }
}

/// Mount a filesystem as a drive letter
pub fn mount_drive(letter: char, fs: Box<dyn Filesystem>) -> VfsResult<()> {
    let mut vfs = VFS.lock();
    match vfs.as_mut() {
        Some(v) => v.mount_drive(letter, fs),
        None => Err(VfsError::NotInitialized),
    }
}

/// Mount a filesystem as a drive letter with a label
pub fn mount_drive_labeled(letter: char, fs: Box<dyn Filesystem>, label: &str) -> VfsResult<()> {
    let mut vfs = VFS.lock();
    match vfs.as_mut() {
        Some(v) => v.mount_drive_labeled(letter, fs, label),
        None => Err(VfsError::NotInitialized),
    }
}

/// Unmount a drive letter
pub fn unmount_drive(letter: char) -> VfsResult<()> {
    let mut vfs = VFS.lock();
    match vfs.as_mut() {
        Some(v) => v.unmount_drive(letter),
        None => Err(VfsError::NotInitialized),
    }
}

/// Open a file
pub fn open(path: &str, mode: FileMode) -> VfsResult<Box<dyn FileOperations>> {
    let vfs = VFS.lock();
    match vfs.as_ref() {
        Some(v) => v.open(path, mode),
        None => Err(VfsError::NotInitialized),
    }
}

/// Get file statistics
pub fn stat(path: &str) -> VfsResult<FileStat> {
    let vfs = VFS.lock();
    match vfs.as_ref() {
        Some(v) => v.stat(path),
        None => Err(VfsError::NotInitialized),
    }
}

/// Read directory entries
pub fn readdir(path: &str) -> VfsResult<Vec<DirEntry>> {
    let vfs = VFS.lock();
    match vfs.as_ref() {
        Some(v) => v.readdir(path),
        None => Err(VfsError::NotInitialized),
    }
}
