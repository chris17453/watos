//! VFS Error types

/// VFS Result type
pub type VfsResult<T> = Result<T, VfsError>;

/// VFS Error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsError {
    /// File or directory not found
    NotFound,
    /// Permission denied
    PermissionDenied,
    /// File already exists
    AlreadyExists,
    /// Not a directory
    NotADirectory,
    /// Is a directory (when expecting file)
    IsADirectory,
    /// Directory not empty
    DirectoryNotEmpty,
    /// No space left on device
    NoSpace,
    /// Read-only filesystem
    ReadOnly,
    /// Too many open files
    TooManyOpenFiles,
    /// Invalid argument
    InvalidArgument,
    /// I/O error
    IoError,
    /// Cross-device link/rename
    CrossDevice,
    /// Name too long
    NameTooLong,
    /// Path too long
    PathTooLong,
    /// Invalid path
    InvalidPath,
    /// Not a mount point
    NotMounted,
    /// Already mounted
    AlreadyMounted,
    /// Busy (in use)
    Busy,
    /// VFS not initialized
    NotInitialized,
    /// Operation not supported
    NotSupported,
    /// Invalid name
    InvalidName,
    /// Not a file (when expecting file)
    NotAFile,
    /// Corrupted data
    Corrupted,
    /// Filesystem-specific error
    FsError(i32),
}

impl VfsError {
    /// Convert to errno-style error code
    pub fn to_errno(&self) -> i32 {
        match self {
            VfsError::NotFound => -2,           // ENOENT
            VfsError::PermissionDenied => -13,  // EACCES
            VfsError::AlreadyExists => -17,     // EEXIST
            VfsError::NotADirectory => -20,     // ENOTDIR
            VfsError::IsADirectory => -21,      // EISDIR
            VfsError::InvalidArgument => -22,   // EINVAL
            VfsError::NoSpace => -28,           // ENOSPC
            VfsError::ReadOnly => -30,          // EROFS
            VfsError::NameTooLong => -36,       // ENAMETOOLONG
            VfsError::DirectoryNotEmpty => -39, // ENOTEMPTY
            VfsError::TooManyOpenFiles => -24,  // EMFILE
            VfsError::CrossDevice => -18,       // EXDEV
            VfsError::IoError => -5,            // EIO
            VfsError::Busy => -16,              // EBUSY
            VfsError::PathTooLong => -36,
            VfsError::InvalidPath => -22,
            VfsError::NotMounted => -22,
            VfsError::AlreadyMounted => -16,
            VfsError::NotInitialized => -22,
            VfsError::NotSupported => -38,      // ENOSYS
            VfsError::InvalidName => -22,       // EINVAL
            VfsError::NotAFile => -21,          // EISDIR (not a file)
            VfsError::Corrupted => -5,          // EIO (corruption)
            VfsError::FsError(e) => *e,
        }
    }
}
