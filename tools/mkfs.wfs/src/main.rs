//! mkfs.wfs - Create WFS (WATOS File System) disk images
//!
//! WFS v1 Features (CoW):
//! - Copy-on-Write with atomic transactions
//! - B+tree indexed directories (O(log n) lookup)
//! - Extent-based file storage
//! - Crash-safe without journal
//! - Checksums on all metadata
//!
//! Usage:
//!   mkfs.wfs -o disk.img -s 64M          # Create 64MB v1 disk image
//!   mkfs.wfs -o disk.img -s 1G           # Create 1GB v1 disk image

use clap::Parser;
use std::cell::UnsafeCell;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

// Import all shared WFS definitions
use wfs_common::*;
use wfs_common::core::{
    Superblock, TreeNode, NodeType, Inode, BPlusTree, BlockDevice, BlockAllocator, TreeError,
    TreeOps, BLOCK_SIZE, ROOT_INODE, InodeOps, DirOps, FilesystemState,
};
use wfs_common::core::inode::INODE_INLINE_SIZE;
use wfs_common::core::dir::{DirEntry, EntryType};

#[derive(Parser)]
#[command(name = "mkfs.wfs")]
#[command(about = "Create WFS (WATOS File System) v1 disk images")]
struct Args {
    /// Output disk image file
    #[arg(short, long)]
    output: PathBuf,

    /// Disk size (e.g., 64M, 1G)
    #[arg(short, long)]
    size: String,

    /// Directory to copy files from
    #[arg(short, long)]
    dir: Option<PathBuf>,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
}

fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim().to_uppercase();
    let (num_str, mult) = if s.ends_with("G") || s.ends_with("GB") {
        (s.trim_end_matches("GB").trim_end_matches("G"), 1024 * 1024 * 1024)
    } else if s.ends_with("M") || s.ends_with("MB") {
        (s.trim_end_matches("MB").trim_end_matches("M"), 1024 * 1024)
    } else if s.ends_with("K") || s.ends_with("KB") {
        (s.trim_end_matches("KB").trim_end_matches("K"), 1024)
    } else {
        (s.as_str(), 1)
    };

    num_str.parse::<u64>().ok().map(|n| n * mult)
}

struct FileBlockDevice {
    file: UnsafeCell<File>,
    next_block: UnsafeCell<u64>,
}

impl FileBlockDevice {
    fn new(path: &PathBuf, size: u64) -> std::io::Result<Self> {
        // Open file for read+write
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        file.set_len(size)?;
        // First 5 blocks are reserved (superblocks, trees)
        Ok(Self {
            file: UnsafeCell::new(file),
            next_block: UnsafeCell::new(5),
        })
    }

    fn write_block_raw(&mut self, block: u64, data: &[u8]) -> std::io::Result<()> {
        let file = unsafe { &mut *self.file.get() };
        file.seek(SeekFrom::Start(block * BLOCK_SIZE as u64))?;
        file.write_all(data)
    }
}

impl BlockDevice for FileBlockDevice {
    fn read_node(&self, block: u64) -> Result<TreeNode, TreeError> {
        let file = unsafe { &mut *self.file.get() };
        let mut node = TreeNode::default();
        file.seek(SeekFrom::Start(block * BLOCK_SIZE as u64))
            .map_err(|_| TreeError::IoError)?;

        let node_bytes = unsafe {
            std::slice::from_raw_parts_mut(
                &mut node as *mut TreeNode as *mut u8,
                BLOCK_SIZE as usize,
            )
        };
        file.read_exact(node_bytes).map_err(|_| TreeError::IoError)?;
        Ok(node)
    }

    fn write_node(&mut self, block: u64, node: &TreeNode) -> Result<(), TreeError> {
        let file = unsafe { &mut *self.file.get() };
        file.seek(SeekFrom::Start(block * BLOCK_SIZE as u64))
            .map_err(|_| TreeError::IoError)?;

        let node_bytes = unsafe {
            std::slice::from_raw_parts(
                node as *const TreeNode as *const u8,
                BLOCK_SIZE as usize,
            )
        };
        file.write_all(node_bytes).map_err(|_| TreeError::IoError)?;
        Ok(())
    }

    fn sync(&mut self) -> Result<(), TreeError> {
        let file = unsafe { &mut *self.file.get() };
        file.sync_all().map_err(|_| TreeError::IoError)
    }
}

impl BlockAllocator for FileBlockDevice {
    fn allocate_block(&mut self) -> Result<u64, TreeError> {
        let next_block = unsafe { &mut *self.next_block.get() };
        let block = *next_block;
        *next_block += 1;

        // Zero out the new block
        let zero_block = [0u8; BLOCK_SIZE as usize];
        self.write_block_raw(block, &zero_block)
            .map_err(|_| TreeError::IoError)?;

        Ok(block)
    }

    fn free_block(&mut self, _block: u64) -> Result<(), TreeError> {
        // For mkfs.wfs, we don't need to track freed blocks since
        // we're building the filesystem from scratch
        Ok(())
    }
}

/// Populate WFS filesystem from a source directory
fn populate_filesystem(
    device: &mut FileBlockDevice,
    state: &mut FilesystemState,
    source_dir: &Path,
    verbose: bool,
) -> std::io::Result<()> {
    let mut file_count = 0;
    let mut dir_count = 0;
    let mut skipped_count = 0;

    // Helper function to recursively populate
    fn populate_recursive(
        device: &mut FileBlockDevice,
        state: &mut FilesystemState,
        source_path: &Path,
        wfs_parent_inode_num: u64,
        file_count: &mut usize,
        dir_count: &mut usize,
        skipped_count: &mut usize,
        verbose: bool,
    ) -> std::io::Result<()> {
        for entry in fs::read_dir(source_path)? {
            let entry = entry?;
            let path = entry.path();
            let file_name = entry.file_name();
            let name_str = file_name.to_string_lossy();

            // Skip special names
            if name_str == "." || name_str == ".." {
                continue;
            }

            let metadata = entry.metadata()?;

            if metadata.is_dir() {
                // Create directory in WFS
                if let Err(e) = create_directory(device, state, wfs_parent_inode_num, &name_str, verbose) {
                    eprintln!("Failed to create directory {}: {:?}", name_str, e);
                    *skipped_count += 1;
                    continue;
                }

                *dir_count += 1;
                if verbose {
                    println!("  DIR:  {}", name_str);
                }

                // Get the newly created directory's inode number
                let dir_inode_num = find_entry_inode(device, state, wfs_parent_inode_num, &name_str)?;

                // Recursively populate subdirectory
                populate_recursive(device, state, &path, dir_inode_num,
                                 file_count, dir_count, skipped_count, verbose)?;

            } else if metadata.is_file() {
                let file_size = metadata.len();

                // Only support small files that fit in inline data
                if file_size <= INODE_INLINE_SIZE as u64 {
                    // Read file contents
                    let mut file_data = Vec::new();
                    let mut file = File::open(&path)?;
                    file.read_to_end(&mut file_data)?;

                    // Create file in WFS
                    if let Err(e) = create_file(device, state, wfs_parent_inode_num,
                                                &name_str, &file_data, verbose) {
                        eprintln!("Failed to create file {}: {:?}", name_str, e);
                        *skipped_count += 1;
                        continue;
                    }

                    *file_count += 1;
                    if verbose {
                        println!("  FILE: {} ({} bytes)", name_str, file_size);
                    }
                } else {
                    if verbose {
                        println!("  SKIP: {} ({} bytes, too large)", name_str, file_size);
                    }
                    *skipped_count += 1;
                }
            }
        }

        Ok(())
    }

    populate_recursive(device, state, source_dir, ROOT_INODE,
                      &mut file_count, &mut dir_count, &mut skipped_count, verbose)?;

    println!("\nPopulation complete:");
    println!("  Files:   {}", file_count);
    println!("  Dirs:    {}", dir_count);
    println!("  Skipped: {}", skipped_count);

    Ok(())
}

/// Find inode number for an entry in a directory
fn find_entry_inode(
    device: &mut FileBlockDevice,
    state: &FilesystemState,
    parent_inode_num: u64,
    name: &str,
) -> std::io::Result<u64> {
    let dev_ptr = device as *mut FileBlockDevice;
    let (dev_ref, alloc_ref) = unsafe { (&mut *dev_ptr, &mut *dev_ptr) };
    let mut ops = TreeOps::new(dev_ref, alloc_ref);

    // Get parent inode
    let parent_inode = InodeOps::lookup(&mut ops, state, parent_inode_num)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e)))?
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Parent not found"))?;

    // Search directory tree
    let parent_dir_tree = BPlusTree::new(
        parent_inode.extent_root,
        NodeType::Directory,
        state.superblock.root_generation,
    );

    let entry = DirOps::lookup(&mut ops, &parent_dir_tree, name)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e)))?
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Entry not found"))?;

    Ok(entry.inode_num)
}

/// Create a directory in WFS
fn create_directory(
    device: &mut FileBlockDevice,
    state: &mut FilesystemState,
    parent_inode_num: u64,
    name: &str,
    verbose: bool,
) -> std::io::Result<()> {
    let dev_ptr = device as *mut FileBlockDevice;
    let (dev_ref, alloc_ref) = unsafe { (&mut *dev_ptr, &mut *dev_ptr) };
    let mut ops = TreeOps::new(dev_ref, alloc_ref);

    // Get parent inode
    let mut parent_inode = InodeOps::lookup(&mut ops, state, parent_inode_num)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e)))?
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Parent not found"))?;

    // Allocate new inode and directory tree block
    let inode_num = InodeOps::allocate_inode_num(state);
    let dir_tree_block = device.allocate_block()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e)))?;

    // Create directory inode
    let mut dir_inode = Inode::new(inode_num, S_IFDIR | 0o755);
    dir_inode.extent_root = dir_tree_block;
    dir_inode.nlink = 2;

    // Create empty directory tree
    let mut dir_tree_node = TreeNode::new(NodeType::Directory, 0, state.superblock.root_generation);
    dir_tree_node.update_crc();
    device.write_node(dir_tree_block, &dir_tree_node)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e)))?;

    // Insert directory inode
    InodeOps::insert(&mut ops, state, dir_inode)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e)))?;

    // Add entry to parent directory
    let entry = DirEntry::new(name, inode_num, EntryType::Directory)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "Name too long"))?;

    let mut parent_dir_tree = BPlusTree::new(
        parent_inode.extent_root,
        NodeType::Directory,
        state.superblock.root_generation,
    );

    DirOps::insert(&mut ops, &mut parent_dir_tree, entry)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e)))?;

    // Update parent inode (CoW may change root)
    parent_inode.extent_root = parent_dir_tree.root_block;
    InodeOps::insert(&mut ops, state, parent_inode)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e)))?;

    Ok(())
}

/// Create a file in WFS with inline data
fn create_file(
    device: &mut FileBlockDevice,
    state: &mut FilesystemState,
    parent_inode_num: u64,
    name: &str,
    data: &[u8],
    verbose: bool,
) -> std::io::Result<()> {
    if data.len() > INODE_INLINE_SIZE {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "File too large"));
    }

    let dev_ptr = device as *mut FileBlockDevice;
    let (dev_ref, alloc_ref) = unsafe { (&mut *dev_ptr, &mut *dev_ptr) };
    let mut ops = TreeOps::new(dev_ref, alloc_ref);

    // Get parent inode
    let mut parent_inode = InodeOps::lookup(&mut ops, state, parent_inode_num)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e)))?
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Parent not found"))?;

    // Allocate new inode
    let inode_num = InodeOps::allocate_inode_num(state);

    // Create file inode with inline data
    let mut file_inode = Inode::new(inode_num, S_IFREG | 0o644);
    file_inode.size = data.len() as u64;
    file_inode.inline_size = data.len() as u16;
    file_inode.inline_data[..data.len()].copy_from_slice(data);
    file_inode.nlink = 1;

    // Insert file inode
    InodeOps::insert(&mut ops, state, file_inode)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e)))?;

    // Add entry to parent directory
    let entry = DirEntry::new(name, inode_num, EntryType::File)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "Name too long"))?;

    let mut parent_dir_tree = BPlusTree::new(
        parent_inode.extent_root,
        NodeType::Directory,
        state.superblock.root_generation,
    );

    DirOps::insert(&mut ops, &mut parent_dir_tree, entry)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e)))?;

    // Update parent inode (CoW may change root)
    parent_inode.extent_root = parent_dir_tree.root_block;
    InodeOps::insert(&mut ops, state, parent_inode)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e)))?;

    Ok(())
}

fn create_wfs_v1(path: &PathBuf, size: u64, verbose: bool) -> std::io::Result<(FileBlockDevice, FilesystemState)> {
    let mut device = FileBlockDevice::new(path, size)?;
    let total_blocks = size / BLOCK_SIZE as u64;

    if verbose {
        println!("Creating WFS v1 filesystem:");
        println!("  Total blocks: {}", total_blocks);
        println!("  Block size: {} bytes", BLOCK_SIZE);
    }

    // Block allocation:
    // 0-1: Superblock (primary + backup)
    // 2: Root inode tree
    // 3: Free space tree
    // 4: Root directory tree
    // 5+: Data blocks

    let root_inode_tree_block = 2;
    let free_tree_block = 3;
    let root_dir_tree_block = 4;
    let data_start = 5;
    let free_count = total_blocks - data_start;

    // Create empty trees with proper CRCs
    let mut inode_tree_node = TreeNode::new(NodeType::Inode, 0, 1);
    inode_tree_node.update_crc();

    let mut free_tree_node = TreeNode::new(NodeType::FreeSpace, 0, 1);
    free_tree_node.update_crc();

    let mut dir_tree_node = TreeNode::new(NodeType::Directory, 0, 1);
    dir_tree_node.update_crc();

    // Create root directory inode
    let mut root_inode = Inode::new(ROOT_INODE, S_IFDIR | 0o755);
    root_inode.extent_root = root_dir_tree_block;
    root_inode.nlink = 2;

    // Create superblock
    let mut superblock = Superblock::new(total_blocks);
    superblock.root_tree_block = root_inode_tree_block;
    superblock.free_tree_block = free_tree_block;
    superblock.root_generation = 1;
    superblock.inode_count = 1;
    superblock.next_inode = 2;
    superblock.free_blocks = free_count;
    superblock.data_start_block = data_start;
    superblock.update_crc();

    if verbose {
        println!("  Root inode tree: block {}", root_inode_tree_block);
        println!("  Free space tree: block {}", free_tree_block);
        println!("  Root dir tree:   block {}", root_dir_tree_block);
        println!("  Data starts:     block {}", data_start);
        println!("  Free blocks:     {}", free_count);
    }

    // Write all structures
    device.write_node(root_inode_tree_block, &inode_tree_node)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "Failed to write inode tree"))?;

    device.write_node(free_tree_block, &free_tree_node)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "Failed to write free tree"))?;

    device.write_node(root_dir_tree_block, &dir_tree_node)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "Failed to write dir tree"))?;

    // Write superblocks (primary and backup)
    let mut sb_block = [0u8; BLOCK_SIZE as usize];
    let sb_bytes = unsafe {
        std::slice::from_raw_parts(&superblock as *const _ as *const u8,
            std::mem::size_of::<Superblock>())
    };
    sb_block[..sb_bytes.len()].copy_from_slice(sb_bytes);

    device.write_block_raw(0, &sb_block)?;
    device.write_block_raw(1, &sb_block)?;

    // Sync to ensure all writes are flushed before we start using TreeOps
    device.sync().map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "Initial sync failed"))?;

    // Create filesystem state
    let mut state = FilesystemState::new(superblock);

    // Insert root inode into inode tree
    let dev_ptr = &mut device as *mut FileBlockDevice;
    let (dev_ref, alloc_ref) = unsafe { (&mut *dev_ptr, &mut *dev_ptr) };
    let mut ops = TreeOps::new(dev_ref, alloc_ref);
    InodeOps::insert(&mut ops, &mut state, root_inode)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to insert root inode: {:?}", e)))?;

    device.sync().map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "Sync failed"))?;

    if verbose {
        println!("\nWFS v1 filesystem created successfully!");
    }

    Ok((device, state))
}

fn main() -> std::io::Result<()> {
    let args = Args::parse();

    let size = parse_size(&args.size)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput,
                                           "Invalid size format"))?;

    if size < 1024 * 1024 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput,
                                       "Disk size must be at least 1MB"));
    }

    // Create WFS v1 CoW filesystem
    println!("Creating WFS v1 (CoW) disk image: {}", args.output.display());
    println!("  Size: {} bytes ({} blocks)", size, size / BLOCK_SIZE as u64);

    let (mut device, mut state) = create_wfs_v1(&args.output, size, args.verbose)?;

    // Add files from directory if specified
    if let Some(ref dir) = args.dir {
        if !dir.exists() {
            return Err(std::io::Error::new(std::io::ErrorKind::NotFound,
                format!("Directory not found: {}", dir.display())));
        }

        println!("\nPopulating filesystem from: {}", dir.display());
        println!("Note: Only small files (<160 bytes) will be copied.");
        println!("      Larger files will be skipped.\n");

        populate_filesystem(&mut device, &mut state, dir, args.verbose)?;

        // Write updated superblock
        let mut sb_block = [0u8; BLOCK_SIZE as usize];
        let sb_bytes = unsafe {
            std::slice::from_raw_parts(&state.superblock as *const _ as *const u8,
                std::mem::size_of::<Superblock>())
        };
        sb_block[..sb_bytes.len()].copy_from_slice(sb_bytes);
        device.write_block_raw(0, &sb_block)?;
        device.write_block_raw(1, &sb_block)?;
    }

    // Final sync
    device.sync().map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "Final sync failed"))?;

    println!("\nDone! WFS v1 filesystem created.");
    Ok(())
}
