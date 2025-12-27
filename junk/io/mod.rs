//! I/O subsystem - Handle-based I/O and file operations

pub mod file_io;

// Re-export HandleTable and related types from watos-process
pub use watos_process::{
    HandleTable, Handle, OpenMode, ConsoleKind, ConsoleObject, FileObject, KernelObject
};

pub use file_io::{
    HandleIO, init_file_io, mount_filesystem, fs_error_to_errno, mode_from_u64
};
