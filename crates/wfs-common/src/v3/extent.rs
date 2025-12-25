//! Extent Structure
//!
//! Extents map file offsets to disk blocks.
//! Each file has an extent tree (or inline data for small files).
//! Extents enable efficient storage of contiguous blocks and support CoW.

use super::structures::{BLOCK_SIZE, DEFAULT_CHUNK_SIZE};

// ============================================================================
// CONSTANTS
// ============================================================================

/// Extent size on disk
pub const EXTENT_SIZE: usize = 40;

/// Maximum extents per tree node
pub const EXTENTS_PER_NODE: usize = super::node::NODE_DATA_SIZE / EXTENT_SIZE;

// ============================================================================
// EXTENT FLAGS
// ============================================================================

/// Extent data is shared (CoW - don't modify in place)
pub const EXTENT_SHARED: u32 = 0x0001;

/// Extent data is compressed (future)
pub const EXTENT_COMPRESSED: u32 = 0x0002;

/// Extent data is encrypted (future)
pub const EXTENT_ENCRYPTED: u32 = 0x0004;

/// Extent is a hole (sparse, no disk blocks allocated)
pub const EXTENT_HOLE: u32 = 0x0008;

/// Extent is preallocated but unwritten
pub const EXTENT_UNWRITTEN: u32 = 0x0010;

// ============================================================================
// EXTENT STRUCTURE
// ============================================================================

/// Extent - maps a range of file bytes to disk blocks
///
/// Stored in extent trees, keyed by file_offset.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Extent {
    // File position (8 bytes)
    pub file_offset: u64,              // Offset within file (in bytes)

    // Disk location (8 bytes)
    pub disk_block: u64,               // Starting block on disk

    // Size (8 bytes)
    pub length: u64,                   // Length in bytes

    // Metadata (8 bytes)
    pub flags: u32,                    // Extent flags
    pub refcount: u32,                 // Reference count for CoW sharing

    // Integrity (8 bytes)
    pub crc32: u32,                    // Checksum of extent data (optional)
    pub generation: u32,               // Generation when written
}

impl Extent {
    /// Create a new extent
    pub fn new(file_offset: u64, disk_block: u64, length: u64) -> Self {
        Self {
            file_offset,
            disk_block,
            length,
            flags: 0,
            refcount: 1,
            crc32: 0,
            generation: 0,
        }
    }

    /// Create a hole extent (sparse, no disk allocation)
    pub fn hole(file_offset: u64, length: u64) -> Self {
        Self {
            file_offset,
            disk_block: 0,
            length,
            flags: EXTENT_HOLE,
            refcount: 0,
            crc32: 0,
            generation: 0,
        }
    }

    /// Check if this is a hole (sparse region)
    pub fn is_hole(&self) -> bool {
        self.flags & EXTENT_HOLE != 0
    }

    /// Check if this extent is shared (CoW)
    pub fn is_shared(&self) -> bool {
        self.refcount > 1 || (self.flags & EXTENT_SHARED != 0)
    }

    /// Number of disk blocks this extent covers
    pub fn block_count(&self) -> u64 {
        (self.length + BLOCK_SIZE as u64 - 1) / BLOCK_SIZE as u64
    }

    /// Number of chunks this extent covers
    pub fn chunk_count(&self) -> u64 {
        (self.length + DEFAULT_CHUNK_SIZE as u64 - 1) / DEFAULT_CHUNK_SIZE as u64
    }

    /// End offset in file (exclusive)
    pub fn file_end(&self) -> u64 {
        self.file_offset + self.length
    }

    /// End block on disk (exclusive)
    pub fn disk_end(&self) -> u64 {
        self.disk_block + self.block_count()
    }

    /// Check if a file offset falls within this extent
    pub fn contains_offset(&self, offset: u64) -> bool {
        offset >= self.file_offset && offset < self.file_end()
    }

    /// Get disk block for a file offset
    pub fn offset_to_block(&self, file_off: u64) -> Option<u64> {
        if !self.contains_offset(file_off) {
            return None;
        }
        if self.is_hole() {
            return None; // Holes don't have disk blocks
        }
        let offset_in_extent = file_off - self.file_offset;
        let block_in_extent = offset_in_extent / BLOCK_SIZE as u64;
        Some(self.disk_block + block_in_extent)
    }

    /// Split extent at a file offset
    ///
    /// Returns (left_extent, right_extent) where left ends at split_offset
    /// and right starts at split_offset.
    pub fn split_at(&self, split_offset: u64) -> Option<(Extent, Extent)> {
        if split_offset <= self.file_offset || split_offset >= self.file_end() {
            return None; // Split point outside extent
        }

        let left_length = split_offset - self.file_offset;
        let right_length = self.length - left_length;
        let left_blocks = (left_length + BLOCK_SIZE as u64 - 1) / BLOCK_SIZE as u64;

        let left = Extent {
            file_offset: self.file_offset,
            disk_block: self.disk_block,
            length: left_length,
            flags: self.flags,
            refcount: self.refcount,
            crc32: 0, // CRC needs recalc
            generation: self.generation,
        };

        let right = Extent {
            file_offset: split_offset,
            disk_block: if self.is_hole() { 0 } else { self.disk_block + left_blocks },
            length: right_length,
            flags: self.flags,
            refcount: self.refcount,
            crc32: 0, // CRC needs recalc
            generation: self.generation,
        };

        Some((left, right))
    }

    /// Increment reference count (for CoW)
    pub fn inc_refcount(&mut self) {
        self.refcount = self.refcount.saturating_add(1);
        if self.refcount > 1 {
            self.flags |= EXTENT_SHARED;
        }
    }

    /// Decrement reference count
    pub fn dec_refcount(&mut self) -> u32 {
        self.refcount = self.refcount.saturating_sub(1);
        if self.refcount <= 1 {
            self.flags &= !EXTENT_SHARED;
        }
        self.refcount
    }

    /// Check if extents are adjacent and can be merged
    pub fn can_merge_with(&self, other: &Extent) -> bool {
        // Can't merge holes with non-holes
        if self.is_hole() != other.is_hole() {
            return false;
        }
        // Can't merge shared extents
        if self.is_shared() || other.is_shared() {
            return false;
        }
        // Check if adjacent in file
        if self.file_end() != other.file_offset {
            return false;
        }
        // For non-holes, check if adjacent on disk
        if !self.is_hole() && self.disk_end() != other.disk_block {
            return false;
        }
        true
    }

    /// Merge with adjacent extent
    pub fn merge_with(&self, other: &Extent) -> Option<Extent> {
        if !self.can_merge_with(other) {
            return None;
        }
        Some(Extent {
            file_offset: self.file_offset,
            disk_block: self.disk_block,
            length: self.length + other.length,
            flags: self.flags,
            refcount: 1, // Merged extent is exclusive
            crc32: 0,
            generation: self.generation.max(other.generation),
        })
    }
}

impl Default for Extent {
    fn default() -> Self {
        Self::new(0, 0, 0)
    }
}

// ============================================================================
// EXTENT TREE KEY
// ============================================================================

/// Key for extent tree (file offset)
pub type ExtentKey = u64;

/// Get the extent tree key for a file offset
pub fn extent_key(file_offset: u64) -> ExtentKey {
    file_offset
}

// ============================================================================
// COMPILE-TIME CHECKS
// ============================================================================

const _: () = assert!(core::mem::size_of::<Extent>() == EXTENT_SIZE);
