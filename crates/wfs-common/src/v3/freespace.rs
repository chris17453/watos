//! Free Space Management
//!
//! The free space tree tracks available blocks on disk.
//! It's a B+tree mapping (starting_block) -> (range_length).
//! This allows efficient allocation of contiguous block ranges.

#[cfg(feature = "std")]
use std::vec::Vec;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::structures::BLOCK_SIZE;

// ============================================================================
// CONSTANTS
// ============================================================================

/// Free range entry size
pub const FREE_RANGE_SIZE: usize = 24;

/// Maximum free ranges per tree node
pub const FREE_RANGES_PER_NODE: usize = super::node::NODE_DATA_SIZE / FREE_RANGE_SIZE;

// ============================================================================
// FREE RANGE STRUCTURE
// ============================================================================

/// A range of free blocks on disk
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FreeRange {
    /// Starting block number
    pub start_block: u64,
    /// Number of blocks in this range
    pub block_count: u64,
    /// Generation when freed (for lazy cleanup)
    pub freed_generation: u64,
}

impl FreeRange {
    /// Create a new free range
    pub fn new(start_block: u64, block_count: u64) -> Self {
        Self {
            start_block,
            block_count,
            freed_generation: 0,
        }
    }

    /// End block (exclusive)
    pub fn end_block(&self) -> u64 {
        self.start_block + self.block_count
    }

    /// Size in bytes
    pub fn size_bytes(&self) -> u64 {
        self.block_count * BLOCK_SIZE as u64
    }

    /// Check if this range can satisfy an allocation request
    pub fn can_allocate(&self, blocks_needed: u64) -> bool {
        self.block_count >= blocks_needed
    }

    /// Check if this range is adjacent to another (for merging)
    pub fn is_adjacent(&self, other: &FreeRange) -> bool {
        self.end_block() == other.start_block || other.end_block() == self.start_block
    }

    /// Merge with an adjacent range
    pub fn merge(&self, other: &FreeRange) -> Option<FreeRange> {
        if self.end_block() == other.start_block {
            // self is before other
            Some(FreeRange {
                start_block: self.start_block,
                block_count: self.block_count + other.block_count,
                freed_generation: self.freed_generation.max(other.freed_generation),
            })
        } else if other.end_block() == self.start_block {
            // other is before self
            Some(FreeRange {
                start_block: other.start_block,
                block_count: self.block_count + other.block_count,
                freed_generation: self.freed_generation.max(other.freed_generation),
            })
        } else {
            None
        }
    }

    /// Allocate blocks from this range
    ///
    /// Returns (allocated_start, remaining_range) or None if not enough blocks.
    pub fn allocate(&self, blocks_needed: u64) -> Option<(u64, Option<FreeRange>)> {
        if self.block_count < blocks_needed {
            return None;
        }

        let allocated_start = self.start_block;

        if self.block_count == blocks_needed {
            // Exact fit, no remaining range
            Some((allocated_start, None))
        } else {
            // Split the range
            let remaining = FreeRange {
                start_block: self.start_block + blocks_needed,
                block_count: self.block_count - blocks_needed,
                freed_generation: self.freed_generation,
            };
            Some((allocated_start, Some(remaining)))
        }
    }

    /// Allocate blocks from a specific offset within this range
    ///
    /// For aligned allocations (e.g., chunk-aligned).
    pub fn allocate_at(&self, offset: u64, blocks_needed: u64) -> Option<(u64, Vec<FreeRange>)> {
        if offset < self.start_block || offset + blocks_needed > self.end_block() {
            return None;
        }

        let mut remaining = Vec::new();

        // Left portion (before allocation)
        if offset > self.start_block {
            remaining.push(FreeRange {
                start_block: self.start_block,
                block_count: offset - self.start_block,
                freed_generation: self.freed_generation,
            });
        }

        // Right portion (after allocation)
        let alloc_end = offset + blocks_needed;
        if alloc_end < self.end_block() {
            remaining.push(FreeRange {
                start_block: alloc_end,
                block_count: self.end_block() - alloc_end,
                freed_generation: self.freed_generation,
            });
        }

        Some((offset, remaining))
    }
}

impl Default for FreeRange {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

// ============================================================================
// FREE SPACE ALLOCATOR
// ============================================================================

/// Allocation strategy
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AllocStrategy {
    /// First fit - find first range that can satisfy request
    FirstFit,
    /// Best fit - find smallest range that can satisfy request
    BestFit,
    /// Chunk aligned - align to chunk boundaries for CoW
    ChunkAligned,
}

/// Result of an allocation attempt
#[derive(Clone, Copy, Debug)]
pub struct AllocResult {
    /// Starting block of allocation
    pub start_block: u64,
    /// Number of blocks allocated
    pub block_count: u64,
    /// Whether this was chunk-aligned
    pub chunk_aligned: bool,
}

impl AllocResult {
    pub fn new(start_block: u64, block_count: u64) -> Self {
        Self {
            start_block,
            block_count,
            chunk_aligned: false,
        }
    }
}

// ============================================================================
// PENDING FREE LIST
// ============================================================================

/// A block pending to be freed (after commit)
///
/// In CoW, we can't free blocks until we're sure the new tree is committed.
/// Pending frees are processed after successful commit.
#[derive(Clone, Copy, Debug)]
pub struct PendingFree {
    /// Block to free
    pub block: u64,
    /// Number of blocks
    pub count: u64,
    /// Transaction that freed this block
    pub transaction_id: u64,
}

impl PendingFree {
    pub fn new(block: u64, count: u64, transaction_id: u64) -> Self {
        Self {
            block,
            count,
            transaction_id,
        }
    }

    /// Convert to FreeRange
    pub fn to_range(&self) -> FreeRange {
        FreeRange::new(self.block, self.count)
    }
}

// ============================================================================
// COMPILE-TIME CHECKS
// ============================================================================

const _: () = assert!(core::mem::size_of::<FreeRange>() == FREE_RANGE_SIZE);
