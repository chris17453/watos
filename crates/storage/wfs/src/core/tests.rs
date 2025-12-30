//! Unit tests for WFS v3 filesystem components
//!
//! Run with: cargo test --package wfs-common --features std

use super::*;
use std::collections::HashMap;

// ============================================================================
// MOCK BLOCK DEVICE FOR TESTING
// ============================================================================

/// In-memory block device for testing
struct MockBlockDevice {
    blocks: HashMap<u64, TreeNode>,
    next_block: u64,
}

impl MockBlockDevice {
    fn new() -> Self {
        Self {
            blocks: HashMap::new(),
            next_block: 10, // Start after reserved blocks
        }
    }

    fn block_count(&self) -> usize {
        self.blocks.len()
    }
}

impl BlockDevice for MockBlockDevice {
    fn read_node(&self, block: u64) -> Result<TreeNode, TreeError> {
        self.blocks.get(&block).cloned().ok_or(TreeError::NodeNotFound)
    }

    fn write_node(&mut self, block: u64, node: &TreeNode) -> Result<(), TreeError> {
        self.blocks.insert(block, *node);
        Ok(())
    }

    fn sync(&mut self) -> Result<(), TreeError> {
        Ok(())
    }
}

impl BlockAllocator for MockBlockDevice {
    fn allocate_block(&mut self) -> Result<u64, TreeError> {
        let block = self.next_block;
        self.next_block += 1;
        Ok(block)
    }

    fn free_block(&mut self, _block: u64) -> Result<(), TreeError> {
        Ok(())
    }
}

// ============================================================================
// SUPERBLOCK TESTS
// ============================================================================

#[test]
fn test_superblock_new() {
    let sb = Superblock::new(1000);
    assert_eq!(sb.magic, WFS_MAGIC);
    assert_eq!(sb.version, WFS_VERSION);
    assert_eq!(sb.total_blocks, 1000);
    assert_eq!(sb.block_size, BLOCK_SIZE);
}

#[test]
fn test_superblock_crc() {
    let mut sb = Superblock::new(1000);
    sb.update_crc();
    assert!(sb.verify_crc());

    // Modify and verify CRC fails
    sb.total_blocks = 2000;
    assert!(!sb.verify_crc());

    // Update CRC and verify passes
    sb.update_crc();
    assert!(sb.verify_crc());
}

#[test]
fn test_superblock_is_valid() {
    let mut sb = Superblock::new(1000);
    sb.update_crc();
    assert!(sb.is_valid());

    // Invalid magic
    sb.magic = 0;
    sb.update_crc();
    assert!(!sb.is_valid());
}

#[test]
fn test_superblock_size() {
    assert_eq!(std::mem::size_of::<Superblock>(), SUPERBLOCK_SIZE);
}

// ============================================================================
// TREE NODE TESTS
// ============================================================================

#[test]
fn test_tree_node_new_leaf() {
    let node = TreeNode::new_leaf(NodeType::Inode, 1);
    assert!(node.is_leaf());
    assert!(!node.is_internal());
    assert_eq!(node.level, 0);
    assert_eq!(node.generation, 1);
    assert_eq!(node.item_count, 0);
}

#[test]
fn test_tree_node_new_internal() {
    let node = TreeNode::new_internal(NodeType::Directory, 1, 5);
    assert!(!node.is_leaf());
    assert!(node.is_internal());
    assert_eq!(node.level, 1);
    assert_eq!(node.generation, 5);
}

#[test]
fn test_tree_node_crc() {
    let mut node = TreeNode::new_leaf(NodeType::Inode, 1);
    node.item_count = 5;
    node.data[0] = 42;
    node.update_crc();
    assert!(node.verify_crc());

    // Modify and verify fails
    node.data[0] = 43;
    assert!(!node.verify_crc());
}

#[test]
fn test_tree_node_type() {
    let node = TreeNode::new_leaf(NodeType::Directory, 1);
    assert_eq!(node.node_type(), Some(NodeType::Directory));

    let node = TreeNode::new_leaf(NodeType::Extent, 1);
    assert_eq!(node.node_type(), Some(NodeType::Extent));

    let node = TreeNode::new_leaf(NodeType::FreeSpace, 1);
    assert_eq!(node.node_type(), Some(NodeType::FreeSpace));
}

#[test]
fn test_tree_node_size() {
    assert_eq!(std::mem::size_of::<TreeNode>(), BLOCK_SIZE as usize);
}

// ============================================================================
// INODE TESTS
// ============================================================================

#[test]
fn test_inode_new_file() {
    let inode = Inode::new_file(42);
    assert_eq!(inode.inode_num, 42);
    assert!(inode.is_file());
    assert!(!inode.is_directory());
    assert_eq!(inode.nlink, 1);
    assert_eq!(inode.size, 0);
}

#[test]
fn test_inode_new_directory() {
    let inode = Inode::new_directory(100);
    assert_eq!(inode.inode_num, 100);
    assert!(inode.is_directory());
    assert!(!inode.is_file());
    assert_eq!(inode.nlink, 2); // . and parent reference
}

#[test]
fn test_inode_new_symlink() {
    let inode = Inode::new_symlink(200);
    assert_eq!(inode.inode_num, 200);
    assert!(inode.is_symlink());
}

#[test]
fn test_inode_inline_data() {
    let mut inode = Inode::new_file(1);
    let data = b"Hello, World!";

    assert!(inode.set_inline_data(data));
    assert!(inode.is_inline());
    assert_eq!(inode.size, data.len() as u64);
    assert_eq!(inode.get_inline_data(), data);
}

#[test]
fn test_inode_inline_data_too_large() {
    let mut inode = Inode::new_file(1);
    let data = vec![0u8; inode::INODE_INLINE_SIZE + 1];

    assert!(!inode.set_inline_data(&data));
}

#[test]
fn test_inode_refcount() {
    let mut inode = Inode::new_file(1);
    assert_eq!(inode.refcount, 1);
    assert!(!inode.is_shared());

    inode.inc_refcount();
    assert_eq!(inode.refcount, 2);
    assert!(inode.is_shared());

    inode.dec_refcount();
    assert_eq!(inode.refcount, 1);
    assert!(!inode.is_shared());
}

#[test]
fn test_inode_crc() {
    let mut inode = Inode::new_file(1);
    inode.size = 1000;
    inode.update_crc();
    assert!(inode.verify_crc());
}

#[test]
fn test_inode_size() {
    assert_eq!(std::mem::size_of::<Inode>(), inode::INODE_SIZE);
}

// ============================================================================
// DIRECTORY ENTRY TESTS
// ============================================================================

#[test]
fn test_direntry_new() {
    let entry = dir::DirEntry::new("test.txt", 42, dir::EntryType::File).unwrap();
    assert_eq!(entry.inode_num, 42);
    assert_eq!(entry.name_str(), "test.txt");
    assert!(entry.is_file());
    assert!(!entry.is_directory());
}

#[test]
fn test_direntry_directory() {
    let entry = dir::DirEntry::new("subdir", 100, dir::EntryType::Directory).unwrap();
    assert!(entry.is_directory());
    assert!(!entry.is_file());
}

#[test]
fn test_direntry_hash() {
    let entry1 = dir::DirEntry::new("file1.txt", 1, dir::EntryType::File).unwrap();
    let entry2 = dir::DirEntry::new("file2.txt", 2, dir::EntryType::File).unwrap();

    // Different names should have different hashes
    assert_ne!(entry1.name_hash, entry2.name_hash);

    // Same name should have same hash
    let entry3 = dir::DirEntry::new("file1.txt", 3, dir::EntryType::File).unwrap();
    assert_eq!(entry1.name_hash, entry3.name_hash);
}

#[test]
fn test_direntry_matches() {
    let entry = dir::DirEntry::new("myfile.txt", 42, dir::EntryType::File).unwrap();
    assert!(entry.matches("myfile.txt"));
    assert!(!entry.matches("other.txt"));
    assert!(!entry.matches("MYFILE.TXT")); // Case-sensitive
}

#[test]
fn test_direntry_special() {
    let dot = dir::dot_entry(1);
    assert_eq!(dot.name_str(), ".");
    assert_eq!(dot.inode_num, 1);

    let dotdot = dir::dotdot_entry(0);
    assert_eq!(dotdot.name_str(), "..");
    assert_eq!(dotdot.inode_num, 0);
}

#[test]
fn test_direntry_invalid_name() {
    // Empty name
    assert!(dir::DirEntry::new("", 1, dir::EntryType::File).is_none());

    // Too long name
    let long_name = "a".repeat(256);
    assert!(dir::DirEntry::new(&long_name, 1, dir::EntryType::File).is_none());
}

#[test]
fn test_direntry_size() {
    assert_eq!(std::mem::size_of::<dir::DirEntry>(), dir::DIRENTRY_SIZE);
}

// ============================================================================
// EXTENT TESTS
// ============================================================================

#[test]
fn test_extent_new() {
    let ext = Extent::new(0, 100, 4096);
    assert_eq!(ext.file_offset, 0);
    assert_eq!(ext.disk_block, 100);
    assert_eq!(ext.length, 4096);
    assert_eq!(ext.refcount, 1);
    assert!(!ext.is_hole());
    assert!(!ext.is_shared());
}

#[test]
fn test_extent_hole() {
    let ext = Extent::hole(0, 8192);
    assert!(ext.is_hole());
    assert_eq!(ext.disk_block, 0);
}

#[test]
fn test_extent_contains_offset() {
    let ext = Extent::new(1000, 50, 2000);
    assert!(!ext.contains_offset(999));
    assert!(ext.contains_offset(1000));
    assert!(ext.contains_offset(2999));
    assert!(!ext.contains_offset(3000));
}

#[test]
fn test_extent_offset_to_block() {
    let ext = Extent::new(0, 100, 8192); // 2 blocks

    assert_eq!(ext.offset_to_block(0), Some(100));
    assert_eq!(ext.offset_to_block(4095), Some(100));
    assert_eq!(ext.offset_to_block(4096), Some(101));
    assert_eq!(ext.offset_to_block(8191), Some(101));
    assert_eq!(ext.offset_to_block(8192), None); // Out of range
}

#[test]
fn test_extent_split() {
    let ext = Extent::new(0, 100, 8192);
    let (left, right) = ext.split_at(4096).unwrap();

    assert_eq!(left.file_offset, 0);
    assert_eq!(left.length, 4096);
    assert_eq!(left.disk_block, 100);

    assert_eq!(right.file_offset, 4096);
    assert_eq!(right.length, 4096);
    assert_eq!(right.disk_block, 101);
}

#[test]
fn test_extent_refcount() {
    let mut ext = Extent::new(0, 100, 4096);
    assert!(!ext.is_shared());

    ext.inc_refcount();
    assert!(ext.is_shared());
    assert_eq!(ext.refcount, 2);

    ext.dec_refcount();
    assert!(!ext.is_shared());
    assert_eq!(ext.refcount, 1);
}

#[test]
fn test_extent_merge() {
    let ext1 = Extent::new(0, 100, 4096);
    let ext2 = Extent::new(4096, 101, 4096);

    let merged = ext1.merge_with(&ext2).unwrap();
    assert_eq!(merged.file_offset, 0);
    assert_eq!(merged.length, 8192);
    assert_eq!(merged.disk_block, 100);
}

#[test]
fn test_extent_cannot_merge_non_adjacent() {
    let ext1 = Extent::new(0, 100, 4096);
    let ext2 = Extent::new(8192, 102, 4096); // Gap in file

    assert!(ext1.merge_with(&ext2).is_none());
}

#[test]
fn test_extent_size() {
    assert_eq!(std::mem::size_of::<Extent>(), extent::EXTENT_SIZE);
}

// ============================================================================
// FREE RANGE TESTS
// ============================================================================

#[test]
fn test_free_range_new() {
    let range = FreeRange::new(100, 50);
    assert_eq!(range.start_block, 100);
    assert_eq!(range.block_count, 50);
    assert_eq!(range.end_block(), 150);
}

#[test]
fn test_free_range_allocate() {
    let range = FreeRange::new(100, 50);

    // Allocate from start
    let (start, remaining) = range.allocate(10).unwrap();
    assert_eq!(start, 100);
    let rem = remaining.unwrap();
    assert_eq!(rem.start_block, 110);
    assert_eq!(rem.block_count, 40);
}

#[test]
fn test_free_range_allocate_exact() {
    let range = FreeRange::new(100, 10);
    let (start, remaining) = range.allocate(10).unwrap();
    assert_eq!(start, 100);
    assert!(remaining.is_none());
}

#[test]
fn test_free_range_merge() {
    let range1 = FreeRange::new(100, 10);
    let range2 = FreeRange::new(110, 20);

    let merged = range1.merge(&range2).unwrap();
    assert_eq!(merged.start_block, 100);
    assert_eq!(merged.block_count, 30);
}

#[test]
fn test_free_range_size() {
    assert_eq!(std::mem::size_of::<FreeRange>(), freespace::FREE_RANGE_SIZE);
}

// ============================================================================
// B+TREE LEAF NODE TESTS
// ============================================================================

#[test]
fn test_leaf_node_insert_search() {
    let mut node = TreeNode::new_leaf(NodeType::Inode, 1);

    {
        let mut leaf = tree::LeafNode::<u64, u64>::new(&mut node);

        // Insert some values
        leaf.insert(10, 100).unwrap();
        leaf.insert(5, 50).unwrap();
        leaf.insert(15, 150).unwrap();
        leaf.insert(8, 80).unwrap();

        // Search for values
        assert_eq!(leaf.search(&5), Ok(0));
        assert_eq!(leaf.search(&8), Ok(1));
        assert_eq!(leaf.search(&10), Ok(2));
        assert_eq!(leaf.search(&15), Ok(3));

        // Search for non-existent (returns insert position)
        assert_eq!(leaf.search(&3), Err(0));
        assert_eq!(leaf.search(&7), Err(1));
        assert_eq!(leaf.search(&20), Err(4));
    }

    // Check item count after borrow is released
    assert_eq!(node.item_count, 4);
}

#[test]
fn test_leaf_node_delete() {
    let mut node = TreeNode::new_leaf(NodeType::Inode, 1);

    {
        let mut leaf = tree::LeafNode::<u64, u64>::new(&mut node);
        leaf.insert(10, 100).unwrap();
        leaf.insert(20, 200).unwrap();
        leaf.insert(30, 300).unwrap();
    }

    {
        let mut leaf = tree::LeafNode::<u64, u64>::new(&mut node);
        let deleted = leaf.delete(&20).unwrap();
        assert_eq!(deleted, 200);

        // Verify remaining entries
        assert_eq!(leaf.search(&10), Ok(0));
        assert_eq!(leaf.search(&30), Ok(1));
        assert_eq!(leaf.search(&20), Err(1));
    }

    // Check item count after borrow is released
    assert_eq!(node.item_count, 2);
}

#[test]
fn test_leaf_node_update() {
    let mut node = TreeNode::new_leaf(NodeType::Inode, 1);

    {
        let mut leaf = tree::LeafNode::<u64, u64>::new(&mut node);
        leaf.insert(10, 100).unwrap();
    }

    {
        let mut leaf = tree::LeafNode::<u64, u64>::new(&mut node);
        let old = leaf.update(&10, 999).unwrap();
        assert_eq!(old, 100);

        let (_, value) = leaf.get_entry(0).unwrap();
        assert_eq!(value, 999);
    }
}

#[test]
fn test_leaf_node_duplicate_key() {
    let mut node = TreeNode::new_leaf(NodeType::Inode, 1);

    {
        let mut leaf = tree::LeafNode::<u64, u64>::new(&mut node);
        leaf.insert(10, 100).unwrap();

        let result = leaf.insert(10, 200);
        assert_eq!(result, Err(TreeError::DuplicateKey));
    }
}

// ============================================================================
// B+TREE OPERATIONS TESTS
// ============================================================================

#[test]
fn test_tree_ops_insert_into_empty() {
    let mut device = MockBlockDevice::new();
    let mut allocator = MockBlockDevice::new();

    let mut tree = BPlusTree::new(0, NodeType::Inode, 0);

    {
        let mut ops = TreeOps::new(&mut device, &mut allocator);
        let new_root = ops.insert::<u64, u64>(&mut tree, 42, 420).unwrap();
        assert!(new_root > 0);
    }

    assert!(!tree.is_empty());
    assert_eq!(tree.height, 0);
}

#[test]
fn test_tree_ops_insert_and_search() {
    let mut device = MockBlockDevice::new();
    let mut allocator = MockBlockDevice::new();

    let mut tree = BPlusTree::new(0, NodeType::Inode, 0);

    // Insert values
    {
        let mut ops = TreeOps::new(&mut device, &mut allocator);
        ops.insert::<u64, u64>(&mut tree, 10, 100).unwrap();
        ops.insert::<u64, u64>(&mut tree, 20, 200).unwrap();
        ops.insert::<u64, u64>(&mut tree, 30, 300).unwrap();
    }

    // Search for values
    {
        let ops = TreeOps::new(&mut device, &mut allocator);

        let (result, _) = ops.search::<u64, u64>(&tree, &10).unwrap();
        assert_eq!(result.value(), Some(100));

        let (result, _) = ops.search::<u64, u64>(&tree, &20).unwrap();
        assert_eq!(result.value(), Some(200));

        let (result, _) = ops.search::<u64, u64>(&tree, &30).unwrap();
        assert_eq!(result.value(), Some(300));

        let (result, _) = ops.search::<u64, u64>(&tree, &15).unwrap();
        assert!(!result.is_found());
    }
}

#[test]
fn test_tree_ops_delete() {
    let mut device = MockBlockDevice::new();
    let mut allocator = MockBlockDevice::new();

    let mut tree = BPlusTree::new(0, NodeType::Inode, 0);

    // Insert values
    {
        let mut ops = TreeOps::new(&mut device, &mut allocator);
        ops.insert::<u64, u64>(&mut tree, 10, 100).unwrap();
        ops.insert::<u64, u64>(&mut tree, 20, 200).unwrap();
        ops.insert::<u64, u64>(&mut tree, 30, 300).unwrap();
    }

    // Delete a value
    {
        let mut ops = TreeOps::new(&mut device, &mut allocator);
        let deleted = ops.delete::<u64, u64>(&mut tree, &20).unwrap();
        assert_eq!(deleted, 200);
    }

    // Verify deletion
    {
        let ops = TreeOps::new(&mut device, &mut allocator);

        let (result, _) = ops.search::<u64, u64>(&tree, &20).unwrap();
        assert!(!result.is_found());

        // Other values still exist
        let (result, _) = ops.search::<u64, u64>(&tree, &10).unwrap();
        assert!(result.is_found());
    }
}

#[test]
fn test_tree_ops_cow_creates_new_blocks() {
    let mut device = MockBlockDevice::new();
    let mut allocator = MockBlockDevice::new();

    let mut tree = BPlusTree::new(0, NodeType::Inode, 0);

    {
        let mut ops = TreeOps::new(&mut device, &mut allocator);
        ops.insert::<u64, u64>(&mut tree, 10, 100).unwrap();
    }

    let initial_blocks = device.block_count();
    let initial_root = tree.root_block;

    // Another insert should create new blocks (CoW)
    {
        let mut ops = TreeOps::new(&mut device, &mut allocator);
        ops.insert::<u64, u64>(&mut tree, 20, 200).unwrap();
    }

    // Root should have changed (CoW)
    assert_ne!(tree.root_block, initial_root);
    // More blocks allocated
    assert!(device.block_count() > initial_blocks);
}

// ============================================================================
// TRANSACTION TESTS
// ============================================================================

#[test]
fn test_transaction_new() {
    let txn = Transaction::new(1, 100, 50);
    assert_eq!(txn.id, 1);
    assert_eq!(txn.generation, 100);
    assert_eq!(txn.working_root, 50);
    assert!(txn.is_active());
    assert!(!txn.is_dirty());
}

#[test]
fn test_transaction_record_allocation() {
    let mut txn = Transaction::new(1, 100, 50);

    txn.record_allocation(60);
    txn.record_allocation(61);

    assert!(txn.is_dirty());
    assert_eq!(txn.allocated_count(), 2);
    assert_eq!(txn.allocated_blocks, vec![60, 61]);
}

#[test]
fn test_transaction_cow_copy() {
    let mut txn = Transaction::new(1, 100, 50);

    txn.record_cow_copy(100, 200);

    assert!(txn.is_dirty());
    assert_eq!(txn.modified_count(), 1);
    assert_eq!(txn.modified_blocks[0].original_block, 100);
    assert_eq!(txn.modified_blocks[0].new_block, 200);
}

#[test]
fn test_transaction_schedule_free() {
    let mut txn = Transaction::new(1, 100, 50);

    txn.schedule_free(80, 5);

    assert!(txn.is_dirty());
    assert_eq!(txn.pending_frees.len(), 1);
    assert_eq!(txn.pending_frees[0].block, 80);
    assert_eq!(txn.pending_frees[0].count, 5);
}

#[test]
fn test_transaction_commit_lifecycle() {
    let mut txn = Transaction::new(1, 100, 50);

    assert!(txn.is_active());
    assert!(txn.begin_commit());
    assert_eq!(txn.state, transaction::TransactionState::Committing);

    txn.complete_commit();
    assert_eq!(txn.state, transaction::TransactionState::Committed);
    assert!(!txn.is_active());
}

#[test]
fn test_transaction_abort() {
    let mut txn = Transaction::new(1, 100, 50);
    txn.record_allocation(60);

    txn.abort();

    assert_eq!(txn.state, transaction::TransactionState::Aborted);
    assert!(!txn.is_active());
    assert_eq!(txn.blocks_to_free_on_abort(), &[60]);
}

// ============================================================================
// INTEGRATION TESTS
// ============================================================================

#[test]
fn test_filesystem_state_new() {
    let sb = Superblock::new(1000);
    let state = FilesystemState::new(sb);

    assert!(!state.has_active_transaction());
    assert_eq!(state.root_block(), 0); // No root yet
}

#[test]
fn test_tree_key_u64() {
    let key: u64 = 12345;
    let mut buf = [0u8; 16];

    let written = TreeKey::serialize(&key, &mut buf);
    assert_eq!(written, 8);

    let (deserialized, read) = <u64 as TreeKey>::deserialize(&buf).unwrap();
    assert_eq!(deserialized, 12345);
    assert_eq!(read, 8);
}

#[test]
fn test_search_result() {
    let found: SearchResult<u64> = SearchResult::Found(42);
    assert!(found.is_found());
    assert_eq!(found.value(), Some(42));

    let not_found: SearchResult<u64> = SearchResult::NotFound;
    assert!(!not_found.is_found());
    assert_eq!(not_found.value(), None);
}

#[test]
fn test_tree_path() {
    let mut path = TreePath::new();
    assert!(path.is_empty());

    path.push(100, 0);
    path.push(200, 1);
    path.push(300, 2);

    assert_eq!(path.len(), 3);
    assert_eq!(path.root_block(), Some(100));
    assert_eq!(path.leaf_block(), Some(300));

    let (block, idx) = path.pop().unwrap();
    assert_eq!(block, 300);
    assert_eq!(idx, 2);
    assert_eq!(path.len(), 2);
}

// ============================================================================
// STRESS TESTS
// ============================================================================

#[test]
fn test_many_insertions() {
    let mut device = MockBlockDevice::new();
    let mut allocator = MockBlockDevice::new();

    let mut tree = BPlusTree::new(0, NodeType::Inode, 0);

    // Insert 100 values
    {
        let mut ops = TreeOps::new(&mut device, &mut allocator);
        for i in 0..100u64 {
            ops.insert::<u64, u64>(&mut tree, i, i * 10).unwrap();
        }
    }

    // Verify all values
    {
        let ops = TreeOps::new(&mut device, &mut allocator);
        for i in 0..100u64 {
            let (result, _) = ops.search::<u64, u64>(&tree, &i).unwrap();
            assert_eq!(result.value(), Some(i * 10));
        }
    }
}

#[test]
fn test_random_order_insertions() {
    let mut device = MockBlockDevice::new();
    let mut allocator = MockBlockDevice::new();

    let mut tree = BPlusTree::new(0, NodeType::Inode, 0);

    // Insert in "random" order
    let values = [50, 25, 75, 10, 30, 60, 90, 5, 15, 28, 35, 55, 65, 85, 95];

    {
        let mut ops = TreeOps::new(&mut device, &mut allocator);
        for &v in &values {
            ops.insert::<u64, u64>(&mut tree, v, v * 100).unwrap();
        }
    }

    // Verify all values
    {
        let ops = TreeOps::new(&mut device, &mut allocator);
        for &v in &values {
            let (result, _) = ops.search::<u64, u64>(&tree, &v).unwrap();
            assert_eq!(result.value(), Some(v * 100));
        }
    }
}
