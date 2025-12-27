//! WFS v3 - Copy-on-Write Filesystem
//!
//! A modern CoW filesystem with:
//! - Atomic transactions via root pointer swap
//! - B+tree indexed directories
//! - Extent-based file storage with chunk-level CoW
//! - Crash-safe without journal
//! - Checksums on all metadata

#[allow(unused_imports)]
use crate::prelude::*;

pub mod structures;
pub mod node;
pub mod inode;
pub mod dir;
pub mod extent;
pub mod freespace;
pub mod tree;
pub mod transaction;
pub mod fs;

#[cfg(test)]
mod tests;

// Re-export commonly used items
pub use structures::*;
pub use node::{TreeNode, NodeType};
pub use inode::Inode;
pub use dir::DirEntry;
pub use extent::Extent;
pub use freespace::FreeRange;
pub use tree::{BPlusTree, BlockDevice, BlockAllocator, TreeOps, TreeError, SearchResult, TreePath, TreeKey, TreeValue};
pub use transaction::{Transaction, TransactionState, TransactionManager, TransactionError};
pub use fs::{FilesystemState, FilesystemOps, InodeOps, DirOps, ExtentOps, FileOps, FreeSpaceOps, init_filesystem, resolve_path};
