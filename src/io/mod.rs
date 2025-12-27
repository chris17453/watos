//! I/O subsystem - Handle-based I/O and file operations

pub mod file_io;

pub use file_io::{
    HandleTable, HandleIO, OpenMode, ConsoleKind,
    init_file_io, mount_filesystem, fs_error_to_errno
};