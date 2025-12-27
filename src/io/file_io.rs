//! WATOS Handle-Based I/O System
//!
//! This module provides the WATOS handle table and file I/O system.
//! Each process owns a private handle table with explicit handle semantics.
//! No handle inheritance unless explicitly requested.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use alloc::string::String;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::disk::vfs::{FileSystem, BoxedFs, FsError, create_vfs};
use crate::disk::drives::{FsType};

/// Handle type - opaque integers for kernel objects
pub type Handle = u32;

/// File open modes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OpenMode {
    ReadOnly = 0,
    WriteOnly = 1,
    Append = 2,
    ReadWrite = 3,
}

impl From<u64> for OpenMode {
    fn from(mode: u64) -> Self {
        match mode {
            0 => OpenMode::ReadOnly,
            1 => OpenMode::WriteOnly,
            2 => OpenMode::Append,
            3 => OpenMode::ReadWrite,
            _ => OpenMode::ReadOnly,
        }
    }
}

/// Kernel objects that can be stored in handle tables
#[derive(Debug)]
pub enum KernelObject {
    File(FileObject),
    Console(ConsoleObject),
    // Future: Event, Memory, Process, Thread, etc.
}

/// File kernel object
#[derive(Debug)]
pub struct FileObject {
    path: String,
    mode: OpenMode,
    position: u64,
    fs_id: u32,
}

/// Console kernel object (stdin/stdout/stderr)
#[derive(Debug)]
pub struct ConsoleObject {
    pub kind: ConsoleKind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConsoleKind {
    Stdin,
    Stdout,
    Stderr,
}

/// Per-process handle table
/// Every WATOS process owns a private handle table.
/// No handle inheritance unless explicitly requested.
#[derive(Debug)]
pub struct HandleTable {
    objects: BTreeMap<Handle, KernelObject>,
    next_handle: Handle,
}

impl HandleTable {
    /// Create a new handle table with no handles
    /// In WATOS, child processes get no handles by default
    pub fn new() -> Self {
        Self {
            objects: BTreeMap::new(),
            next_handle: 1, // Start at 1, no reserved handles
        }
    }

    /// Create a handle table with standard console handles
    /// This is optional - not all processes need console access
    pub fn new_with_console() -> Self {
        let mut table = Self::new();
        // Explicitly create console handles if needed
        table.add_console_handle(ConsoleKind::Stdin);
        table.add_console_handle(ConsoleKind::Stdout);
        table.add_console_handle(ConsoleKind::Stderr);
        table
    }

    /// Allocate a new handle
    fn allocate_handle(&mut self) -> Handle {
        let handle = self.next_handle;
        self.next_handle += 1;
        handle
    }

    /// Add a console handle
    pub fn add_console_handle(&mut self, kind: ConsoleKind) -> Handle {
        let handle = self.allocate_handle();
        let console = ConsoleObject { kind };
        self.objects.insert(handle, KernelObject::Console(console));
        handle
    }

    /// Open a file and return handle
    pub fn open_file(&mut self, path: &str, mode: OpenMode, fs_id: u32) -> Result<Handle, FsError> {
        let handle = self.allocate_handle();
        let file_obj = FileObject {
            path: String::from(path),
            mode,
            position: 0,
            fs_id,
        };
        
        self.objects.insert(handle, KernelObject::File(file_obj));
        Ok(handle)
    }

    /// Close a handle
    pub fn close(&mut self, handle: Handle) -> Result<(), FsError> {
        self.objects.remove(&handle).ok_or(FsError::NotFound)?;
        Ok(())
    }

    /// Get kernel object by handle
    pub fn get(&self, handle: Handle) -> Option<&KernelObject> {
        self.objects.get(&handle)
    }

    /// Get mutable kernel object by handle
    pub fn get_mut(&mut self, handle: Handle) -> Option<&mut KernelObject> {
        self.objects.get_mut(&handle)
    }

    /// Get file object by handle
    pub fn get_file(&self, handle: Handle) -> Option<&FileObject> {
        match self.objects.get(&handle) {
            Some(KernelObject::File(file)) => Some(file),
            _ => None,
        }
    }

    /// Get mutable file object by handle
    pub fn get_file_mut(&mut self, handle: Handle) -> Option<&mut FileObject> {
        match self.objects.get_mut(&handle) {
            Some(KernelObject::File(file)) => Some(file),
            _ => None,
        }
    }

    /// Get console object by handle
    pub fn get_console(&self, handle: Handle) -> Option<&ConsoleObject> {
        match self.objects.get(&handle) {
            Some(KernelObject::Console(console)) => Some(console),
            _ => None,
        }
    }
}

impl Default for HandleTable {
    fn default() -> Self {
        Self::new() // No handles by default - explicit is better
    }
}

/// Global filesystem registry
static mut FILESYSTEMS: Option<BTreeMap<u32, BoxedFs>> = None;
static NEXT_FS_ID: AtomicU32 = AtomicU32::new(1);

/// Initialize the global file I/O system
pub fn init_file_io() {
    unsafe {
        FILESYSTEMS = Some(BTreeMap::new());
    }
}

/// Register a filesystem and return its ID
pub fn register_filesystem(fs: BoxedFs) -> u32 {
    let fs_id = NEXT_FS_ID.fetch_add(1, Ordering::SeqCst);
    
    unsafe {
        if let Some(ref mut filesystems) = FILESYSTEMS {
            filesystems.insert(fs_id, fs);
        }
    }
    
    fs_id
}

/// Mount a filesystem from drive information
pub fn mount_filesystem(fs_type: FsType, port: u8, start_lba: u64) -> Option<u32> {
    if let Some(fs) = create_vfs(fs_type, port, start_lba) {
        Some(register_filesystem(fs))
    } else {
        None
    }
}

/// Get filesystem by ID
fn get_filesystem(fs_id: u32) -> Option<&'static mut dyn FileSystem> {
    unsafe {
        if let Some(ref mut filesystems) = FILESYSTEMS {
            filesystems.get_mut(&fs_id).map(|fs| fs.as_mut())
        } else {
            None
        }
    }
}

/// WATOS Handle-based I/O operations
pub struct HandleIO;

impl HandleIO {
    /// Open a file and return handle
    pub fn open(handle_table: &mut HandleTable, path: &str, mode: OpenMode, fs_id: u32) -> Result<Handle, FsError> {
        // Verify the filesystem exists
        if get_filesystem(fs_id).is_none() {
            return Err(FsError::NotFound);
        }
        
        // Check if file exists for read operations
        if let Some(fs) = get_filesystem(fs_id) {
            if mode == OpenMode::ReadOnly || mode == OpenMode::ReadWrite {
                if !fs.exists(path) {
                    return Err(FsError::NotFound);
                }
            }
        }
        
        handle_table.open_file(path, mode, fs_id)
    }

    /// Close a handle
    pub fn close(handle_table: &mut HandleTable, handle: Handle) -> Result<(), FsError> {
        handle_table.close(handle)
    }

    /// Read from a file handle
    pub fn read_file(handle_table: &mut HandleTable, handle: Handle, buffer: &mut [u8]) -> Result<usize, FsError> {
        let file_obj = handle_table.get_file_mut(handle).ok_or(FsError::NotFound)?;
        
        if file_obj.mode == OpenMode::WriteOnly {
            return Err(FsError::PermissionDenied);
        }
        
        let fs_id = file_obj.fs_id;
        let path = file_obj.path.clone(); // Clone to avoid borrow checker issues
        
        if let Some(fs) = get_filesystem(fs_id) {
            // For now, read the entire file and return the portion requested
            let file_data = fs.read_file(&path)?;
            let start = file_obj.position as usize;
            let end = (start + buffer.len()).min(file_data.len());
            
            if start >= file_data.len() {
                return Ok(0); // EOF
            }
            
            let bytes_read = end - start;
            buffer[..bytes_read].copy_from_slice(&file_data[start..end]);
            file_obj.position += bytes_read as u64;
            
            Ok(bytes_read)
        } else {
            Err(FsError::IoError)
        }
    }

    /// Write to a file handle
    pub fn write_file(handle_table: &mut HandleTable, handle: Handle, data: &[u8]) -> Result<usize, FsError> {
        let file_obj = handle_table.get_file_mut(handle).ok_or(FsError::NotFound)?;
        
        if file_obj.mode == OpenMode::ReadOnly {
            return Err(FsError::PermissionDenied);
        }
        
        let fs_id = file_obj.fs_id;
        let path = file_obj.path.clone();
        
        if let Some(fs) = get_filesystem(fs_id) {
            // For now, simple append or overwrite
            // TODO: Implement proper position-based writing
            match file_obj.mode {
                OpenMode::WriteOnly | OpenMode::ReadWrite => {
                    fs.write_file(&path, data)?;
                    Ok(data.len())
                }
                OpenMode::Append => {
                    // Read existing file and append
                    let mut existing = fs.read_file(&path).unwrap_or_else(|_| Vec::new());
                    existing.extend_from_slice(data);
                    fs.write_file(&path, &existing)?;
                    Ok(data.len())
                }
                _ => Err(FsError::PermissionDenied),
            }
        } else {
            Err(FsError::IoError)
        }
    }

    /// Write to console handle
    pub fn write_console(handle_table: &HandleTable, handle: Handle, data: &[u8]) -> Result<usize, FsError> {
        if let Some(console) = handle_table.get_console(handle) {
            match console.kind {
                ConsoleKind::Stdout | ConsoleKind::Stderr => {
                    crate::console::print(data);
                    Ok(data.len())
                }
                ConsoleKind::Stdin => {
                    Err(FsError::PermissionDenied) // Can't write to stdin
                }
            }
        } else {
            Err(FsError::NotFound)
        }
    }

    /// Read from console handle
    pub fn read_console(handle_table: &HandleTable, handle: Handle, buffer: &mut [u8]) -> Result<usize, FsError> {
        if let Some(console) = handle_table.get_console(handle) {
            match console.kind {
                ConsoleKind::Stdin => {
                    // For now, just try to read one key
                    if let Some(scancode) = crate::interrupts::get_scancode() {
                        // Simple scancode to ASCII conversion
                        let ascii = match scancode {
                            0x1C => b'\n', // Enter
                            0x39 => b' ',  // Space  
                            0x1E..=0x26 => b'a' + (scancode - 0x1E), // a-l
                            0x10..=0x19 => b'q' + (scancode - 0x10), // q-p
                            0x2C..=0x32 => b'z' + (scancode - 0x2C), // z-m
                            0x02..=0x0B => b'1' + (scancode - 0x02), // 1-0
                            _ => b'?',
                        };
                        if buffer.len() > 0 {
                            buffer[0] = ascii;
                            Ok(1)
                        } else {
                            Ok(0)
                        }
                    } else {
                        Ok(0) // No input available
                    }
                }
                ConsoleKind::Stdout | ConsoleKind::Stderr => {
                    Err(FsError::PermissionDenied) // Can't read from output streams
                }
            }
        } else {
            Err(FsError::NotFound)
        }
    }

    /// Check if a file exists on filesystem
    pub fn exists(path: &str, fs_id: u32) -> bool {
        if let Some(fs) = get_filesystem(fs_id) {
            fs.exists(path)
        } else {
            false
        }
    }

    /// Get file information from filesystem
    pub fn stat(path: &str, fs_id: u32) -> Result<crate::disk::vfs::DirEntry, FsError> {
        if let Some(fs) = get_filesystem(fs_id) {
            fs.stat(path)
        } else {
            Err(FsError::NotFound)
        }
    }

    /// List directory contents from filesystem
    pub fn list_dir(path: &str, fs_id: u32) -> Result<Vec<crate::disk::vfs::DirEntry>, FsError> {
        if let Some(fs) = get_filesystem(fs_id) {
            fs.read_dir(path)
        } else {
            Err(FsError::NotFound)
        }
    }
}

/// Convert VFS errors to errno-style return codes
pub fn fs_error_to_errno(error: FsError) -> u64 {
    match error {
        FsError::NotFound => u64::MAX,
        FsError::PermissionDenied => u64::MAX - 1,
        FsError::NotADirectory => u64::MAX - 2,
        FsError::NotAFile => u64::MAX - 3,
        FsError::IoError => u64::MAX - 4,
        FsError::NoSpace => u64::MAX - 5,
        FsError::InvalidName => u64::MAX - 6,
        FsError::AlreadyExists => u64::MAX - 7,
        FsError::NotSupported => u64::MAX - 8,
        FsError::Corrupted => u64::MAX - 9,
    }
}