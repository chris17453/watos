//! B+Tree Node Format
//!
//! Tree nodes are the fundamental building blocks of WFS v3.
//! All metadata (inodes, directories, extents, free space) is stored in B+trees.

use crate::crc32;
use super::structures::BLOCK_SIZE;

// ============================================================================
// CONSTANTS
// ============================================================================

/// Node header size
pub const NODE_HEADER_SIZE: usize = 32;

/// Data area size (block size minus header)
pub const NODE_DATA_SIZE: usize = BLOCK_SIZE as usize - NODE_HEADER_SIZE;

/// Magic numbers for different node types
pub const NODE_MAGIC_INODE: u32 = 0x494E4F44;     // "INOD"
pub const NODE_MAGIC_DIR: u32 = 0x44495245;       // "DIRE"
pub const NODE_MAGIC_EXTENT: u32 = 0x45585445;    // "EXTE"
pub const NODE_MAGIC_FREE: u32 = 0x46524545;      // "FREE"

// ============================================================================
// NODE TYPE
// ============================================================================

/// Type of tree node
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeType {
    /// Inode tree node (maps inode_num -> Inode)
    Inode = NODE_MAGIC_INODE,
    /// Directory tree node (maps name -> inode_num)
    Directory = NODE_MAGIC_DIR,
    /// Extent tree node (maps file_offset -> disk_block)
    Extent = NODE_MAGIC_EXTENT,
    /// Free space tree node (maps block_num -> range_length)
    FreeSpace = NODE_MAGIC_FREE,
}

impl NodeType {
    pub fn from_magic(magic: u32) -> Option<Self> {
        match magic {
            NODE_MAGIC_INODE => Some(NodeType::Inode),
            NODE_MAGIC_DIR => Some(NodeType::Directory),
            NODE_MAGIC_EXTENT => Some(NodeType::Extent),
            NODE_MAGIC_FREE => Some(NodeType::FreeSpace),
            _ => None,
        }
    }
}

// ============================================================================
// TREE NODE
// ============================================================================

/// B+Tree node - 4096 bytes on disk
///
/// Internal nodes store (key, child_block) pairs.
/// Leaf nodes store (key, value) pairs where value depends on tree type.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TreeNode {
    // Header (32 bytes)
    pub magic: u32,                    // Node type magic
    pub level: u16,                    // 0 = leaf, >0 = internal
    pub item_count: u16,               // Number of items in node
    pub generation: u64,               // Generation when written
    pub parent_block: u64,             // Parent node block (0 if root)
    pub crc32: u32,                    // Checksum of node
    pub _pad: u32,

    // Data area - interpretation depends on node type and level
    // For internal nodes: [key0, ptr0, key1, ptr1, ...]
    // For leaf nodes: [key0, value0, key1, value1, ...]
    pub data: [u8; NODE_DATA_SIZE],
}

impl TreeNode {
    /// Create a new empty tree node
    pub fn new(node_type: NodeType, level: u16, generation: u64) -> Self {
        Self {
            magic: node_type as u32,
            level,
            item_count: 0,
            generation,
            parent_block: 0,
            crc32: 0,
            _pad: 0,
            data: [0; NODE_DATA_SIZE],
        }
    }

    /// Create an empty leaf node
    pub fn new_leaf(node_type: NodeType, generation: u64) -> Self {
        Self::new(node_type, 0, generation)
    }

    /// Create an empty internal node
    pub fn new_internal(node_type: NodeType, level: u16, generation: u64) -> Self {
        assert!(level > 0, "Internal nodes must have level > 0");
        Self::new(node_type, level, generation)
    }

    /// Check if this is a leaf node
    pub fn is_leaf(&self) -> bool {
        self.level == 0
    }

    /// Check if this is an internal node
    pub fn is_internal(&self) -> bool {
        self.level > 0
    }

    /// Get the node type
    pub fn node_type(&self) -> Option<NodeType> {
        NodeType::from_magic(self.magic)
    }

    /// Calculate CRC of node data
    pub fn calculate_crc(&self) -> u32 {
        // CRC everything except the crc32 field itself
        // Header is 32 bytes, crc32 is at offset 24
        let mut bytes = [0u8; BLOCK_SIZE as usize - 4];
        let self_bytes = unsafe {
            core::slice::from_raw_parts(self as *const _ as *const u8, BLOCK_SIZE as usize)
        };
        // Copy bytes 0-23 (before crc32)
        bytes[..24].copy_from_slice(&self_bytes[..24]);
        // Skip crc32 (bytes 24-27), copy pad (bytes 28-31) and data
        bytes[24..].copy_from_slice(&self_bytes[28..]);
        crc32(&bytes)
    }

    /// Update the CRC
    pub fn update_crc(&mut self) {
        self.crc32 = self.calculate_crc();
    }

    /// Verify CRC
    pub fn verify_crc(&self) -> bool {
        self.crc32 == self.calculate_crc()
    }

    /// Check if node is valid
    pub fn is_valid(&self) -> bool {
        self.node_type().is_some() && self.verify_crc()
    }

    /// Get maximum items for this node type
    /// This depends on the key/value sizes which vary by tree type
    pub fn max_items(&self, key_size: usize, value_size: usize) -> usize {
        NODE_DATA_SIZE / (key_size + value_size)
    }

    /// Check if node is full
    pub fn is_full(&self, key_size: usize, value_size: usize) -> bool {
        self.item_count as usize >= self.max_items(key_size, value_size)
    }

    /// Check if node needs split (more than half full is typical threshold)
    pub fn needs_split(&self, key_size: usize, value_size: usize) -> bool {
        self.item_count as usize > self.max_items(key_size, value_size) / 2
    }
}

impl Default for TreeNode {
    fn default() -> Self {
        Self::new(NodeType::Inode, 0, 0)
    }
}

// ============================================================================
// KEY-VALUE HELPERS
// ============================================================================

/// A key-value pair in a tree node
#[derive(Clone, Copy, Debug)]
pub struct KeyValue<K, V> {
    pub key: K,
    pub value: V,
}

/// Internal node entry (key + child pointer)
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct InternalEntry {
    pub key: u64,           // Key (interpretation depends on tree type)
    pub child_block: u64,   // Block number of child node
}

impl InternalEntry {
    pub const SIZE: usize = 16;
}

/// Maximum internal entries per node
pub const MAX_INTERNAL_ENTRIES: usize = NODE_DATA_SIZE / InternalEntry::SIZE;

// ============================================================================
// COMPILE-TIME CHECKS
// ============================================================================

const _: () = assert!(core::mem::size_of::<TreeNode>() == BLOCK_SIZE as usize);
const _: () = assert!(NODE_HEADER_SIZE == 32);
const _: () = assert!(NODE_DATA_SIZE == 4064);
