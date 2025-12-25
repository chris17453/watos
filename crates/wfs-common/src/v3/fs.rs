//! WFS v3 Filesystem Operations
//!
//! High-level filesystem operations that use transactions and CoW.
//! This module provides the core filesystem logic shared between
//! kernel driver and userspace tools.

#[cfg(feature = "std")]
use std::vec::Vec;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::structures::{SuperblockV3, ROOT_INODE, WFS3_MAGIC};
use super::node::{TreeNode, NodeType};
use super::inode::{Inode, INODE_SIZE};
use super::dir::DirEntry;
use super::extent::{Extent, EXTENT_SIZE};
use super::tree::{BPlusTree, BlockDevice, BlockAllocator, TreeOps, TreeError, TreeValue};
use super::transaction::{Transaction, TransactionError};

// ============================================================================
// FILESYSTEM STATE
// ============================================================================

/// Filesystem state - tracks current superblock and active transaction
#[derive(Clone, Debug)]
pub struct FilesystemState {
    /// Current superblock
    pub superblock: SuperblockV3,
    /// Inode tree
    pub inode_tree: BPlusTree,
    /// Free space tree
    pub free_tree: BPlusTree,
    /// Active transaction (if any)
    pub transaction: Option<Transaction>,
    /// Next transaction ID
    pub next_txn_id: u64,
}

impl FilesystemState {
    /// Create a new filesystem state from a superblock
    pub fn new(superblock: SuperblockV3) -> Self {
        let inode_tree = BPlusTree::new(
            superblock.root_tree_block,
            NodeType::Inode,
            superblock.root_generation,
        );
        let free_tree = BPlusTree::new(
            superblock.free_tree_block,
            NodeType::FreeSpace,
            superblock.root_generation,
        );

        Self {
            superblock,
            inode_tree,
            free_tree,
            transaction: None,
            next_txn_id: 1,
        }
    }

    /// Check if a transaction is active
    pub fn has_active_transaction(&self) -> bool {
        self.transaction.as_ref().map_or(false, |t| t.is_active())
    }

    /// Get the current root block
    pub fn root_block(&self) -> u64 {
        if let Some(ref txn) = self.transaction {
            txn.working_root
        } else {
            self.superblock.root_tree_block
        }
    }
}

// ============================================================================
// FILESYSTEM OPERATIONS TRAIT
// ============================================================================

/// Trait for filesystem operations
///
/// Implemented by kernel driver and userspace tools with different
/// block device backends.
pub trait FilesystemOps: BlockDevice + BlockAllocator {
    /// Begin a new transaction
    fn begin_transaction(&mut self, state: &mut FilesystemState) -> Result<(), TransactionError> {
        if state.has_active_transaction() {
            return Err(TransactionError::TransactionAlreadyActive);
        }

        let txn_id = state.next_txn_id;
        state.next_txn_id += 1;

        let generation = state.superblock.root_generation + 1;
        let current_root = state.superblock.root_tree_block;

        state.transaction = Some(Transaction::new(txn_id, generation, current_root));
        Ok(())
    }

    /// Commit the current transaction
    ///
    /// This atomically updates the superblock to point to the new tree root.
    fn commit_transaction(&mut self, state: &mut FilesystemState) -> Result<(), TransactionError> {
        let txn = state.transaction.as_mut().ok_or(TransactionError::NoActiveTransaction)?;

        if !txn.is_active() {
            return Err(TransactionError::InvalidState);
        }

        // Begin commit
        txn.begin_commit();

        // Update superblock with new root
        state.superblock.root_tree_block = txn.working_root;
        state.superblock.root_generation = txn.generation;
        state.superblock.update_crc();

        // Write primary superblock (block 0)
        self.write_superblock(0, &state.superblock)?;

        // Sync to disk
        BlockDevice::sync(self).map_err(|_| TransactionError::IoError)?;

        // Write backup superblock (block 1)
        self.write_superblock(1, &state.superblock)?;

        // Complete commit
        txn.complete_commit();

        // Process pending frees (add blocks back to free tree)
        // This is safe because the new tree is now committed
        let pending_frees = txn.pending_frees.clone();
        for pf in pending_frees {
            // Add freed blocks back to free space tree
            self.free_blocks(state, pf.block, pf.count)?;
        }

        // Clear transaction
        state.transaction = None;
        state.inode_tree.root_block = state.superblock.root_tree_block;
        state.inode_tree.generation = state.superblock.root_generation;

        Ok(())
    }

    /// Abort the current transaction
    fn abort_transaction(&mut self, state: &mut FilesystemState) -> Result<(), TransactionError> {
        let txn = state.transaction.as_mut().ok_or(TransactionError::NoActiveTransaction)?;

        if !txn.is_active() {
            return Err(TransactionError::InvalidState);
        }

        // Mark as aborted
        txn.abort();

        // Return all allocated blocks to free space
        let allocated = txn.allocated_blocks.clone();
        for block in allocated {
            self.free_block(block).ok();
        }

        // Clear transaction
        state.transaction = None;

        Ok(())
    }

    /// Write a superblock to disk
    fn write_superblock(&mut self, block: u64, sb: &SuperblockV3) -> Result<(), TransactionError> {
        // Convert superblock to node (same size as block)
        let mut node = TreeNode::default();
        let sb_bytes = unsafe {
            core::slice::from_raw_parts(sb as *const _ as *const u8, core::mem::size_of::<SuperblockV3>())
        };
        node.data[..sb_bytes.len()].copy_from_slice(sb_bytes);

        self.write_node(block, &node).map_err(|_| TransactionError::IoError)
    }

    /// Read a superblock from disk
    fn read_superblock(&self, block: u64) -> Result<SuperblockV3, TransactionError> {
        let node = self.read_node(block).map_err(|_| TransactionError::IoError)?;

        let sb = unsafe {
            core::ptr::read(node.data.as_ptr() as *const SuperblockV3)
        };

        if sb.magic != WFS3_MAGIC {
            return Err(TransactionError::IoError);
        }

        if !sb.verify_crc() {
            return Err(TransactionError::IoError);
        }

        Ok(sb)
    }

    /// Allocate blocks from free space
    fn allocate_blocks(&mut self, state: &mut FilesystemState, count: u64) -> Result<u64, TransactionError>;

    /// Free blocks (add to free space)
    fn free_blocks(&mut self, state: &mut FilesystemState, start: u64, count: u64) -> Result<(), TransactionError>;
}

// ============================================================================
// INODE OPERATIONS
// ============================================================================

/// Inode operations using B+tree
pub struct InodeOps;

impl InodeOps {
    /// Lookup an inode by number
    pub fn lookup<D: BlockDevice, A: BlockAllocator>(
        ops: &mut TreeOps<D, A>,
        state: &FilesystemState,
        inode_num: u64,
    ) -> Result<Option<Inode>, TreeError> {
        let tree = &state.inode_tree;
        let (result, _path) = ops.search::<u64, InodeValue>(tree, &inode_num)?;

        match result.value() {
            Some(iv) => Ok(Some(iv.0)),
            None => Ok(None),
        }
    }

    /// Insert or update an inode
    pub fn insert<D: BlockDevice, A: BlockAllocator>(
        ops: &mut TreeOps<D, A>,
        state: &mut FilesystemState,
        inode: Inode,
    ) -> Result<(), TreeError> {
        let inode_num = inode.inode_num;

        // Try to delete existing (if update)
        let _ = ops.delete::<u64, InodeValue>(&mut state.inode_tree, &inode_num);

        // Insert new
        ops.insert(&mut state.inode_tree, inode_num, InodeValue(inode))?;

        // Update transaction
        if let Some(ref mut txn) = state.transaction {
            txn.record_inode_modification(inode_num);
            txn.working_root = state.inode_tree.root_block;
        }

        Ok(())
    }

    /// Delete an inode
    pub fn delete<D: BlockDevice, A: BlockAllocator>(
        ops: &mut TreeOps<D, A>,
        state: &mut FilesystemState,
        inode_num: u64,
    ) -> Result<Inode, TreeError> {
        let result = ops.delete::<u64, InodeValue>(&mut state.inode_tree, &inode_num)?;

        // Update transaction
        if let Some(ref mut txn) = state.transaction {
            txn.record_inode_modification(inode_num);
            txn.working_root = state.inode_tree.root_block;
        }

        Ok(result.0)
    }

    /// Allocate a new inode number
    pub fn allocate_inode_num(state: &mut FilesystemState) -> u64 {
        let num = state.superblock.next_inode;
        state.superblock.next_inode += 1;
        state.superblock.inode_count += 1;
        num
    }
}

// ============================================================================
// INODE VALUE WRAPPER
// ============================================================================

/// Wrapper for Inode to implement TreeValue
#[derive(Clone, Copy)]
pub struct InodeValue(pub Inode);

impl TreeValue for InodeValue {
    fn serialized_size(&self) -> usize {
        INODE_SIZE
    }

    fn serialize(&self, buf: &mut [u8]) -> usize {
        if buf.len() >= INODE_SIZE {
            let bytes = unsafe {
                core::slice::from_raw_parts(&self.0 as *const _ as *const u8, INODE_SIZE)
            };
            buf[..INODE_SIZE].copy_from_slice(bytes);
            INODE_SIZE
        } else {
            0
        }
    }

    fn deserialize(buf: &[u8]) -> Option<(Self, usize)> {
        if buf.len() >= INODE_SIZE {
            let inode = unsafe {
                core::ptr::read(buf.as_ptr() as *const Inode)
            };
            Some((InodeValue(inode), INODE_SIZE))
        } else {
            None
        }
    }
}

// ============================================================================
// DIRECTORY OPERATIONS
// ============================================================================

/// Directory operations
pub struct DirOps;

impl DirOps {
    /// Lookup a directory entry by name
    pub fn lookup<D: BlockDevice, A: BlockAllocator>(
        ops: &mut TreeOps<D, A>,
        dir_tree: &BPlusTree,
        name: &str,
    ) -> Result<Option<DirEntry>, TreeError> {
        let hash = DirEntry::hash_name(name);
        let (result, _) = ops.search::<u64, DirEntryValue>(dir_tree, &hash)?;

        match result.value() {
            Some(dev) => {
                // Verify name matches (handle hash collisions)
                if dev.0.matches(name) {
                    Ok(Some(dev.0))
                } else {
                    // Hash collision - would need to scan for exact match
                    // For simplicity, return None (full implementation would handle this)
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    /// Insert a directory entry
    pub fn insert<D: BlockDevice, A: BlockAllocator>(
        ops: &mut TreeOps<D, A>,
        dir_tree: &mut BPlusTree,
        entry: DirEntry,
    ) -> Result<(), TreeError> {
        let hash = entry.name_hash;
        ops.insert(dir_tree, hash, DirEntryValue(entry))?;
        Ok(())
    }

    /// Delete a directory entry
    pub fn delete<D: BlockDevice, A: BlockAllocator>(
        ops: &mut TreeOps<D, A>,
        dir_tree: &mut BPlusTree,
        name: &str,
    ) -> Result<DirEntry, TreeError> {
        let hash = DirEntry::hash_name(name);
        let result = ops.delete::<u64, DirEntryValue>(dir_tree, &hash)?;
        Ok(result.0)
    }
}

// ============================================================================
// DIRECTORY ENTRY VALUE WRAPPER
// ============================================================================

/// Wrapper for DirEntry to implement TreeValue
#[derive(Clone, Copy)]
pub struct DirEntryValue(pub DirEntry);

impl TreeValue for DirEntryValue {
    fn serialized_size(&self) -> usize {
        super::dir::DIRENTRY_SIZE
    }

    fn serialize(&self, buf: &mut [u8]) -> usize {
        let size = super::dir::DIRENTRY_SIZE;
        if buf.len() >= size {
            let bytes = unsafe {
                core::slice::from_raw_parts(&self.0 as *const _ as *const u8, size)
            };
            buf[..size].copy_from_slice(bytes);
            size
        } else {
            0
        }
    }

    fn deserialize(buf: &[u8]) -> Option<(Self, usize)> {
        let size = super::dir::DIRENTRY_SIZE;
        if buf.len() >= size {
            let entry = unsafe {
                core::ptr::read(buf.as_ptr() as *const DirEntry)
            };
            Some((DirEntryValue(entry), size))
        } else {
            None
        }
    }
}

// ============================================================================
// EXTENT OPERATIONS
// ============================================================================

/// Extent operations for file data
pub struct ExtentOps;

/// Wrapper for Extent to implement TreeValue
#[derive(Clone, Copy)]
pub struct ExtentValue(pub Extent);

impl TreeValue for ExtentValue {
    fn serialized_size(&self) -> usize {
        EXTENT_SIZE
    }

    fn serialize(&self, buf: &mut [u8]) -> usize {
        if buf.len() >= EXTENT_SIZE {
            let bytes = unsafe {
                core::slice::from_raw_parts(&self.0 as *const _ as *const u8, EXTENT_SIZE)
            };
            buf[..EXTENT_SIZE].copy_from_slice(bytes);
            EXTENT_SIZE
        } else {
            0
        }
    }

    fn deserialize(buf: &[u8]) -> Option<(Self, usize)> {
        if buf.len() >= EXTENT_SIZE {
            let extent = unsafe {
                core::ptr::read(buf.as_ptr() as *const Extent)
            };
            Some((ExtentValue(extent), EXTENT_SIZE))
        } else {
            None
        }
    }
}

impl ExtentOps {
    /// Lookup an extent containing a file offset
    pub fn lookup<D: BlockDevice, A: BlockAllocator>(
        ops: &mut TreeOps<D, A>,
        extent_tree: &BPlusTree,
        file_offset: u64,
    ) -> Result<Option<Extent>, TreeError> {
        // Search for extent by file offset
        let (result, _) = ops.search::<u64, ExtentValue>(extent_tree, &file_offset)?;

        match result.value() {
            Some(ev) => {
                // Check if offset is within this extent
                if ev.0.contains_offset(file_offset) {
                    Ok(Some(ev.0))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    /// Insert an extent
    pub fn insert<D: BlockDevice, A: BlockAllocator>(
        ops: &mut TreeOps<D, A>,
        extent_tree: &mut BPlusTree,
        extent: Extent,
    ) -> Result<(), TreeError> {
        ops.insert(extent_tree, extent.file_offset, ExtentValue(extent))?;
        Ok(())
    }

    /// Delete an extent
    pub fn delete<D: BlockDevice, A: BlockAllocator>(
        ops: &mut TreeOps<D, A>,
        extent_tree: &mut BPlusTree,
        file_offset: u64,
    ) -> Result<Extent, TreeError> {
        let result = ops.delete::<u64, ExtentValue>(extent_tree, &file_offset)?;
        Ok(result.0)
    }

    /// Allocate blocks for a new extent
    ///
    /// This is the core of file write operations.
    /// For CoW, if the extent is shared, allocate new blocks.
    pub fn allocate_for_write<D: BlockDevice + BlockAllocator>(
        device: &mut D,
        state: &mut FilesystemState,
        file_offset: u64,
        length: u64,
    ) -> Result<Extent, TreeError> {
        // Calculate number of blocks needed
        let block_count = (length + super::structures::BLOCK_SIZE as u64 - 1)
            / super::structures::BLOCK_SIZE as u64;

        // Allocate blocks
        // Note: This is a simplified version - full implementation would
        // use the free space tree for efficient allocation
        let start_block = device.allocate_block()?;

        // Allocate remaining blocks (contiguous if possible)
        for _ in 1..block_count {
            let _ = device.allocate_block()?;
        }

        // Record allocation in transaction
        if let Some(ref mut txn) = state.transaction {
            for i in 0..block_count {
                txn.record_allocation(start_block + i);
            }
        }

        Ok(Extent::new(file_offset, start_block, length))
    }
}

// ============================================================================
// FILE OPERATIONS
// ============================================================================

/// File read/write operations
pub struct FileOps;

impl FileOps {
    /// Read file data at an offset
    ///
    /// Returns the data read and actual bytes read.
    pub fn read<D: BlockDevice, A: BlockAllocator>(
        ops: &mut TreeOps<D, A>,
        device: &D,
        inode: &Inode,
        offset: u64,
        buf: &mut [u8],
    ) -> Result<usize, TreeError> {
        // Check bounds
        if offset >= inode.size {
            return Ok(0);
        }

        let read_len = core::cmp::min(buf.len() as u64, inode.size - offset) as usize;

        // Check for inline data
        if inode.is_inline() {
            let inline_data = inode.get_inline_data();
            if offset < inline_data.len() as u64 {
                let start = offset as usize;
                let copy_len = core::cmp::min(read_len, inline_data.len() - start);
                buf[..copy_len].copy_from_slice(&inline_data[start..start + copy_len]);
                return Ok(copy_len);
            }
            return Ok(0);
        }

        // For extents, we need to traverse the extent tree
        let extent_tree = BPlusTree::from_root(
            inode.extent_root,
            0, // height unknown, but search works
            NodeType::Extent,
            0,
        );

        let mut bytes_read = 0;
        let mut current_offset = offset;

        while bytes_read < read_len {
            // Find extent for current offset
            let extent = ExtentOps::lookup(ops, &extent_tree, current_offset)?;

            match extent {
                Some(ext) if !ext.is_hole() => {
                    // Calculate read position within extent
                    let offset_in_extent = current_offset - ext.file_offset;
                    let extent_remaining = ext.length - offset_in_extent;
                    let to_read = core::cmp::min(
                        (read_len - bytes_read) as u64,
                        extent_remaining,
                    ) as usize;

                    // Calculate disk block
                    let block = ext.offset_to_block(current_offset)
                        .ok_or(TreeError::InvalidNode)?;

                    // Read block from disk
                    let node = device.read_node(block)?;

                    // Copy data from block
                    let block_offset = (offset_in_extent % super::structures::BLOCK_SIZE as u64) as usize;
                    let copy_len = core::cmp::min(
                        to_read,
                        super::structures::BLOCK_SIZE as usize - block_offset,
                    );
                    buf[bytes_read..bytes_read + copy_len]
                        .copy_from_slice(&node.data[block_offset..block_offset + copy_len]);

                    bytes_read += copy_len;
                    current_offset += copy_len as u64;
                }
                Some(_) | None => {
                    // Hole or no extent - return zeros
                    let remaining = read_len - bytes_read;
                    let to_zero = core::cmp::min(remaining, super::structures::BLOCK_SIZE as usize);
                    for i in 0..to_zero {
                        buf[bytes_read + i] = 0;
                    }
                    bytes_read += to_zero;
                    current_offset += to_zero as u64;
                }
            }
        }

        Ok(bytes_read)
    }

    /// Write file data at an offset
    ///
    /// Handles CoW for shared extents.
    pub fn write<D: BlockDevice + BlockAllocator, A: BlockAllocator>(
        ops: &mut TreeOps<D, A>,
        device: &mut D,
        state: &mut FilesystemState,
        inode: &mut Inode,
        offset: u64,
        data: &[u8],
    ) -> Result<usize, TreeError> {
        let write_len = data.len();

        // Check for inline data
        if Inode::can_inline(offset as usize + write_len) && offset == 0 {
            // Can write inline
            inode.set_inline_data(data);
            inode.update_crc();
            InodeOps::insert(ops, state, *inode)?;
            return Ok(write_len);
        }

        // Need to use extents
        // For simplicity, allocate a new extent for this write
        let extent = ExtentOps::allocate_for_write(device, state, offset, write_len as u64)?;

        // Write data to disk blocks
        let mut bytes_written = 0;
        let mut current_block = extent.disk_block;

        while bytes_written < write_len {
            let remaining = write_len - bytes_written;
            let to_write = core::cmp::min(remaining, super::structures::BLOCK_SIZE as usize);

            // Create a node with the data
            let mut node = TreeNode::new_leaf(NodeType::Extent, state.superblock.root_generation);
            node.data[..to_write].copy_from_slice(&data[bytes_written..bytes_written + to_write]);

            // Write to disk
            device.write_node(current_block, &node)?;

            bytes_written += to_write;
            current_block += 1;
        }

        // Insert extent into tree
        let mut extent_tree = BPlusTree::from_root(
            inode.extent_root,
            0,
            NodeType::Extent,
            state.superblock.root_generation,
        );

        if inode.extent_root == 0 {
            // No existing extent tree - this becomes the first extent
        }

        ExtentOps::insert(ops, &mut extent_tree, extent)?;

        // Update inode
        inode.extent_root = extent_tree.root_block;
        if offset + write_len as u64 > inode.size {
            inode.size = offset + write_len as u64;
        }
        inode.blocks = (inode.size + super::structures::BLOCK_SIZE as u64 - 1)
            / super::structures::BLOCK_SIZE as u64;
        inode.update_crc();
        InodeOps::insert(ops, state, *inode)?;

        Ok(write_len)
    }
}

// ============================================================================
// FREE SPACE OPERATIONS
// ============================================================================

/// Free space tree operations
pub struct FreeSpaceOps;

impl FreeSpaceOps {
    /// Find a free range that can satisfy an allocation
    pub fn find_free_range<D: BlockDevice, A: BlockAllocator>(
        ops: &mut TreeOps<D, A>,
        state: &FilesystemState,
        blocks_needed: u64,
    ) -> Result<Option<(u64, u64)>, TreeError> {
        // Search the free space tree for a suitable range
        // For simplicity, just find any range >= blocks_needed
        // A full implementation would use best-fit or first-fit strategies

        // Start from the first key (0) and scan
        let free_tree = &state.free_tree;
        let (result, _) = ops.search::<u64, u64>(free_tree, &0)?;

        match result.value() {
            Some(length) if length >= blocks_needed => {
                // Found a suitable range at block 0 (would need proper iteration)
                Ok(Some((0, length)))
            }
            _ => Ok(None),
        }
    }

    /// Allocate blocks from free space tree
    pub fn allocate<D: BlockDevice, A: BlockAllocator>(
        ops: &mut TreeOps<D, A>,
        state: &mut FilesystemState,
        blocks_needed: u64,
    ) -> Result<u64, TreeError> {
        // Find a free range
        let (start_block, range_length) = FreeSpaceOps::find_free_range(ops, state, blocks_needed)?
            .ok_or(TreeError::NodeFull)?; // No space

        // Remove the old range from tree
        ops.delete::<u64, u64>(&mut state.free_tree, &start_block)?;

        // If there's remaining space, add it back
        if range_length > blocks_needed {
            let new_start = start_block + blocks_needed;
            let new_length = range_length - blocks_needed;
            ops.insert(&mut state.free_tree, new_start, new_length)?;
        }

        // Update superblock free count
        state.superblock.free_blocks = state.superblock.free_blocks
            .saturating_sub(blocks_needed);

        // Record allocation in transaction
        if let Some(ref mut txn) = state.transaction {
            for i in 0..blocks_needed {
                txn.record_allocation(start_block + i);
            }
        }

        Ok(start_block)
    }

    /// Free blocks back to free space tree
    pub fn free<D: BlockDevice, A: BlockAllocator>(
        ops: &mut TreeOps<D, A>,
        state: &mut FilesystemState,
        start_block: u64,
        block_count: u64,
    ) -> Result<(), TreeError> {
        // Schedule for freeing after commit (CoW safety)
        if let Some(ref mut txn) = state.transaction {
            txn.schedule_free(start_block, block_count);
        } else {
            // Direct free (no transaction - e.g., during abort)
            ops.insert(&mut state.free_tree, start_block, block_count)?;
            state.superblock.free_blocks += block_count;
        }

        Ok(())
    }

    /// Process pending frees after commit
    ///
    /// This adds freed blocks back to the free space tree.
    /// Should only be called after successful commit.
    pub fn process_pending_frees<D: BlockDevice, A: BlockAllocator>(
        ops: &mut TreeOps<D, A>,
        state: &mut FilesystemState,
        pending_frees: &[(u64, u64)], // (start_block, count) pairs
    ) -> Result<(), TreeError> {
        for &(start, count) in pending_frees {
            // Insert free range (could merge with adjacent ranges)
            ops.insert(&mut state.free_tree, start, count)?;
            state.superblock.free_blocks += count;
        }
        Ok(())
    }
}

// ============================================================================
// PATH RESOLUTION
// ============================================================================

/// Resolve a path to an inode
pub fn resolve_path<D: BlockDevice, A: BlockAllocator>(
    ops: &mut TreeOps<D, A>,
    state: &FilesystemState,
    path: &str,
) -> Result<Option<Inode>, TreeError> {
    // Start at root inode
    let mut current_inode = match InodeOps::lookup(ops, state, ROOT_INODE)? {
        Some(inode) => inode,
        None => return Err(TreeError::NodeNotFound),
    };

    // Handle root path
    if path == "/" || path.is_empty() {
        return Ok(Some(current_inode));
    }

    // Split path and traverse
    let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    for component in components {
        // Current must be a directory
        if !current_inode.is_directory() {
            return Err(TreeError::InvalidOperation);
        }

        // Get directory tree for this inode
        let dir_tree = BPlusTree::new(
            current_inode.extent_root, // Directory uses extent_root for its tree
            NodeType::Directory,
            state.inode_tree.generation,
        );

        // Lookup component in directory
        match DirOps::lookup(ops, &dir_tree, component)? {
            Some(entry) => {
                // Follow to next inode
                current_inode = match InodeOps::lookup(ops, state, entry.inode_num)? {
                    Some(inode) => inode,
                    None => return Err(TreeError::NodeNotFound),
                };
            }
            None => return Ok(None), // Not found
        }
    }

    Ok(Some(current_inode))
}

// ============================================================================
// FILESYSTEM INITIALIZATION
// ============================================================================

/// Initialize a new filesystem
pub fn init_filesystem<D: BlockDevice + BlockAllocator>(
    device: &mut D,
    total_blocks: u64,
) -> Result<FilesystemState, TreeError> {
    // Create superblock
    let mut superblock = SuperblockV3::new(total_blocks);

    // Allocate blocks for initial structures
    // Block 0: Primary superblock
    // Block 1: Backup superblock
    // Block 2: Root inode tree node
    // Block 3: Free space tree node
    // Block 4: Root directory tree node

    let root_inode_tree_block = 2;
    let free_tree_block = 3;
    let root_dir_tree_block = 4;

    // Create root inode tree with root directory inode
    let mut root_tree_node = TreeNode::new_leaf(NodeType::Inode, 1);
    let mut root_inode = Inode::new_directory(ROOT_INODE);
    root_inode.extent_root = root_dir_tree_block; // Points to directory tree
    root_inode.update_crc();

    // Insert root inode into tree
    {
        use super::tree::LeafNode;
        let mut leaf = LeafNode::<u64, InodeValue>::new(&mut root_tree_node);
        leaf.insert(ROOT_INODE, InodeValue(root_inode)).map_err(|_| TreeError::NodeFull)?;
    }
    root_tree_node.update_crc();

    // Create empty free space tree
    let mut free_tree_node = TreeNode::new_leaf(NodeType::FreeSpace, 1);
    // Add all free blocks (starting after initial structures)
    let data_start = 5u64;
    let free_count = total_blocks.saturating_sub(data_start);
    if free_count > 0 {
        use super::tree::LeafNode;
        let mut leaf = LeafNode::<u64, u64>::new(&mut free_tree_node);
        leaf.insert(data_start, free_count).map_err(|_| TreeError::NodeFull)?;
    }
    free_tree_node.update_crc();

    // Create empty root directory tree (for . and ..)
    let mut root_dir_node = TreeNode::new_leaf(NodeType::Directory, 1);
    {
        use super::tree::LeafNode;
        let mut leaf = LeafNode::<u64, DirEntryValue>::new(&mut root_dir_node);

        // Add . entry
        let dot = super::dir::dot_entry(ROOT_INODE);
        leaf.insert(dot.name_hash, DirEntryValue(dot)).map_err(|_| TreeError::NodeFull)?;

        // Add .. entry (points to self for root)
        let dotdot = super::dir::dotdot_entry(ROOT_INODE);
        leaf.insert(dotdot.name_hash, DirEntryValue(dotdot)).map_err(|_| TreeError::NodeFull)?;
    }
    root_dir_node.update_crc();

    // Update superblock
    superblock.root_tree_block = root_inode_tree_block;
    superblock.free_tree_block = free_tree_block;
    superblock.root_generation = 1;
    superblock.inode_count = 1;
    superblock.next_inode = 2;
    superblock.free_blocks = free_count;
    superblock.data_start_block = data_start;
    superblock.update_crc();

    // Write all structures to disk
    device.write_node(root_inode_tree_block, &root_tree_node)?;
    device.write_node(free_tree_block, &free_tree_node)?;
    device.write_node(root_dir_tree_block, &root_dir_node)?;

    // Write superblocks (primary and backup)
    let mut sb_node = TreeNode::default();
    let sb_bytes = unsafe {
        core::slice::from_raw_parts(&superblock as *const _ as *const u8, core::mem::size_of::<SuperblockV3>())
    };
    sb_node.data[..sb_bytes.len()].copy_from_slice(sb_bytes);
    device.write_node(0, &sb_node)?;
    device.write_node(1, &sb_node)?;

    device.sync()?;

    Ok(FilesystemState::new(superblock))
}
