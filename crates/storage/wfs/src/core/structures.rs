//! WFS Core Data Structures
//!
//! On-disk structures for the Copy-on-Write filesystem.

#[allow(unused_imports)]
use crate::prelude::*;
use crate::crc32;

// ============================================================================
// CONSTANTS
// ============================================================================

/// WFS magic number ("WFS")
pub const WFS_MAGIC: u32 = 0x57465331;

/// Filesystem version
pub const WFS_VERSION: u16 = 1;

/// Block size (fixed at 4KB)
pub const BLOCK_SIZE: u32 = 4096;

/// Sectors per block (for AHCI)
pub const SECTORS_PER_BLOCK: u32 = BLOCK_SIZE / 512;

/// Default chunk size for CoW (64KB = 16 blocks)
pub const DEFAULT_CHUNK_SIZE: u32 = 65536;

/// Blocks per chunk
pub const BLOCKS_PER_CHUNK: u32 = DEFAULT_CHUNK_SIZE / BLOCK_SIZE;

/// Superblock size
pub const SUPERBLOCK_SIZE: usize = 256;

/// Root inode number (always 1)
pub const ROOT_INODE: u64 = 1;

/// Maximum filename length
pub const MAX_FILENAME: usize = 255;

/// Inline data threshold - files smaller than this are stored in inode
pub const INLINE_DATA_MAX: usize = 160;

// Filesystem signature
pub const WFS_SIGNATURE: &[u8; 64] =
    b"WFS CoW FileSystem - Chris Watkins <chris@watkinslabs.com>\0\0\0\0\0\0";

// ============================================================================
// SUPERBLOCK
// ============================================================================

/// Superblock - stored at block 0 and block 1 (backup)
///
/// The superblock contains the atomic root pointer that makes CoW work.
/// Updating root_tree_block atomically commits all pending changes.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Superblock {
    // Header (8 bytes)
    pub magic: u32,                    // "WFS" (0x57465331)
    pub version: u16,                  // Version 3
    pub flags: u16,                    // Mount flags

    // Signature (64 bytes)
    pub signature: [u8; 64],           // Creator signature

    // Block configuration (8 bytes)
    pub block_size: u32,               // Always 4096
    pub chunk_size: u32,               // CoW chunk size (default 65536)

    // Disk geometry (16 bytes)
    pub total_blocks: u64,             // Total disk capacity
    pub free_blocks: u64,              // Available blocks

    // Atomic root pointers - THE KEY TO CoW (16 bytes)
    pub root_tree_block: u64,          // Current root of inode tree
    pub root_generation: u64,          // Monotonic generation number

    // Tree locations (8 bytes)
    pub free_tree_block: u64,          // Free space tree root

    // Metadata (24 bytes)
    pub inode_count: u64,              // Total inodes allocated
    pub next_inode: u64,               // Next inode number to allocate
    pub data_start_block: u64,         // First block for data/trees

    // Reserved for future use (104 bytes)
    pub reserved: [u8; 104],

    // Integrity (8 bytes)
    pub crc32: u32,                    // Checksum
    pub _pad: u32,
}

/// Size of data to CRC (everything before crc32 field)
pub const SUPERBLOCK_CRC_OFFSET: usize = 248;

impl Superblock {
    /// Create a new superblock for a disk of given size
    pub fn new(total_blocks: u64) -> Self {
        Self {
            magic: WFS_MAGIC,
            version: WFS_VERSION,
            flags: 0,
            signature: *WFS_SIGNATURE,
            block_size: BLOCK_SIZE,
            chunk_size: DEFAULT_CHUNK_SIZE,
            total_blocks,
            free_blocks: total_blocks.saturating_sub(4), // Reserve superblocks + initial trees
            root_tree_block: 0,
            root_generation: 0,
            free_tree_block: 0,
            inode_count: 0,
            next_inode: ROOT_INODE,
            data_start_block: 2, // After primary and backup superblocks
            reserved: [0; 104],
            crc32: 0,
            _pad: 0,
        }
    }

    /// Calculate and set CRC
    pub fn update_crc(&mut self) {
        self.crc32 = self.calculate_crc();
    }

    /// Calculate CRC of superblock data
    pub fn calculate_crc(&self) -> u32 {
        let bytes = unsafe {
            core::slice::from_raw_parts(self as *const _ as *const u8, SUPERBLOCK_CRC_OFFSET)
        };
        crc32(bytes)
    }

    /// Verify CRC
    pub fn verify_crc(&self) -> bool {
        self.crc32 == self.calculate_crc()
    }

    /// Check if this is a valid WFS superblock
    pub fn is_valid(&self) -> bool {
        self.magic == WFS_MAGIC && self.version == WFS_VERSION && self.verify_crc()
    }
}

impl Default for Superblock {
    fn default() -> Self {
        Self::new(0)
    }
}

// ============================================================================
// MOUNT FLAGS
// ============================================================================

/// Filesystem is mounted read-only
pub const FLAG_READONLY: u16 = 0x0001;

/// Filesystem needs fsck
pub const FLAG_DIRTY: u16 = 0x0002;

/// Enable compression (future)
pub const FLAG_COMPRESS: u16 = 0x0004;

/// Enable encryption (future)
pub const FLAG_ENCRYPT: u16 = 0x0008;

// ============================================================================
// INODE TYPES (mode field)
// ============================================================================

/// Regular file
pub const S_IFREG: u32 = 0o100000;

/// Directory
pub const S_IFDIR: u32 = 0o040000;

/// Symbolic link
pub const S_IFLNK: u32 = 0o120000;

/// Block device (future)
pub const S_IFBLK: u32 = 0o060000;

/// Character device (future)
pub const S_IFCHR: u32 = 0o020000;

/// Named pipe/FIFO (future)
pub const S_IFIFO: u32 = 0o010000;

/// Socket (future)
pub const S_IFSOCK: u32 = 0o140000;

/// Mask to extract file type
pub const S_IFMT: u32 = 0o170000;

// ============================================================================
// COMPILE-TIME CHECKS
// ============================================================================

const _: () = assert!(core::mem::size_of::<Superblock>() == SUPERBLOCK_SIZE);
