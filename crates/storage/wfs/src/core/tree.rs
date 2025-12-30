//! B+Tree Implementation
//!
//! Generic B+tree for WFS v3. Used for:
//! - Inode tree (inode_num -> Inode)
//! - Directory trees (name_hash -> DirEntry)
//! - Extent trees (file_offset -> Extent)
//! - Free space tree (start_block -> length)

#[allow(unused_imports)]
use crate::prelude::*;
use super::node::{TreeNode, NodeType, InternalEntry, NODE_DATA_SIZE, MAX_INTERNAL_ENTRIES};

// ============================================================================
// B+TREE CONFIGURATION
// ============================================================================

/// Minimum fill factor for nodes (except root)
pub const MIN_FILL_FACTOR: usize = 2; // At least 2 items per node

/// Maximum key size for variable keys
pub const MAX_KEY_SIZE: usize = 256;

// ============================================================================
// KEY-VALUE TRAITS
// ============================================================================

/// Trait for tree keys
pub trait TreeKey: Clone + Ord + Sized {
    /// Size of key when serialized
    fn serialized_size(&self) -> usize;

    /// Serialize key to bytes
    fn serialize(&self, buf: &mut [u8]) -> usize;

    /// Deserialize key from bytes
    fn deserialize(buf: &[u8]) -> Option<(Self, usize)>;
}

/// Trait for tree values
pub trait TreeValue: Clone + Sized {
    /// Size of value when serialized
    fn serialized_size(&self) -> usize;

    /// Serialize value to bytes
    fn serialize(&self, buf: &mut [u8]) -> usize;

    /// Deserialize value from bytes
    fn deserialize(buf: &[u8]) -> Option<(Self, usize)>;
}

// ============================================================================
// SIMPLE KEY IMPLEMENTATIONS
// ============================================================================

impl TreeKey for u64 {
    fn serialized_size(&self) -> usize {
        8
    }

    fn serialize(&self, buf: &mut [u8]) -> usize {
        if buf.len() >= 8 {
            buf[..8].copy_from_slice(&self.to_le_bytes());
            8
        } else {
            0
        }
    }

    fn deserialize(buf: &[u8]) -> Option<(Self, usize)> {
        if buf.len() >= 8 {
            let val = u64::from_le_bytes([
                buf[0], buf[1], buf[2], buf[3],
                buf[4], buf[5], buf[6], buf[7],
            ]);
            Some((val, 8))
        } else {
            None
        }
    }
}

impl TreeValue for u64 {
    fn serialized_size(&self) -> usize {
        8
    }

    fn serialize(&self, buf: &mut [u8]) -> usize {
        if buf.len() >= 8 {
            buf[..8].copy_from_slice(&self.to_le_bytes());
            8
        } else {
            0
        }
    }

    fn deserialize(buf: &[u8]) -> Option<(Self, usize)> {
        if buf.len() >= 8 {
            let val = u64::from_le_bytes([
                buf[0], buf[1], buf[2], buf[3],
                buf[4], buf[5], buf[6], buf[7],
            ]);
            Some((val, 8))
        } else {
            None
        }
    }
}

// ============================================================================
// SEARCH RESULT
// ============================================================================

/// Result of a tree search
#[derive(Clone, Debug)]
pub enum SearchResult<V> {
    /// Key found with value
    Found(V),
    /// Key not found
    NotFound,
    /// Error during search
    Error(TreeError),
}

impl<V> SearchResult<V> {
    pub fn is_found(&self) -> bool {
        matches!(self, SearchResult::Found(_))
    }

    pub fn value(self) -> Option<V> {
        match self {
            SearchResult::Found(v) => Some(v),
            _ => None,
        }
    }
}

// ============================================================================
// TREE ERROR
// ============================================================================

/// Tree operation errors
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeError {
    /// Node not found
    NodeNotFound,
    /// Invalid node format
    InvalidNode,
    /// CRC mismatch
    CrcError,
    /// Node is full
    NodeFull,
    /// Key not found
    KeyNotFound,
    /// Duplicate key
    DuplicateKey,
    /// I/O error
    IoError,
    /// Tree is empty
    EmptyTree,
    /// Invalid operation
    InvalidOperation,
}

// ============================================================================
// TREE PATH
// ============================================================================

/// A path from root to a leaf node
///
/// Used for CoW - when modifying, we need to copy all nodes in the path.
#[derive(Clone, Debug)]
pub struct TreePath {
    /// Nodes in path from root to leaf
    /// Each entry is (block_number, index_in_parent)
    pub nodes: Vec<(u64, usize)>,
}

impl TreePath {
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    pub fn push(&mut self, block: u64, index: usize) {
        self.nodes.push((block, index));
    }

    pub fn pop(&mut self) -> Option<(u64, usize)> {
        self.nodes.pop()
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Get the leaf block (last in path)
    pub fn leaf_block(&self) -> Option<u64> {
        self.nodes.last().map(|(b, _)| *b)
    }

    /// Get the root block (first in path)
    pub fn root_block(&self) -> Option<u64> {
        self.nodes.first().map(|(b, _)| *b)
    }
}

impl Default for TreePath {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// B+TREE LEAF NODE OPERATIONS
// ============================================================================

/// Leaf node operations for a specific key-value type
pub struct LeafNode<'a, K, V> {
    node: &'a mut TreeNode,
    _key: core::marker::PhantomData<K>,
    _value: core::marker::PhantomData<V>,
}

impl<'a, K: TreeKey, V: TreeValue> LeafNode<'a, K, V> {
    pub fn new(node: &'a mut TreeNode) -> Self {
        Self {
            node,
            _key: core::marker::PhantomData,
            _value: core::marker::PhantomData,
        }
    }

    /// Entry size (key + value)
    fn entry_size() -> usize {
        core::mem::size_of::<K>() + core::mem::size_of::<V>()
    }

    /// Maximum entries in this node
    pub fn max_entries() -> usize {
        NODE_DATA_SIZE / Self::entry_size()
    }

    /// Get entry at index
    pub fn get_entry(&self, index: usize) -> Option<(K, V)> {
        if index >= self.node.item_count as usize {
            return None;
        }

        let offset = index * Self::entry_size();
        let (key, _) = K::deserialize(&self.node.data[offset..])?;
        let (value, _) = V::deserialize(&self.node.data[offset + core::mem::size_of::<K>()..])?;
        Some((key, value))
    }

    /// Set entry at index
    pub fn set_entry(&mut self, index: usize, key: &K, value: &V) -> bool {
        if index > self.node.item_count as usize {
            return false;
        }

        let offset = index * Self::entry_size();
        key.serialize(&mut self.node.data[offset..]);
        value.serialize(&mut self.node.data[offset + core::mem::size_of::<K>()..]);
        true
    }

    /// Search for key, returns index if found
    pub fn search(&self, key: &K) -> Result<usize, usize> {
        // Binary search
        let mut left = 0;
        let mut right = self.node.item_count as usize;

        while left < right {
            let mid = left + (right - left) / 2;
            if let Some((mid_key, _)) = self.get_entry(mid) {
                match mid_key.cmp(key) {
                    core::cmp::Ordering::Less => left = mid + 1,
                    core::cmp::Ordering::Greater => right = mid,
                    core::cmp::Ordering::Equal => return Ok(mid),
                }
            } else {
                break;
            }
        }
        Err(left) // Insert position
    }

    /// Insert key-value pair
    pub fn insert(&mut self, key: K, value: V) -> Result<(), TreeError> {
        if self.node.item_count as usize >= Self::max_entries() {
            return Err(TreeError::NodeFull);
        }

        match self.search(&key) {
            Ok(_) => return Err(TreeError::DuplicateKey),
            Err(pos) => {
                // Shift entries right to make room
                let count = self.node.item_count as usize;
                for i in (pos..count).rev() {
                    if let Some((k, v)) = self.get_entry(i) {
                        self.set_entry(i + 1, &k, &v);
                    }
                }
                // Insert new entry
                self.set_entry(pos, &key, &value);
                self.node.item_count += 1;
                Ok(())
            }
        }
    }

    /// Delete key
    pub fn delete(&mut self, key: &K) -> Result<V, TreeError> {
        match self.search(key) {
            Ok(pos) => {
                let (_, value) = self.get_entry(pos).ok_or(TreeError::InvalidNode)?;
                // Shift entries left
                let count = self.node.item_count as usize;
                for i in pos..count - 1 {
                    if let Some((k, v)) = self.get_entry(i + 1) {
                        self.set_entry(i, &k, &v);
                    }
                }
                self.node.item_count -= 1;
                Ok(value)
            }
            Err(_) => Err(TreeError::KeyNotFound),
        }
    }

    /// Update value for existing key
    pub fn update(&mut self, key: &K, value: V) -> Result<V, TreeError> {
        match self.search(key) {
            Ok(pos) => {
                let (_, old_value) = self.get_entry(pos).ok_or(TreeError::InvalidNode)?;
                self.set_entry(pos, key, &value);
                Ok(old_value)
            }
            Err(_) => Err(TreeError::KeyNotFound),
        }
    }

    /// Check if node is full
    pub fn is_full(&self) -> bool {
        self.node.item_count as usize >= Self::max_entries()
    }

    /// Check if node needs split
    pub fn needs_split(&self) -> bool {
        self.node.item_count as usize > Self::max_entries() / 2
    }
}

// ============================================================================
// B+TREE INTERNAL NODE OPERATIONS
// ============================================================================

/// Internal node operations
pub struct InternalNode<'a> {
    node: &'a mut TreeNode,
}

impl<'a> InternalNode<'a> {
    pub fn new(node: &'a mut TreeNode) -> Self {
        Self { node }
    }

    /// Get child pointer at index
    pub fn get_child(&self, index: usize) -> Option<u64> {
        if index >= self.node.item_count as usize {
            return None;
        }

        let offset = index * InternalEntry::SIZE + 8; // Skip key, get child pointer
        if offset + 8 <= self.node.data.len() {
            let bytes = &self.node.data[offset..offset + 8];
            Some(u64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3],
                bytes[4], bytes[5], bytes[6], bytes[7],
            ]))
        } else {
            None
        }
    }

    /// Get key at index
    pub fn get_key(&self, index: usize) -> Option<u64> {
        if index >= self.node.item_count as usize {
            return None;
        }

        let offset = index * InternalEntry::SIZE;
        if offset + 8 <= self.node.data.len() {
            let bytes = &self.node.data[offset..offset + 8];
            Some(u64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3],
                bytes[4], bytes[5], bytes[6], bytes[7],
            ]))
        } else {
            None
        }
    }

    /// Set entry at index
    pub fn set_entry(&mut self, index: usize, key: u64, child: u64) {
        let offset = index * InternalEntry::SIZE;
        self.node.data[offset..offset + 8].copy_from_slice(&key.to_le_bytes());
        self.node.data[offset + 8..offset + 16].copy_from_slice(&child.to_le_bytes());
    }

    /// Find child for a key
    pub fn find_child(&self, key: u64) -> Option<(usize, u64)> {
        let count = self.node.item_count as usize;
        if count == 0 {
            return None;
        }

        // Binary search for the correct child
        let mut left = 0;
        let mut right = count;

        while left < right {
            let mid = left + (right - left) / 2;
            if let Some(mid_key) = self.get_key(mid) {
                if key < mid_key {
                    right = mid;
                } else {
                    left = mid + 1;
                }
            } else {
                break;
            }
        }

        // Child is at index 'left' (or last child if key is larger than all)
        let child_idx = if left > 0 { left - 1 } else { 0 };
        self.get_child(child_idx).map(|c| (child_idx, c))
    }

    /// Insert key-child pair
    pub fn insert(&mut self, key: u64, child: u64) -> Result<(), TreeError> {
        if self.node.item_count as usize >= MAX_INTERNAL_ENTRIES {
            return Err(TreeError::NodeFull);
        }

        // Find insert position
        let mut pos = 0;
        for i in 0..self.node.item_count as usize {
            if let Some(k) = self.get_key(i) {
                if key < k {
                    break;
                }
                pos = i + 1;
            }
        }

        // Shift entries right
        let count = self.node.item_count as usize;
        for i in (pos..count).rev() {
            if let (Some(k), Some(c)) = (self.get_key(i), self.get_child(i)) {
                self.set_entry(i + 1, k, c);
            }
        }

        // Insert new entry
        self.set_entry(pos, key, child);
        self.node.item_count += 1;
        Ok(())
    }
}

// ============================================================================
// B+TREE STRUCTURE
// ============================================================================

/// B+Tree structure
///
/// This is a logical representation. The actual tree lives on disk.
/// Operations require a BlockDevice to read/write nodes.
#[derive(Clone, Debug)]
pub struct BPlusTree {
    /// Root block number
    pub root_block: u64,
    /// Tree height (0 = just root leaf, 1 = root + leaves, etc.)
    pub height: u32,
    /// Node type for this tree
    pub node_type: NodeType,
    /// Current generation
    pub generation: u64,
}

impl BPlusTree {
    /// Create a new empty tree
    pub fn new(root_block: u64, node_type: NodeType, generation: u64) -> Self {
        Self {
            root_block,
            height: 0,
            node_type,
            generation,
        }
    }

    /// Create a tree from an existing root
    pub fn from_root(root_block: u64, height: u32, node_type: NodeType, generation: u64) -> Self {
        Self {
            root_block,
            height,
            node_type,
            generation,
        }
    }

    /// Check if tree is empty
    pub fn is_empty(&self) -> bool {
        self.root_block == 0
    }

    /// Increment generation (for CoW)
    pub fn next_generation(&mut self) -> u64 {
        self.generation += 1;
        self.generation
    }
}

impl Default for BPlusTree {
    fn default() -> Self {
        Self::new(0, NodeType::Inode, 0)
    }
}

// ============================================================================
// BLOCK DEVICE TRAIT
// ============================================================================

/// Trait for block device I/O operations
///
/// Implemented by kernel (using AhciController) and tools (using std::fs).
pub trait BlockDevice {
    /// Read a block from disk into a TreeNode
    fn read_node(&self, block: u64) -> Result<TreeNode, TreeError>;

    /// Write a TreeNode to disk
    fn write_node(&mut self, block: u64, node: &TreeNode) -> Result<(), TreeError>;

    /// Sync all writes to disk
    fn sync(&mut self) -> Result<(), TreeError>;
}

/// Trait for block allocation
///
/// Used during tree modifications to allocate new blocks for CoW.
pub trait BlockAllocator {
    /// Allocate a single block
    fn allocate_block(&mut self) -> Result<u64, TreeError>;

    /// Free a block (scheduled for later release after commit)
    fn free_block(&mut self, block: u64) -> Result<(), TreeError>;
}

// ============================================================================
// COW TREE OPERATIONS
// ============================================================================

/// Copy-on-Write tree operations
///
/// Provides high-level tree operations that handle CoW semantics:
/// - Never modify live blocks
/// - Allocate new blocks for modified nodes
/// - Return the new root block after modifications
pub struct TreeOps<'a, D: BlockDevice, A: BlockAllocator> {
    device: &'a mut D,
    allocator: &'a mut A,
}

impl<'a, D: BlockDevice, A: BlockAllocator> TreeOps<'a, D, A> {
    /// Create new tree operations context
    pub fn new(device: &'a mut D, allocator: &'a mut A) -> Self {
        Self { device, allocator }
    }

    /// Search for a key in the tree
    ///
    /// Returns the value if found, along with the path taken.
    pub fn search<K: TreeKey, V: TreeValue>(
        &self,
        tree: &BPlusTree,
        key: &K,
    ) -> Result<(SearchResult<V>, TreePath), TreeError> {
        if tree.is_empty() {
            return Ok((SearchResult::NotFound, TreePath::new()));
        }

        let mut path = TreePath::new();
        let mut current_block = tree.root_block;

        // Traverse from root to leaf
        for level in (0..=tree.height).rev() {
            let node = self.device.read_node(current_block)?;

            if !node.is_valid() {
                return Err(TreeError::CrcError);
            }

            if level == 0 {
                // Leaf node - search for key
                path.push(current_block, 0);

                // Create a temporary mutable node for the LeafNode wrapper
                let mut leaf_node = node;
                let leaf = LeafNode::<K, V>::new(&mut leaf_node);

                match leaf.search(key) {
                    Ok(idx) => {
                        if let Some((_, value)) = leaf.get_entry(idx) {
                            return Ok((SearchResult::Found(value), path));
                        }
                    }
                    Err(_) => {}
                }
                return Ok((SearchResult::NotFound, path));
            } else {
                // Internal node - find child
                let mut internal_node = node;
                let internal = InternalNode::new(&mut internal_node);

                // Convert key to u64 for internal node lookup
                // This works for u64 keys; other key types need different handling
                let key_bytes = {
                    let mut buf = [0u8; 8];
                    key.serialize(&mut buf);
                    u64::from_le_bytes(buf)
                };

                if let Some((idx, child)) = internal.find_child(key_bytes) {
                    path.push(current_block, idx);
                    current_block = child;
                } else {
                    return Err(TreeError::InvalidNode);
                }
            }
        }

        Err(TreeError::InvalidOperation)
    }

    /// Insert a key-value pair into the tree
    ///
    /// Uses CoW: allocates new blocks for all modified nodes.
    /// Returns the new root block.
    pub fn insert<K: TreeKey, V: TreeValue>(
        &mut self,
        tree: &mut BPlusTree,
        key: K,
        value: V,
    ) -> Result<u64, TreeError> {
        if tree.is_empty() {
            // Create a new root leaf node
            let new_block = self.allocator.allocate_block()?;
            let generation = tree.next_generation();
            let mut node = TreeNode::new_leaf(tree.node_type, generation);

            let mut leaf = LeafNode::<K, V>::new(&mut node);
            leaf.insert(key, value)?;

            node.update_crc();
            self.device.write_node(new_block, &node)?;

            tree.root_block = new_block;
            return Ok(new_block);
        }

        // Search for insert position
        let (result, path) = self.search::<K, V>(tree, &key)?;

        if result.is_found() {
            return Err(TreeError::DuplicateKey);
        }

        // Insert into leaf (may need split)
        let leaf_block = path.leaf_block().ok_or(TreeError::EmptyTree)?;
        let mut node = self.device.read_node(leaf_block)?;

        let generation = tree.next_generation();
        node.generation = generation;

        {
            let mut leaf = LeafNode::<K, V>::new(&mut node);

            if !leaf.is_full() {
                // Simple case: leaf has room
                leaf.insert(key, value)?;
                node.update_crc();

                // Allocate new block (CoW)
                let new_block = self.allocator.allocate_block()?;
                self.device.write_node(new_block, &node)?;

                // Propagate new block up the path
                return self.propagate_cow(tree, &path, new_block, None);
            }
        }

        // Node is full - need to split
        self.insert_with_split::<K, V>(tree, &path, key, value)
    }

    /// Insert with node splitting
    fn insert_with_split<K: TreeKey, V: TreeValue>(
        &mut self,
        tree: &mut BPlusTree,
        path: &TreePath,
        key: K,
        value: V,
    ) -> Result<u64, TreeError> {
        let leaf_block = path.leaf_block().ok_or(TreeError::EmptyTree)?;
        let mut node = self.device.read_node(leaf_block)?;
        let generation = tree.next_generation();

        // Split the leaf node
        let mut new_sibling = TreeNode::new_leaf(tree.node_type, generation);
        let (split_key, _) = self.split_leaf::<K, V>(&mut node, &mut new_sibling, key, value)?;

        node.generation = generation;
        node.update_crc();
        new_sibling.update_crc();

        // Allocate blocks for both nodes
        let left_block = self.allocator.allocate_block()?;
        let right_block = self.allocator.allocate_block()?;

        self.device.write_node(left_block, &node)?;
        self.device.write_node(right_block, &new_sibling)?;

        // Propagate split up the tree
        self.propagate_split(tree, path, left_block, split_key, right_block)
    }

    /// Split a leaf node
    ///
    /// Returns the split key (first key of right node).
    fn split_leaf<K: TreeKey, V: TreeValue>(
        &self,
        left: &mut TreeNode,
        right: &mut TreeNode,
        new_key: K,
        new_value: V,
    ) -> Result<(u64, ()), TreeError> {
        let mid = left.item_count as usize / 2;
        let item_count = left.item_count as usize;

        // Collect entries that will go to the right node
        let mut right_entries: Vec<(K, V)> = Vec::new();
        {
            let left_leaf = LeafNode::<K, V>::new(left);
            for i in mid..item_count {
                if let Some(entry) = left_leaf.get_entry(i) {
                    right_entries.push(entry);
                }
            }
        }

        // Determine where new key goes
        let new_key_bytes = {
            let mut buf = [0u8; 8];
            new_key.serialize(&mut buf);
            u64::from_le_bytes(buf)
        };

        // Truncate left node
        left.item_count = mid as u16;

        // Insert entries into right node
        let mut right_leaf = LeafNode::<K, V>::new(right);
        for (k, v) in right_entries {
            right_leaf.insert(k, v)?;
        }

        // Insert new key in appropriate node
        let first_right_key = if let Some((k, _)) = right_leaf.get_entry(0) {
            let mut buf = [0u8; 8];
            k.serialize(&mut buf);
            u64::from_le_bytes(buf)
        } else {
            0
        };

        if new_key_bytes < first_right_key {
            let mut left_leaf = LeafNode::<K, V>::new(left);
            left_leaf.insert(new_key, new_value)?;
        } else {
            right_leaf.insert(new_key, new_value)?;
        }

        // Return split key (first key of right node)
        let split_key = if let Some((k, _)) = LeafNode::<K, V>::new(right).get_entry(0) {
            let mut buf = [0u8; 8];
            k.serialize(&mut buf);
            u64::from_le_bytes(buf)
        } else {
            0
        };

        Ok((split_key, ()))
    }

    /// Propagate CoW changes up the tree
    fn propagate_cow(
        &mut self,
        tree: &mut BPlusTree,
        path: &TreePath,
        new_child_block: u64,
        _split_key: Option<u64>,
    ) -> Result<u64, TreeError> {
        if path.len() <= 1 {
            // Only leaf in path - it becomes the new root
            tree.root_block = new_child_block;
            return Ok(new_child_block);
        }

        let mut current_new_block = new_child_block;
        let generation = tree.generation;

        // Walk up the path, updating parent pointers
        for i in (0..path.len() - 1).rev() {
            let (parent_block, child_idx) = path.nodes[i];
            let mut parent = self.device.read_node(parent_block)?;

            // Update child pointer
            let mut internal = InternalNode::new(&mut parent);
            if let Some(key) = internal.get_key(child_idx) {
                internal.set_entry(child_idx, key, current_new_block);
            }

            parent.generation = generation;
            parent.update_crc();

            // Allocate new block for this parent
            current_new_block = self.allocator.allocate_block()?;
            self.device.write_node(current_new_block, &parent)?;
        }

        tree.root_block = current_new_block;
        Ok(current_new_block)
    }

    /// Propagate a split up the tree
    fn propagate_split(
        &mut self,
        tree: &mut BPlusTree,
        path: &TreePath,
        left_block: u64,
        split_key: u64,
        right_block: u64,
    ) -> Result<u64, TreeError> {
        if path.len() <= 1 {
            // Split the root - create new root
            let new_root_block = self.allocator.allocate_block()?;
            let generation = tree.generation;
            let mut new_root = TreeNode::new_internal(tree.node_type, tree.height as u16 + 1, generation);

            let mut internal = InternalNode::new(&mut new_root);
            internal.insert(0, left_block)?; // Leftmost child
            internal.insert(split_key, right_block)?;

            new_root.update_crc();
            self.device.write_node(new_root_block, &new_root)?;

            tree.root_block = new_root_block;
            tree.height += 1;
            return Ok(new_root_block);
        }

        // Insert split key into parent
        let parent_idx = path.len() - 2;
        let (parent_block, child_idx) = path.nodes[parent_idx];
        let mut parent = self.device.read_node(parent_block)?;

        let mut internal = InternalNode::new(&mut parent);

        // Update left child pointer
        if let Some(old_key) = internal.get_key(child_idx) {
            internal.set_entry(child_idx, old_key, left_block);
        }

        // Try to insert new key-child
        if (internal.node.item_count as usize) < MAX_INTERNAL_ENTRIES {
            internal.insert(split_key, right_block)?;
            parent.generation = tree.generation;
            parent.update_crc();

            let new_parent_block = self.allocator.allocate_block()?;
            self.device.write_node(new_parent_block, &parent)?;

            // Create new path without the leaf
            let mut parent_path = TreePath::new();
            for i in 0..parent_idx {
                parent_path.push(path.nodes[i].0, path.nodes[i].1);
            }
            parent_path.push(parent_block, child_idx);

            return self.propagate_cow(tree, &parent_path, new_parent_block, None);
        }

        // Parent is full - need to split it too (recursive)
        // This is a simplified version; full implementation would handle this
        Err(TreeError::NodeFull)
    }

    /// Delete a key from the tree
    ///
    /// Returns the old value if found.
    pub fn delete<K: TreeKey, V: TreeValue>(
        &mut self,
        tree: &mut BPlusTree,
        key: &K,
    ) -> Result<V, TreeError> {
        if tree.is_empty() {
            return Err(TreeError::EmptyTree);
        }

        let (result, path) = self.search::<K, V>(tree, key)?;

        match result {
            SearchResult::Found(value) => {
                // Delete from leaf
                let leaf_block = path.leaf_block().ok_or(TreeError::EmptyTree)?;
                let mut node = self.device.read_node(leaf_block)?;
                let generation = tree.next_generation();
                node.generation = generation;

                {
                    let mut leaf = LeafNode::<K, V>::new(&mut node);
                    leaf.delete(key)?;
                }

                node.update_crc();

                // Allocate new block (CoW)
                let new_block = self.allocator.allocate_block()?;
                self.device.write_node(new_block, &node)?;

                // Propagate changes up
                self.propagate_cow(tree, &path, new_block, None)?;

                Ok(value)
            }
            SearchResult::NotFound => Err(TreeError::KeyNotFound),
            SearchResult::Error(e) => Err(e),
        }
    }
}
