//! File handle and file types

/// File type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    /// Regular file
    Regular,
    /// Directory
    Directory,
    /// Symbolic link
    Symlink,
    /// Block device
    BlockDevice,
    /// Character device
    CharDevice,
    /// Named pipe (FIFO)
    Fifo,
    /// Unix socket
    Socket,
    /// Unknown type
    Unknown,
}

/// File open mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileMode {
    /// Read access
    pub read: bool,
    /// Write access
    pub write: bool,
    /// Append mode
    pub append: bool,
    /// Create if not exists
    pub create: bool,
    /// Truncate on open
    pub truncate: bool,
    /// Fail if exists (with create)
    pub exclusive: bool,
}

impl FileMode {
    /// Read-only mode
    pub const READ: FileMode = FileMode {
        read: true,
        write: false,
        append: false,
        create: false,
        truncate: false,
        exclusive: false,
    };

    /// Write-only mode (create/truncate)
    pub const WRITE: FileMode = FileMode {
        read: false,
        write: true,
        append: false,
        create: true,
        truncate: true,
        exclusive: false,
    };

    /// Read-write mode
    pub const READ_WRITE: FileMode = FileMode {
        read: true,
        write: true,
        append: false,
        create: false,
        truncate: false,
        exclusive: false,
    };

    /// Append mode
    pub const APPEND: FileMode = FileMode {
        read: false,
        write: true,
        append: true,
        create: true,
        truncate: false,
        exclusive: false,
    };

    /// Create new file (fail if exists)
    pub const CREATE_NEW: FileMode = FileMode {
        read: false,
        write: true,
        append: false,
        create: true,
        truncate: false,
        exclusive: true,
    };
}

/// File statistics
#[derive(Debug, Clone, Copy)]
pub struct FileStat {
    /// File type
    pub file_type: FileType,
    /// File size in bytes
    pub size: u64,
    /// Number of hard links
    pub nlink: u32,
    /// Inode number
    pub inode: u64,
    /// Device ID
    pub dev: u64,
    /// File mode/permissions
    pub mode: u32,
    /// Owner user ID
    pub uid: u32,
    /// Owner group ID
    pub gid: u32,
    /// Block size for I/O
    pub blksize: u32,
    /// Number of 512-byte blocks allocated
    pub blocks: u64,
    /// Access time (seconds since epoch)
    pub atime: u64,
    /// Modification time
    pub mtime: u64,
    /// Status change time
    pub ctime: u64,
}

impl Default for FileStat {
    fn default() -> Self {
        FileStat {
            file_type: FileType::Unknown,
            size: 0,
            nlink: 1,
            inode: 0,
            dev: 0,
            mode: 0,
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

/// File handle (used by processes)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileHandle(pub i32);

impl FileHandle {
    /// Invalid file handle
    pub const INVALID: FileHandle = FileHandle(-1);

    /// Standard input
    pub const STDIN: FileHandle = FileHandle(0);

    /// Standard output
    pub const STDOUT: FileHandle = FileHandle(1);

    /// Standard error
    pub const STDERR: FileHandle = FileHandle(2);

    /// Check if handle is valid
    pub fn is_valid(&self) -> bool {
        self.0 >= 0
    }
}
