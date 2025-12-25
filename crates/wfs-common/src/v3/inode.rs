//! Inode Structure
//!
//! Inodes are stored in the inode B+tree, keyed by inode number.
//! Each inode represents a file, directory, or other filesystem object.

use super::structures::{S_IFDIR, S_IFREG, S_IFLNK, S_IFMT, INLINE_DATA_MAX};
use crate::crc32;

// ============================================================================
// CONSTANTS
// ============================================================================

/// Inode size on disk
pub const INODE_SIZE: usize = 256;

/// Size of inline data area
pub const INODE_INLINE_SIZE: usize = 160;

// ============================================================================
// INODE FLAGS
// ============================================================================

/// File is immutable
pub const INODE_IMMUTABLE: u32 = 0x0001;

/// File is append-only
pub const INODE_APPEND: u32 = 0x0002;

/// Do not update atime
pub const INODE_NOATIME: u32 = 0x0004;

/// File data is shared (CoW reference)
pub const INODE_SHARED: u32 = 0x0008;

/// File has inline data (no extent tree)
pub const INODE_INLINE: u32 = 0x0010;

/// Inode is deleted (pending garbage collection)
pub const INODE_DELETED: u32 = 0x0020;

// ============================================================================
// INODE STRUCTURE
// ============================================================================

/// Inode - 256 bytes on disk
///
/// Stored in the inode tree, keyed by inode_num.
/// Small files can store data inline (up to 178 bytes).
/// Larger files use an extent tree pointed to by extent_root.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Inode {
    // Identity (8 bytes)
    pub inode_num: u64,                // Unique inode number

    // Type and permissions (8 bytes)
    pub mode: u32,                     // File type + permissions (S_IF* + rwx)
    pub flags: u32,                    // Inode flags (immutable, append, etc.)

    // Size (16 bytes)
    pub size: u64,                     // File size in bytes
    pub blocks: u64,                   // Blocks allocated (for quota)

    // Ownership (8 bytes)
    pub uid: u32,                      // Owner user ID
    pub gid: u32,                      // Owner group ID

    // Timestamps (24 bytes)
    pub atime: u64,                    // Access time (seconds since epoch)
    pub mtime: u64,                    // Modify time
    pub ctime: u64,                    // Change time (inode change)

    // Links (8 bytes)
    pub nlink: u32,                    // Hard link count
    pub _pad0: u32,

    // Data location (16 bytes)
    pub extent_root: u64,              // Root block of extent tree (if not inline)
    pub refcount: u64,                 // Reference count for CoW sharing

    // Inline/small file support (4 bytes)
    pub inline_size: u16,              // Bytes of inline data (0 if using extents)
    pub _pad1: u16,

    // Integrity (4 bytes)
    pub crc32: u32,

    // Inline data - for small files (160 bytes)
    // If inline_size > 0, file data is stored here instead of extent tree
    pub inline_data: [u8; INODE_INLINE_SIZE],
}

impl Inode {
    /// Create a new inode
    pub fn new(inode_num: u64, mode: u32) -> Self {
        Self {
            inode_num,
            mode,
            flags: 0,
            size: 0,
            blocks: 0,
            uid: 0,
            gid: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
            nlink: 1,
            _pad0: 0,
            extent_root: 0,
            refcount: 1,
            inline_size: 0,
            _pad1: 0,
            crc32: 0,
            inline_data: [0; INODE_INLINE_SIZE],
        }
    }

    /// Create a new regular file inode
    pub fn new_file(inode_num: u64) -> Self {
        Self::new(inode_num, S_IFREG | 0o644)
    }

    /// Create a new directory inode
    pub fn new_directory(inode_num: u64) -> Self {
        let mut inode = Self::new(inode_num, S_IFDIR | 0o755);
        inode.nlink = 2; // . and parent's reference
        inode
    }

    /// Create a new symlink inode
    pub fn new_symlink(inode_num: u64) -> Self {
        Self::new(inode_num, S_IFLNK | 0o777)
    }

    /// Check if this is a regular file
    pub fn is_file(&self) -> bool {
        (self.mode & S_IFMT) == S_IFREG
    }

    /// Check if this is a directory
    pub fn is_directory(&self) -> bool {
        (self.mode & S_IFMT) == S_IFDIR
    }

    /// Check if this is a symbolic link
    pub fn is_symlink(&self) -> bool {
        (self.mode & S_IFMT) == S_IFLNK
    }

    /// Check if file data is stored inline
    pub fn is_inline(&self) -> bool {
        self.flags & INODE_INLINE != 0 || self.inline_size > 0
    }

    /// Check if this inode has shared data (CoW)
    pub fn is_shared(&self) -> bool {
        self.refcount > 1 || (self.flags & INODE_SHARED != 0)
    }

    /// Check if data fits inline
    pub fn can_inline(size: usize) -> bool {
        size <= INLINE_DATA_MAX
    }

    /// Set inline data (for small files)
    pub fn set_inline_data(&mut self, data: &[u8]) -> bool {
        if data.len() > INODE_INLINE_SIZE {
            return false;
        }
        self.inline_data[..data.len()].copy_from_slice(data);
        self.inline_size = data.len() as u16;
        self.size = data.len() as u64;
        self.flags |= INODE_INLINE;
        true
    }

    /// Get inline data
    pub fn get_inline_data(&self) -> &[u8] {
        &self.inline_data[..self.inline_size as usize]
    }

    /// Calculate CRC
    pub fn calculate_crc(&self) -> u32 {
        // CRC everything before crc32 field (offset 74 in structure)
        let offset = 74; // 8+8+16+8+24+8+16+4 - 2 for inline_size position
        let bytes = unsafe {
            core::slice::from_raw_parts(self as *const _ as *const u8, offset)
        };
        crc32(bytes)
    }

    /// Update CRC
    pub fn update_crc(&mut self) {
        self.crc32 = self.calculate_crc();
    }

    /// Verify CRC
    pub fn verify_crc(&self) -> bool {
        self.crc32 == self.calculate_crc()
    }

    /// Increment reference count (for CoW)
    pub fn inc_refcount(&mut self) {
        self.refcount = self.refcount.saturating_add(1);
        if self.refcount > 1 {
            self.flags |= INODE_SHARED;
        }
    }

    /// Decrement reference count
    pub fn dec_refcount(&mut self) -> u64 {
        self.refcount = self.refcount.saturating_sub(1);
        if self.refcount <= 1 {
            self.flags &= !INODE_SHARED;
        }
        self.refcount
    }
}

impl Default for Inode {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

// ============================================================================
// INODE TREE ENTRY
// ============================================================================

/// Entry in the inode tree leaf
/// Key: inode_num (u64)
/// Value: Inode structure
pub const INODE_ENTRY_SIZE: usize = 8 + INODE_SIZE; // key + value

// ============================================================================
// COMPILE-TIME CHECKS
// ============================================================================

const _: () = assert!(core::mem::size_of::<Inode>() == INODE_SIZE);
const _: () = assert!(INODE_INLINE_SIZE == 160);
