//! WFS - WATOS File System
//!
//! A modern Copy-on-Write filesystem with B+tree indexing designed for WATOS.
//!
//! ## Features
//!
//! - **Copy-on-Write (CoW)**: Atomic transactions, crash-safe without journaling
//! - **B+Tree Indexing**: O(log n) directory lookups, supports millions of files
//! - **Extent-Based Storage**: Efficient large file storage
//! - **Checksums**: CRC32 on all metadata and data blocks
//! - **Snapshots**: Foundation for snapshot support
//! - **Compression/Encryption**: Ready for future extensions
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────┐
//! │           Superblock (Block 0 & 1)              │
//! │  - Magic: "WFS1" (0x57465331)                   │
//! │  - Root tree pointer (atomic CoW)               │
//! │  - Free space tree pointer                      │
//! │  - Generation counter                           │
//! └─────────────────────────────────────────────────┘
//!                       │
//!      ┌────────────────┼────────────────┐
//!      │                │                │
//!   Inode Tree     Directory Tree   Free Space Tree
//!   (B+Tree)        (B+Tree)         (B+Tree)
//!      │                │                │
//!   Inodes          Dir Entries      Free Ranges
//!      │
//!   Extent Tree
//!   (per file)
//!      │
//!   Data Blocks
//! ```
//!
//! ## Disk Layout
//!
//! ```text
//! Block 0:        Superblock (primary)
//! Block 1:        Superblock (backup)
//! Block 2+:       Tree nodes (B+tree nodes for metadata)
//! Block N+:       Data blocks (actual file content)
//! ```
//!
//! ## Transaction Model
//!
//! WFS uses Copy-on-Write for all modifications:
//!
//! 1. **Begin**: Clone affected tree nodes
//! 2. **Modify**: Update cloned nodes
//! 3. **Commit**: Atomically update superblock root pointer
//! 4. **Cleanup**: Defer free of old blocks (GC)
//!
//! A crash at any point leaves filesystem consistent:
//! - Before commit: Old data visible
//! - After commit: New data visible
//! - No intermediate corrupt state possible
//!
//! ## Performance Characteristics
//!
//! | Operation | Time Complexity | Notes |
//! |-----------|-----------------|-------|
//! | File lookup | O(log n) | B+tree directory |
//! | Sequential read | O(1) per block | Extent-based |
//! | Random write | O(log n) | CoW overhead |
//! | Directory list | O(n) sorted | B+tree iteration |
//! | Snapshot | O(1) | Just clone root pointer |
//!
//! ## Scalability
//!
//! With 64-bit addressing and 4KB blocks:
//! - **Max filesystem size**: 16 EiB (exbibytes)
//! - **Max file count**: Billions (limited only by disk space)
//! - **Max file size**: 16 EiB
//! - **Max directory entries**: Unlimited

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;

/// Prelude module for no_std/std compatibility
pub mod prelude {
    pub use core::option::Option::{self, None, Some};
    pub use core::result::Result::{self, Err, Ok};
    pub use core::default::Default;
    pub use core::clone::Clone;
    pub use core::marker::{Copy, Send, Sync};
    pub use core::cmp::{Eq, Ord, PartialEq, PartialOrd};
    pub use core::fmt::Debug;
    pub use core::convert::{From, Into};
    pub use core::iter::Iterator;

    #[cfg(feature = "std")]
    pub use std::{vec::Vec, vec, string::String, boxed::Box};

    #[cfg(not(feature = "std"))]
    pub use alloc::{vec::Vec, vec, string::String, boxed::Box};
}

#[allow(unused_imports)]
use prelude::*;

// Core WFS implementation (formerly v3, now the only version)
pub mod core;

// VFS adapter (optional, enabled with "vfs" feature)
#[cfg(feature = "vfs")]
pub mod vfs_adapter;

#[cfg(feature = "vfs")]
pub use vfs_adapter::WfsFilesystem;

// Re-export commonly used types from core
pub use core::{
    // Structures
    Superblock, Inode, TreeNode, NodeType, Extent, DirEntry,

    // Tree operations
    BPlusTree, TreeOps, TreeKey, TreeValue, TreeError,

    // Filesystem operations
    FilesystemState, BlockDevice, BlockAllocator,
    InodeOps, DirOps, ExtentOps,

    // Constants
    WFS_MAGIC, WFS_VERSION, BLOCK_SIZE, ROOT_INODE,
    WFS_SIGNATURE, MAX_FILENAME, INLINE_DATA_MAX,

    // Flags
    S_IFREG, S_IFDIR, S_IFLNK, S_IFMT,
    INODE_IMMUTABLE, INODE_APPEND, INODE_NOATIME,
};

/// CRC32 calculation (same algorithm as used throughout WFS)
pub fn crc32(data: &[u8]) -> u32 {
    const POLY: u32 = 0xEDB88320;
    let mut crc: u32 = 0xFFFFFFFF;

    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ POLY
            } else {
                crc >> 1
            };
        }
    }

    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc32() {
        // Standard CRC32 test vectors
        assert_eq!(crc32(b""), 0x00000000);
        assert_eq!(crc32(b"a"), 0xE8B7BE43);
        assert_eq!(crc32(b"abc"), 0x352441C2);
        assert_eq!(crc32(b"message digest"), 0x20159D7F);
    }
}
