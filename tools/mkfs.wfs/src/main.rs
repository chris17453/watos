//! mkfs.wfs - Create WFS (WATOS File System) disk images
//!
//! WFS v2 Features:
//! - Up to 65536 files per volume
//! - Case-sensitive filenames with whitespace support
//! - Boundary markers for visual debugging
//! - CRC32 on all structures
//!
//! WFS v3 Features (CoW):
//! - Copy-on-Write with atomic transactions
//! - B+tree indexed directories (O(log n) lookup)
//! - Extent-based file storage
//! - Crash-safe without journal
//! - Checksums on all metadata
//!
//! Usage:
//!   mkfs.wfs -o disk.img -s 64M           # Create 64MB v2 disk image
//!   mkfs.wfs -o disk.img -s 64M -d files/ # Create and populate from directory
//!   mkfs.wfs -o disk.img -s 1G -m 8192    # 1GB with 8192 max files
//!   mkfs.wfs -o disk.img -s 64M --v3      # Create v3 CoW filesystem

use clap::Parser;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

// Import all shared WFS definitions
use wfs_common::*;
use wfs_common::v3::{
    SuperblockV3, TreeNode, NodeType, Inode, BPlusTree, BlockDevice, TreeError,
    TreeOps, TreeKey, TreeValue, WFS3_MAGIC, BLOCK_SIZE as V3_BLOCK_SIZE,
    ROOT_INODE, WFS3_SIGNATURE, InodeOps, DirOps, FilesystemState,
};

#[derive(Parser)]
#[command(name = "mkfs.wfs")]
#[command(about = "Create WFS (WATOS File System) disk images")]
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

    /// Maximum files (default: 4096) - v2 only
    #[arg(short, long, default_value = "4096")]
    max_files: u32,

    /// Create v3 CoW filesystem instead of v2
    #[arg(long)]
    v3: bool,

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

struct WfsImage {
    file: File,
    layout: DiskLayout,
    next_data_block: u64,
    file_count: u32,
    bitmap: Vec<u8>,
    file_table: Vec<u8>,
}

impl WfsImage {
    fn create(path: &PathBuf, size: u64, max_files: u32) -> std::io::Result<Self> {
        let file = File::create(path)?;
        file.set_len(size)?;

        let total_blocks = size / BLOCK_SIZE as u64;
        let layout = DiskLayout::calculate(total_blocks, max_files);

        // Allocate bitmap
        let bitmap_bytes = ((total_blocks + 7) / 8) as usize;
        let mut bitmap = vec![0u8; bitmap_bytes];

        // Mark all metadata blocks as used
        Self::mark_block(&mut bitmap, 0);  // Primary superblock
        Self::mark_block(&mut bitmap, 1);  // Boundary after superblock

        // Bitmap blocks
        for i in 0..layout.bitmap_blocks {
            Self::mark_block(&mut bitmap, layout.bitmap_block + i as u64);
        }

        // Boundary after bitmap
        Self::mark_block(&mut bitmap, layout.bitmap_block + layout.bitmap_blocks as u64);

        // File table blocks
        for i in 0..layout.filetable_blocks {
            Self::mark_block(&mut bitmap, layout.filetable_block + i as u64);
        }

        // Boundary after file table
        Self::mark_block(&mut bitmap, layout.filetable_block + layout.filetable_blocks as u64);

        // Backup superblocks and their boundaries
        Self::mark_block(&mut bitmap, layout.mid_superblock - 1);  // Boundary before mid
        Self::mark_block(&mut bitmap, layout.mid_superblock);       // Mid superblock
        Self::mark_block(&mut bitmap, layout.mid_superblock + 1);   // Boundary after mid
        Self::mark_block(&mut bitmap, layout.last_superblock - 1);  // Boundary before last
        Self::mark_block(&mut bitmap, layout.last_superblock);      // Last superblock

        // Allocate file table in memory
        let filetable_size = layout.filetable_blocks as usize * BLOCK_SIZE as usize;
        let file_table = vec![0u8; filetable_size];

        // Extract data_start_block before moving layout
        let next_data_block = layout.data_start_block;

        Ok(Self {
            file,
            layout,
            next_data_block,
            file_count: 0,
            bitmap,
            file_table,
        })
    }

    fn mark_block(bitmap: &mut [u8], block: u64) {
        let byte_idx = (block / 8) as usize;
        let bit_idx = (block % 8) as u8;
        if byte_idx < bitmap.len() {
            bitmap[byte_idx] |= 1 << bit_idx;
        }
    }

    fn write_block(&mut self, block: u64, data: &[u8]) -> std::io::Result<()> {
        self.file.seek(SeekFrom::Start(block * BLOCK_SIZE as u64))?;
        self.file.write_all(data)?;
        Ok(())
    }

    fn write_boundary(&mut self, block: u64, btype: BoundaryType, seq: u32, prev_end: u64, next_start: u64) -> std::io::Result<()> {
        let boundary = BoundaryBlock::new(btype, seq, prev_end, next_start);
        let bytes = unsafe {
            std::slice::from_raw_parts(&boundary as *const _ as *const u8, BLOCK_SIZE as usize)
        };
        self.write_block(block, bytes)
    }

    fn add_file(&mut self, name: &str, data: &[u8], flags: u16, verbose: bool) -> std::io::Result<()> {
        if self.file_count >= self.layout.max_files {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "Too many files"));
        }

        let num_blocks = blocks_needed(data.len());
        let start_block = self.next_data_block;

        // Skip over backup superblock boundaries if we hit them
        let mut actual_start = start_block;
        if actual_start >= self.layout.mid_superblock - 1 && actual_start <= self.layout.mid_superblock + 1 {
            actual_start = self.layout.mid_superblock + 2;
        }
        if actual_start >= self.layout.last_superblock - 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "Disk full"));
        }

        // Check if we have space
        if actual_start + num_blocks as u64 >= self.layout.last_superblock - 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "Disk full"));
        }

        // Write data blocks with CRC
        let mut offset = 0;
        for i in 0..num_blocks {
            let mut block = vec![0u8; BLOCK_SIZE as usize];
            let remaining = data.len().saturating_sub(offset);
            let copy_len = remaining.min(DATA_PER_BLOCK);

            if copy_len > 0 {
                block[..copy_len].copy_from_slice(&data[offset..offset + copy_len]);
            }
            offset += copy_len;

            // Calculate and append CRC at end of block
            let block_crc = crc32(&block[..DATA_PER_BLOCK]);
            block[DATA_PER_BLOCK..DATA_PER_BLOCK + 4].copy_from_slice(&block_crc.to_le_bytes());

            let block_num = actual_start + i as u64;
            self.write_block(block_num, &block)?;
            Self::mark_block(&mut self.bitmap, block_num);
        }

        self.next_data_block = actual_start + num_blocks as u64;

        // Create file entry
        let mut entry = FileEntry::default();
        entry.set_name(name);  // Case-sensitive, whitespace preserved
        entry.size = data.len() as u64;
        entry.start_block = actual_start;
        entry.blocks = num_blocks;
        entry.flags = flags;
        entry.data_crc32 = crc32(data);
        entry.crc32 = file_entry_crc(&entry);

        // Calculate offset into in-memory file table
        let (table_block, byte_offset) = self.layout.file_entry_offset(self.file_count as usize);
        let block_in_table = (table_block - self.layout.filetable_block) as usize;
        let absolute_offset = block_in_table * BLOCK_SIZE as usize + byte_offset;

        // Insert entry into in-memory file table
        unsafe {
            std::ptr::copy_nonoverlapping(
                &entry as *const _ as *const u8,
                self.file_table.as_mut_ptr().add(absolute_offset),
                FILEENTRY_SIZE
            );
        }

        if verbose {
            println!("  {:20} {:>8} bytes  {:>4} blocks  @block {}",
                     name, data.len(), num_blocks, actual_start);
        }

        self.file_count += 1;
        Ok(())
    }

    fn finalize(&mut self, verbose: bool) -> std::io::Result<()> {
        // Calculate free blocks
        let used_blocks = self.bitmap.iter()
            .map(|&b| b.count_ones() as u64)
            .sum::<u64>();
        let free_blocks = self.layout.total_blocks - used_blocks;

        // Create superblock
        let mut sb = Superblock {
            magic: WFS_MAGIC,
            version: WFS_VERSION,
            flags: 0,
            signature: *WFS_SIGNATURE,
            block_size: BLOCK_SIZE,
            _pad0: 0,
            total_blocks: self.layout.total_blocks,
            free_blocks,
            data_start_block: self.layout.data_start_block,
            bitmap_block: self.layout.bitmap_block,
            filetable_block: self.layout.filetable_block,
            bitmap_blocks: self.layout.bitmap_blocks,
            filetable_blocks: self.layout.filetable_blocks,
            max_files: self.layout.max_files,
            root_files: self.file_count,
            reserved: [0; 112],
            crc32: 0,
            _pad1: 0,
        };
        sb.crc32 = superblock_crc(&sb);

        // Write superblock to buffer
        let mut sb_block = vec![0u8; BLOCK_SIZE as usize];
        unsafe {
            std::ptr::copy_nonoverlapping(&sb as *const _ as *const u8,
                                          sb_block.as_mut_ptr(),
                                          SUPERBLOCK_SIZE);
        }

        if verbose {
            println!("\nWriting filesystem structures:");
        }

        // Write primary superblock
        self.write_block(0, &sb_block)?;
        if verbose { println!("  Block 0:      Primary superblock"); }

        // Boundary: Superblock -> Bitmap
        self.write_boundary(1, BoundaryType::SuperblockToBitmap, 1, 0, self.layout.bitmap_block)?;
        if verbose { println!("  Block 1:      Boundary (SB->BITMAP)"); }

        // Write bitmap
        let bitmap_blocks = self.layout.bitmap_blocks as usize;
        for i in 0..bitmap_blocks {
            let start = i * BLOCK_SIZE as usize;
            let end = (start + BLOCK_SIZE as usize).min(self.bitmap.len());
            let mut block = vec![0u8; BLOCK_SIZE as usize];
            if end > start {
                block[..end - start].copy_from_slice(&self.bitmap[start..end]);
            }
            self.write_block(self.layout.bitmap_block + i as u64, &block)?;
        }
        if verbose {
            println!("  Block {}-{}:    Bitmap ({} blocks)",
                     self.layout.bitmap_block,
                     self.layout.bitmap_block + bitmap_blocks as u64 - 1,
                     bitmap_blocks);
        }

        // Boundary: Bitmap -> File table
        let bitmap_end = self.layout.bitmap_block + self.layout.bitmap_blocks as u64 - 1;
        let boundary_block = bitmap_end + 1;
        self.write_boundary(boundary_block, BoundaryType::BitmapToFiletable, 2, bitmap_end, self.layout.filetable_block)?;
        if verbose { println!("  Block {}:     Boundary (BITMAP->FT)", boundary_block); }

        // Write file table
        let ft_blocks = self.layout.filetable_blocks as usize;
        for i in 0..ft_blocks {
            let start = i * BLOCK_SIZE as usize;
            let end = start + BLOCK_SIZE as usize;
            let block_data = self.file_table[start..end].to_vec();
            self.write_block(self.layout.filetable_block + i as u64, &block_data)?;
        }
        if verbose {
            println!("  Block {}-{}:   File table ({} blocks, {} max files)",
                     self.layout.filetable_block,
                     self.layout.filetable_block + ft_blocks as u64 - 1,
                     ft_blocks,
                     self.layout.max_files);
        }

        // Boundary: File table -> Data
        let ft_end = self.layout.filetable_block + self.layout.filetable_blocks as u64 - 1;
        let data_boundary = ft_end + 1;
        self.write_boundary(data_boundary, BoundaryType::FiletableToData, 3, ft_end, self.layout.data_start_block)?;
        if verbose { println!("  Block {}:    Boundary (FT->DATA)", data_boundary); }

        if verbose {
            println!("  Block {}+:   Data blocks", self.layout.data_start_block);
        }

        // Boundary before mid superblock
        self.write_boundary(self.layout.mid_superblock - 1, BoundaryType::DataToSuperblock, 4,
                           self.layout.mid_superblock - 2, self.layout.mid_superblock)?;
        if verbose { println!("  Block {}:   Boundary (DATA->SB)", self.layout.mid_superblock - 1); }

        // Mid superblock
        self.write_block(self.layout.mid_superblock, &sb_block)?;
        if verbose { println!("  Block {}:   Backup superblock 1", self.layout.mid_superblock); }

        // Boundary after mid superblock
        self.write_boundary(self.layout.mid_superblock + 1, BoundaryType::SuperblockToData, 5,
                           self.layout.mid_superblock, self.layout.mid_superblock + 2)?;
        if verbose { println!("  Block {}:   Boundary (SB->DATA)", self.layout.mid_superblock + 1); }

        // Boundary before last superblock
        self.write_boundary(self.layout.last_superblock - 1, BoundaryType::DataToSuperblock, 6,
                           self.layout.last_superblock - 2, self.layout.last_superblock)?;
        if verbose { println!("  Block {}:  Boundary (DATA->SB)", self.layout.last_superblock - 1); }

        // Last superblock
        self.write_block(self.layout.last_superblock, &sb_block)?;
        if verbose { println!("  Block {}:  Backup superblock 2", self.layout.last_superblock); }

        self.file.sync_all()?;
        Ok(())
    }
}

fn is_executable(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".exe") ||
    lower.ends_with(".com") ||
    lower.ends_with(".bin") ||
    lower.ends_with(".sys")
}

/// Recursively collect all files from a directory
/// Returns (relative_path, full_path) pairs where relative_path uses / as separator
fn collect_files_recursive(
    base_dir: &PathBuf,
    current_dir: &PathBuf,
    files: &mut Vec<(String, PathBuf)>,
) -> std::io::Result<()> {
    for entry in fs::read_dir(current_dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;

        if file_type.is_file() {
            // Calculate relative path from base_dir
            let relative = path.strip_prefix(base_dir)
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "Path error"))?;

            // Convert to string with forward slashes
            let relative_str = relative.to_string_lossy().replace('\\', "/");
            files.push((relative_str, path));
        } else if file_type.is_dir() {
            // Recurse into subdirectory
            collect_files_recursive(base_dir, &path, files)?;
        }
    }
    Ok(())
}

// ============================================================================
// WFS V3 SUPPORT
// ============================================================================

/// File-backed block device for v3
struct FileBlockDevice {
    file: File,
    next_block: u64,  // For simple allocation
    total_blocks: u64,
}

impl FileBlockDevice {
    fn new(file: File, total_blocks: u64) -> Self {
        Self {
            file,
            next_block: 5, // Start after superblocks and initial trees
            total_blocks,
        }
    }

    fn write_block_raw(&mut self, block: u64, data: &[u8]) -> std::io::Result<()> {
        self.file.seek(SeekFrom::Start(block * V3_BLOCK_SIZE as u64))?;
        self.file.write_all(data)?;
        Ok(())
    }

    fn read_block_raw(&mut self, block: u64, buf: &mut [u8]) -> std::io::Result<()> {
        self.file.seek(SeekFrom::Start(block * V3_BLOCK_SIZE as u64))?;
        self.file.read_exact(buf)?;
        Ok(())
    }
}

impl BlockDevice for FileBlockDevice {
    fn read_node(&self, block: u64) -> Result<TreeNode, TreeError> {
        let mut file = &self.file;
        file.seek(SeekFrom::Start(block * V3_BLOCK_SIZE as u64))
            .map_err(|_| TreeError::IoError)?;

        let mut buf = [0u8; V3_BLOCK_SIZE as usize];
        file.read_exact(&mut buf).map_err(|_| TreeError::IoError)?;

        // Convert bytes to TreeNode
        let node = unsafe {
            std::ptr::read(buf.as_ptr() as *const TreeNode)
        };
        Ok(node)
    }

    fn write_node(&mut self, block: u64, node: &TreeNode) -> Result<(), TreeError> {
        self.file.seek(SeekFrom::Start(block * V3_BLOCK_SIZE as u64))
            .map_err(|_| TreeError::IoError)?;

        let bytes = unsafe {
            std::slice::from_raw_parts(node as *const _ as *const u8, V3_BLOCK_SIZE as usize)
        };
        self.file.write_all(bytes).map_err(|_| TreeError::IoError)?;
        Ok(())
    }

    fn sync(&mut self) -> Result<(), TreeError> {
        self.file.sync_all().map_err(|_| TreeError::IoError)
    }
}

impl wfs_common::v3::BlockAllocator for FileBlockDevice {
    fn allocate_block(&mut self) -> Result<u64, TreeError> {
        if self.next_block >= self.total_blocks {
            return Err(TreeError::NodeFull);
        }
        let block = self.next_block;
        self.next_block += 1;
        Ok(block)
    }

    fn free_block(&mut self, _block: u64) -> Result<(), TreeError> {
        // For mkfs, we don't need to track frees
        Ok(())
    }
}

/// Create a v3 filesystem
fn create_wfs_v3(path: &PathBuf, size: u64, verbose: bool) -> std::io::Result<()> {
    let file = File::create(path)?;
    file.set_len(size)?;

    let total_blocks = size / V3_BLOCK_SIZE as u64;

    if verbose {
        println!("Creating WFS v3 (CoW) filesystem:");
        println!("  Total blocks: {}", total_blocks);
    }

    let mut device = FileBlockDevice::new(file, total_blocks);

    // Block layout:
    // 0: Primary superblock
    // 1: Backup superblock
    // 2: Root inode tree node
    // 3: Free space tree node
    // 4: Root directory tree node
    // 5+: Data/tree nodes

    let root_inode_tree_block = 2u64;
    let free_tree_block = 3u64;
    let root_dir_tree_block = 4u64;
    let data_start = 5u64;

    // Create root inode (directory)
    let mut root_inode = Inode::new_directory(ROOT_INODE);
    root_inode.extent_root = root_dir_tree_block;
    root_inode.update_crc();

    // Create root inode tree node
    let mut inode_tree_node = TreeNode::new_leaf(NodeType::Inode, 1);

    // Insert root inode into tree
    // Serialize inode into the tree node data
    let inode_key_bytes = ROOT_INODE.to_le_bytes();
    let inode_bytes = unsafe {
        std::slice::from_raw_parts(&root_inode as *const _ as *const u8,
            std::mem::size_of::<Inode>())
    };

    // Format: [key (8 bytes)][inode (256 bytes)]
    inode_tree_node.data[0..8].copy_from_slice(&inode_key_bytes);
    inode_tree_node.data[8..8 + inode_bytes.len()].copy_from_slice(inode_bytes);
    inode_tree_node.item_count = 1;
    inode_tree_node.update_crc();

    // Create free space tree node
    let mut free_tree_node = TreeNode::new_leaf(NodeType::FreeSpace, 1);
    let free_count = total_blocks.saturating_sub(data_start);

    // Insert free range: key=data_start, value=free_count
    free_tree_node.data[0..8].copy_from_slice(&data_start.to_le_bytes());
    free_tree_node.data[8..16].copy_from_slice(&free_count.to_le_bytes());
    free_tree_node.item_count = 1;
    free_tree_node.update_crc();

    // Create empty root directory tree node
    let mut dir_tree_node = TreeNode::new_leaf(NodeType::Directory, 1);
    // Add . and .. entries
    use wfs_common::v3::dir::{DirEntry, EntryType, dot_entry, dotdot_entry, DIRENTRY_SIZE};

    let dot = dot_entry(ROOT_INODE);
    let dotdot = dotdot_entry(ROOT_INODE);

    // Insert . entry
    dir_tree_node.data[0..8].copy_from_slice(&dot.name_hash.to_le_bytes());
    let dot_bytes = unsafe {
        std::slice::from_raw_parts(&dot as *const _ as *const u8, DIRENTRY_SIZE)
    };
    dir_tree_node.data[8..8 + DIRENTRY_SIZE].copy_from_slice(dot_bytes);

    // Insert .. entry
    let offset2 = 8 + DIRENTRY_SIZE;
    dir_tree_node.data[offset2..offset2 + 8].copy_from_slice(&dotdot.name_hash.to_le_bytes());
    let dotdot_bytes = unsafe {
        std::slice::from_raw_parts(&dotdot as *const _ as *const u8, DIRENTRY_SIZE)
    };
    dir_tree_node.data[offset2 + 8..offset2 + 8 + DIRENTRY_SIZE].copy_from_slice(dotdot_bytes);

    dir_tree_node.item_count = 2;
    dir_tree_node.update_crc();

    // Create superblock
    let mut superblock = SuperblockV3::new(total_blocks);
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
    let mut sb_block = [0u8; V3_BLOCK_SIZE as usize];
    let sb_bytes = unsafe {
        std::slice::from_raw_parts(&superblock as *const _ as *const u8,
            std::mem::size_of::<SuperblockV3>())
    };
    sb_block[..sb_bytes.len()].copy_from_slice(sb_bytes);

    device.write_block_raw(0, &sb_block)?;
    device.write_block_raw(1, &sb_block)?;

    device.sync().map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "Sync failed"))?;

    if verbose {
        println!("\nWFS v3 filesystem created successfully!");
    }

    Ok(())
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

    // Dispatch based on version
    if args.v3 {
        // Create WFS v3 CoW filesystem
        println!("Creating WFS v3 (CoW) disk image: {}", args.output.display());
        println!("  Size: {} bytes ({} blocks)", size, size / V3_BLOCK_SIZE as u64);

        create_wfs_v3(&args.output, size, args.verbose)?;

        if args.dir.is_some() {
            println!("\nNote: v3 directory population not yet implemented.");
            println!("      Use v2 format or populate files after mounting.");
        }

        println!("\nDone! WFS v3 filesystem created.");
        return Ok(());
    }

    // Create WFS v2 filesystem (original behavior)
    let max_files = args.max_files.clamp(MIN_MAX_FILES as u32, MAX_MAX_FILES as u32);
    let total_blocks = size / BLOCK_SIZE as u64;

    println!("Creating WFS v2 disk image: {}", args.output.display());
    println!("  Size: {} bytes ({} blocks)", size, total_blocks);
    println!("  Max files: {}", max_files);

    let layout = DiskLayout::calculate(total_blocks, max_files);
    println!("  Bitmap: {} blocks @ block {}", layout.bitmap_blocks, layout.bitmap_block);
    println!("  File table: {} blocks @ block {}", layout.filetable_blocks, layout.filetable_block);
    println!("  Data starts: block {}", layout.data_start_block);
    println!("  Backup superblocks: {} and {}", layout.mid_superblock, layout.last_superblock);

    let mut img = WfsImage::create(&args.output, size, max_files)?;

    // Add files from directory if specified (recursively)
    if let Some(dir) = &args.dir {
        println!("\nAdding files from: {}", dir.display());

        // Collect all files recursively
        let mut files_to_add: Vec<(String, PathBuf)> = Vec::new();
        collect_files_recursive(dir, dir, &mut files_to_add)?;

        for (relative_path, full_path) in files_to_add {
            let data = fs::read(&full_path)?;

            let mut flags = 0u16;
            if is_executable(&relative_path) {
                flags |= FLAG_EXEC;
            }

            img.add_file(&relative_path, &data, flags, args.verbose)?;
        }
    }

    img.finalize(args.verbose)?;

    println!("\nDone! {} files written, {} blocks free.",
             img.file_count,
             img.layout.total_blocks - img.bitmap.iter().map(|&b| b.count_ones() as u64).sum::<u64>());
    Ok(())
}
