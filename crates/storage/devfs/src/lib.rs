//! WATOS Device Filesystem (/dev)
//!
//! A virtual filesystem providing access to devices as files.
//!
//! # Standard Devices
//!
//! | Device | Type | Description |
//! |--------|------|-------------|
//! | null   | char | Discards writes, reads return EOF |
//! | zero   | char | Discards writes, reads return zeros |
//! | full   | char | Writes fail with ENOSPC, reads return zeros |
//! | random | char | Reads return random bytes |
//! | console| char | Active console |
//! | tty    | char | Current terminal |
//!
//! # Usage
//!
//! ```ignore
//! let devfs = DevFs::new();
//! vfs.mount("/dev", Box::new(devfs));
//! ```

#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use watos_vfs::{
    DirEntry, FileMode, FileOperations, FileStat, FileType, Filesystem, FsStats,
    VfsError, VfsResult,
};

mod devices;

pub use devices::*;

/// Device trait - all devices must implement this
pub trait Device: Send + Sync {
    /// Device name (e.g., "null", "zero")
    fn name(&self) -> &'static str;

    /// Device type
    fn device_type(&self) -> FileType;

    /// Major device number
    fn major(&self) -> u32 {
        0
    }

    /// Minor device number
    fn minor(&self) -> u32 {
        0
    }

    /// Open the device, returning file operations
    fn open(&self, mode: FileMode) -> VfsResult<Box<dyn FileOperations>>;

    /// Get device statistics
    fn stat(&self) -> FileStat {
        FileStat {
            file_type: self.device_type(),
            size: 0,
            nlink: 1,
            inode: 0,
            dev: ((self.major() as u64) << 8) | (self.minor() as u64),
            mode: 0o666, // rw-rw-rw-
            uid: 0,
            gid: 0,
            blksize: 512,
            blocks: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
        }
    }
}

/// Device entry in the devfs
struct DeviceEntry {
    /// Device name
    name: String,
    /// The device
    device: Box<dyn Device>,
}

/// DevFS - Device Filesystem
pub struct DevFs {
    /// Registered devices
    devices: Mutex<Vec<DeviceEntry>>,
}

impl DevFs {
    /// Create a new DevFS with standard devices
    pub fn new() -> Self {
        let devfs = DevFs {
            devices: Mutex::new(Vec::new()),
        };

        // Register standard devices
        devfs.register(Box::new(NullDevice));
        devfs.register(Box::new(ZeroDevice));
        devfs.register(Box::new(FullDevice));
        devfs.register(Box::new(RandomDevice::new()));

        devfs
    }

    /// Create an empty DevFS (no standard devices)
    pub fn empty() -> Self {
        DevFs {
            devices: Mutex::new(Vec::new()),
        }
    }

    /// Register a device
    pub fn register(&self, device: Box<dyn Device>) {
        let mut devices = self.devices.lock();
        let name = String::from(device.name());

        // Remove existing device with same name
        devices.retain(|d| d.name != name);

        devices.push(DeviceEntry { name, device });
    }

    /// Unregister a device
    pub fn unregister(&self, name: &str) {
        let mut devices = self.devices.lock();
        devices.retain(|d| d.name != name);
    }

    /// Get a device by name
    fn get_device(&self, name: &str) -> Option<FileStat> {
        let devices = self.devices.lock();
        devices.iter().find(|d| d.name == name).map(|d| d.device.stat())
    }

    /// Open a device by name
    fn open_device(&self, name: &str, mode: FileMode) -> VfsResult<Box<dyn FileOperations>> {
        let devices = self.devices.lock();
        match devices.iter().find(|d| d.name == name) {
            Some(entry) => entry.device.open(mode),
            None => Err(VfsError::NotFound),
        }
    }
}

impl Default for DevFs {
    fn default() -> Self {
        Self::new()
    }
}

impl Filesystem for DevFs {
    fn name(&self) -> &'static str {
        "devfs"
    }

    fn open(&self, path: &str, mode: FileMode) -> VfsResult<Box<dyn FileOperations>> {
        let name = path.trim_start_matches('/');
        if name.is_empty() {
            return Err(VfsError::IsADirectory);
        }
        self.open_device(name, mode)
    }

    fn stat(&self, path: &str) -> VfsResult<FileStat> {
        let name = path.trim_start_matches('/');

        if name.is_empty() {
            // Root directory
            return Ok(FileStat {
                file_type: FileType::Directory,
                size: 0,
                nlink: 2,
                inode: 1,
                dev: 0,
                mode: 0o755,
                uid: 0,
                gid: 0,
                blksize: 512,
                blocks: 0,
                atime: 0,
                mtime: 0,
                ctime: 0,
            });
        }

        self.get_device(name).ok_or(VfsError::NotFound)
    }

    fn mkdir(&self, _path: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn unlink(&self, _path: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn rmdir(&self, _path: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn readdir(&self, path: &str) -> VfsResult<Vec<DirEntry>> {
        let name = path.trim_start_matches('/');
        if !name.is_empty() {
            return Err(VfsError::NotADirectory);
        }

        let devices = self.devices.lock();
        let entries: Vec<DirEntry> = devices
            .iter()
            .enumerate()
            .map(|(i, d)| DirEntry {
                name: d.name.clone(),
                file_type: d.device.device_type(),
                size: 0,
                inode: (i + 2) as u64,
            })
            .collect();

        Ok(entries)
    }

    fn rename(&self, _old: &str, _new: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn sync(&self) -> VfsResult<()> {
        Ok(())
    }

    fn statfs(&self) -> VfsResult<FsStats> {
        Ok(FsStats {
            total_blocks: 0,
            free_blocks: 0,
            block_size: 0,
            total_inodes: self.devices.lock().len() as u64 + 1,
            free_inodes: 0,
            max_name_len: 255,
        })
    }

    fn chmod(&self, _path: &str, _mode: u32) -> VfsResult<()> {
        // devfs is a virtual filesystem - doesn't support permissions
        Err(VfsError::NotSupported)
    }

    fn chown(&self, _path: &str, _uid: u32, _gid: u32) -> VfsResult<()> {
        // devfs is a virtual filesystem - doesn't support ownership
        Err(VfsError::NotSupported)
    }
}
