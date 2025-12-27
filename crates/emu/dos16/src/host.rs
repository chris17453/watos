//! Host interface for DOS emulator
//!
//! Defines traits that the host system must implement to run DOS programs.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

/// Console handle - opaque identifier for a console
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConsoleHandle(pub u32);

/// File handle - opaque identifier for an open file
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileHandle(pub u16);

impl FileHandle {
    pub const STDIN: FileHandle = FileHandle(0);
    pub const STDOUT: FileHandle = FileHandle(1);
    pub const STDERR: FileHandle = FileHandle(2);
    pub const STDAUX: FileHandle = FileHandle(3);
    pub const STDPRN: FileHandle = FileHandle(4);
}

/// File open mode
#[derive(Clone, Copy, Debug)]
pub enum FileMode {
    Read,
    Write,
    ReadWrite,
}

/// File seek origin
#[derive(Clone, Copy, Debug)]
pub enum SeekOrigin {
    Start,
    Current,
    End,
}

/// Directory entry for DOS
#[derive(Clone, Debug)]
pub struct DosDirEntry {
    pub name: [u8; 11],  // 8.3 format, space-padded
    pub attr: u8,
    pub size: u32,
    pub date: u16,
    pub time: u16,
}

/// Error codes for DOS operations
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DosError {
    FileNotFound = 2,
    PathNotFound = 3,
    TooManyOpenFiles = 4,
    AccessDenied = 5,
    InvalidHandle = 6,
    InsufficientMemory = 8,
    InvalidDrive = 15,
    NoMoreFiles = 18,
    WriteProtected = 19,
    UnknownError = 0xFF,
}

/// Host interface trait
///
/// The host system must implement this trait to provide services
/// to the DOS emulator.
pub trait DosHost {
    // Console operations

    /// Create a new console for a DOS task
    fn create_console(&mut self, name: &str, task_id: u32) -> ConsoleHandle;

    /// Destroy a console
    fn destroy_console(&mut self, handle: ConsoleHandle);

    /// Switch to a console (make it active)
    fn switch_console(&mut self, handle: ConsoleHandle);

    /// Write a character to console
    fn console_putchar(&mut self, handle: ConsoleHandle, ch: u8);

    /// Write a string to console
    fn console_write(&mut self, handle: ConsoleHandle, data: &[u8]);

    /// Read a character from console (blocking)
    fn console_getchar(&mut self, handle: ConsoleHandle) -> Option<u8>;

    /// Check if a key is available
    fn console_key_available(&mut self, handle: ConsoleHandle) -> bool;

    /// Get cursor position
    fn console_get_cursor(&self, handle: ConsoleHandle) -> (u8, u8);

    /// Set cursor position
    fn console_set_cursor(&mut self, handle: ConsoleHandle, row: u8, col: u8);

    /// Clear screen
    fn console_clear(&mut self, handle: ConsoleHandle);

    /// Set text attribute
    fn console_set_attr(&mut self, handle: ConsoleHandle, attr: u8);

    // File operations

    /// Open a file
    fn file_open(&mut self, path: &str, mode: FileMode) -> Result<FileHandle, DosError>;

    /// Close a file
    fn file_close(&mut self, handle: FileHandle) -> Result<(), DosError>;

    /// Read from a file
    fn file_read(&mut self, handle: FileHandle, buffer: &mut [u8]) -> Result<usize, DosError>;

    /// Write to a file
    fn file_write(&mut self, handle: FileHandle, buffer: &[u8]) -> Result<usize, DosError>;

    /// Seek in a file
    fn file_seek(&mut self, handle: FileHandle, offset: i32, origin: SeekOrigin) -> Result<u32, DosError>;

    /// Get file size
    fn file_size(&mut self, handle: FileHandle) -> Result<u32, DosError>;

    /// Create a file
    fn file_create(&mut self, path: &str, attr: u8) -> Result<FileHandle, DosError>;

    /// Delete a file
    fn file_delete(&mut self, path: &str) -> Result<(), DosError>;

    /// Rename a file
    fn file_rename(&mut self, old_path: &str, new_path: &str) -> Result<(), DosError>;

    /// Get file attributes
    fn file_get_attr(&mut self, path: &str) -> Result<u8, DosError>;

    /// Set file attributes
    fn file_set_attr(&mut self, path: &str, attr: u8) -> Result<(), DosError>;

    // Directory operations

    /// Get current directory
    fn get_current_dir(&self) -> String;

    /// Set current directory
    fn set_current_dir(&mut self, path: &str) -> Result<(), DosError>;

    /// Create directory
    fn mkdir(&mut self, path: &str) -> Result<(), DosError>;

    /// Remove directory
    fn rmdir(&mut self, path: &str) -> Result<(), DosError>;

    /// Find first matching file
    fn find_first(&mut self, pattern: &str, attr: u8) -> Result<DosDirEntry, DosError>;

    /// Find next matching file
    fn find_next(&mut self) -> Result<DosDirEntry, DosError>;

    // Drive operations

    /// Get current drive (0=A, 1=B, 2=C, ...)
    fn get_current_drive(&self) -> u8;

    /// Set current drive
    fn set_current_drive(&mut self, drive: u8) -> Result<(), DosError>;

    /// Get drive info
    fn get_drive_info(&self, drive: u8) -> Option<DriveInfo>;

    // Time operations

    /// Get current date (year, month, day, day_of_week)
    fn get_date(&self) -> (u16, u8, u8, u8);

    /// Get current time (hour, minute, second, hundredths)
    fn get_time(&self) -> (u8, u8, u8, u8);

    // Memory operations (for INT 21h AH=48h, 49h, 4Ah)
    /// These operate on DOS conventional memory, managed by the emulator

    /// Exit program
    fn exit_program(&mut self, code: u8);
}

/// Drive information
#[derive(Clone, Debug)]
pub struct DriveInfo {
    pub sectors_per_cluster: u16,
    pub bytes_per_sector: u16,
    pub total_clusters: u16,
    pub free_clusters: u16,
}
