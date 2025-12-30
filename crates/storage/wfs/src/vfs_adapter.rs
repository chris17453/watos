//! WFS VFS Adapter
//!
//! Implements the VFS Filesystem trait for WFS (B+tree CoW filesystem).

#![cfg(feature = "vfs")]

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::sync::Arc;
use spin::Mutex;
use core::cell::UnsafeCell;

use watos_vfs::{
    DirEntry as VfsDirEntry, FileMode, FileOperations, FileStat, FileType,
    Filesystem, FsStats, SeekFrom, VfsError, VfsResult,
};
use watos_driver_traits::block::BlockDevice as VfsBlockDevice;

use crate::core::{
    Superblock, Inode, TreeNode, TreeOps, TreeError, FilesystemState, BlockDevice, BlockAllocator,
    FilesystemOps, InodeOps, DirOps, DirEntry, ExtentOps, FileOps, resolve_path, init_filesystem,
    WFS_MAGIC, BLOCK_SIZE, ROOT_INODE, S_IFDIR, S_IFREG, S_IFLNK, S_IFMT, BPlusTree, NodeType,
};

use crate::core::dir::EntryType;
use crate::core::inode::INODE_INLINE_SIZE;

/// WFS Filesystem VFS adapter
pub struct WfsFilesystem<D: VfsBlockDevice + Send + Sync + 'static> {
    inner: Arc<Mutex<WfsInner<WfsBlockDeviceAdapter<D>>>>,
}

struct WfsInner<D: BlockDevice + BlockAllocator> {
    device: D,
    state: FilesystemState,
}

impl<D: VfsBlockDevice + Send + Sync + 'static> WfsFilesystem<D> {
    /// Create a new WFS filesystem from a block device
    pub fn new(device: D) -> VfsResult<Self> {
        let adapter = WfsBlockDeviceAdapter::new(device);
        let inner = WfsInner::new(adapter)?;

        Ok(Self {
            inner: Arc::new(Mutex::new(inner)),
        })
    }
}

impl<D: BlockDevice + BlockAllocator + FilesystemOps> WfsInner<D> {
    fn new(device: D) -> VfsResult<Self> {
        // Read superblock from block 0
        let superblock = device.read_superblock(0)
            .map_err(|_| VfsError::IoError)?;

        // Verify magic
        if !superblock.is_valid() {
            return Err(VfsError::Corrupted);
        }

        let state = FilesystemState::new(superblock);

        Ok(Self { device, state })
    }

    fn resolve_inode(&mut self, path: &str) -> VfsResult<Inode> {
        // TreeOps needs separate device and allocator references
        // We can use the same device for both since it implements both traits
        let dev_ptr = &mut self.device as *mut D;
        let (dev_ref, alloc_ref) = unsafe { (&mut *dev_ptr, &mut *dev_ptr) };
        let mut ops = TreeOps::new(dev_ref, alloc_ref);

        resolve_path(&mut ops, &self.state, path)
            .map_err(tree_error_to_vfs)?
            .ok_or(VfsError::NotFound)
    }

    fn inode_to_stat(&self, inode: &Inode) -> FileStat {
        let file_type = match inode.mode & S_IFMT {
            S_IFDIR => FileType::Directory,
            S_IFREG => FileType::Regular,
            S_IFLNK => FileType::Symlink,
            _ => FileType::Regular,
        };

        FileStat {
            file_type,
            size: inode.size,
            nlink: inode.nlink,
            inode: inode.inode_num,
            dev: 0,  // TODO: Device ID
            mode: inode.mode,
            uid: inode.uid,
            gid: inode.gid,
            blksize: BLOCK_SIZE,
            blocks: inode.blocks,
            atime: inode.atime,
            mtime: inode.mtime,
            ctime: inode.ctime,
        }
    }
}

impl<D: VfsBlockDevice + Send + Sync + 'static> Filesystem for WfsFilesystem<D> {
    fn name(&self) -> &'static str {
        "wfs"
    }

    fn open(&self, path: &str, mode: FileMode) -> VfsResult<Box<dyn FileOperations>> {
        let mut inner = self.inner.lock();
        let inode = inner.resolve_inode(path)?;

        // Check if it's a regular file
        if (inode.mode & S_IFMT) != S_IFREG {
            return Err(VfsError::IsADirectory);
        }

        Ok(Box::new(WfsFile {
            fs: self.inner.clone(),
            inode,
            position: 0,
            mode,
        }))
    }

    fn stat(&self, path: &str) -> VfsResult<FileStat> {
        let mut inner = self.inner.lock();
        let inode = inner.resolve_inode(path)?;
        Ok(inner.inode_to_stat(&inode))
    }

    fn mkdir(&self, path: &str) -> VfsResult<()> {
        let mut inner = self.inner.lock();

        // Get mutable references to device for TreeOps
        let dev_ptr = &mut inner.device as *mut _;
        let (dev_ref, alloc_ref) = unsafe { (&mut *dev_ptr, &mut *dev_ptr) };
        let mut ops = TreeOps::new(dev_ref, alloc_ref);

        // Parse parent and name
        let (parent_path, name) = split_path(path)?;

        // Resolve parent directory
        let mut parent_inode = resolve_path(&mut ops, &inner.state, parent_path)
            .map_err(tree_error_to_vfs)?
            .ok_or(VfsError::NotFound)?;

        // Check parent is a directory
        if (parent_inode.mode & S_IFMT) != S_IFDIR {
            return Err(VfsError::NotADirectory);
        }

        // Allocate new inode number
        let inode_num = InodeOps::allocate_inode_num(&mut inner.state);

        // Allocate a block for the new directory's B+tree
        let dir_tree_block = inner.device.allocate_block()
            .map_err(tree_error_to_vfs)?;

        // Create directory inode with its own B+tree
        let mut dir_inode = Inode::new(inode_num, S_IFDIR | 0o755);
        dir_inode.extent_root = dir_tree_block; // Directory uses extent_root for its B+tree
        dir_inode.nlink = 2; // . and parent's reference

        // Create empty B+tree node for new directory
        let dir_tree_node = TreeNode::new(NodeType::Directory, 0, inner.state.superblock.root_generation);
        inner.device.write_node(dir_tree_block, &dir_tree_node)
            .map_err(tree_error_to_vfs)?;

        // Insert directory inode into inode tree
        InodeOps::insert(&mut ops, &mut inner.state, dir_inode)
            .map_err(tree_error_to_vfs)?;

        // Create directory entry in parent
        let entry = DirEntry::new(name, inode_num, EntryType::Directory)
            .ok_or(VfsError::InvalidArgument)?;

        // Get parent's directory tree and insert entry
        let mut parent_dir_tree = BPlusTree::new(
            parent_inode.extent_root,
            NodeType::Directory,
            inner.state.superblock.root_generation,
        );

        DirOps::insert(&mut ops, &mut parent_dir_tree, entry)
            .map_err(tree_error_to_vfs)?;

        // Update parent inode with new tree root (may have changed due to CoW)
        parent_inode.extent_root = parent_dir_tree.root_block;
        InodeOps::insert(&mut ops, &mut inner.state, parent_inode)
            .map_err(tree_error_to_vfs)?;

        Ok(())
    }

    fn unlink(&self, path: &str) -> VfsResult<()> {
        let mut inner = self.inner.lock();

        // Get mutable references for TreeOps
        let dev_ptr = &mut inner.device as *mut _;
        let (dev_ref, alloc_ref) = unsafe { (&mut *dev_ptr, &mut *dev_ptr) };
        let mut ops = TreeOps::new(dev_ref, alloc_ref);

        // Parse parent and name
        let (parent_path, name) = split_path(path)?;

        // Resolve file and parent
        let file_inode = inner.resolve_inode(path)?;
        let mut parent_inode = resolve_path(&mut ops, &inner.state, parent_path)
            .map_err(tree_error_to_vfs)?
            .ok_or(VfsError::NotFound)?;

        // Check it's a regular file
        if (file_inode.mode & S_IFMT) != S_IFREG {
            return Err(VfsError::IsADirectory);
        }

        // Remove from parent directory tree
        let mut parent_dir_tree = BPlusTree::new(
            parent_inode.extent_root,
            NodeType::Directory,
            inner.state.superblock.root_generation,
        );

        DirOps::delete(&mut ops, &mut parent_dir_tree, name)
            .map_err(tree_error_to_vfs)?;

        // Update parent inode
        parent_inode.extent_root = parent_dir_tree.root_block;
        InodeOps::insert(&mut ops, &mut inner.state, parent_inode)
            .map_err(tree_error_to_vfs)?;

        // Delete file inode
        InodeOps::delete(&mut ops, &mut inner.state, file_inode.inode_num)
            .map_err(tree_error_to_vfs)?;

        // TODO: Free data blocks (extent tree cleanup)

        Ok(())
    }

    fn rmdir(&self, path: &str) -> VfsResult<()> {
        let mut inner = self.inner.lock();

        // Get mutable references for TreeOps
        let dev_ptr = &mut inner.device as *mut _;
        let (dev_ref, alloc_ref) = unsafe { (&mut *dev_ptr, &mut *dev_ptr) };
        let mut ops = TreeOps::new(dev_ref, alloc_ref);

        // Parse parent and name
        let (parent_path, name) = split_path(path)?;

        // Resolve directory and parent
        let dir_inode = inner.resolve_inode(path)?;
        let mut parent_inode = resolve_path(&mut ops, &inner.state, parent_path)
            .map_err(tree_error_to_vfs)?
            .ok_or(VfsError::NotFound)?;

        // Check it's a directory
        if (dir_inode.mode & S_IFMT) != S_IFDIR {
            return Err(VfsError::NotADirectory);
        }

        // TODO: Check directory is empty by iterating its B+tree

        // Remove from parent directory tree
        let mut parent_dir_tree = BPlusTree::new(
            parent_inode.extent_root,
            NodeType::Directory,
            inner.state.superblock.root_generation,
        );

        DirOps::delete(&mut ops, &mut parent_dir_tree, name)
            .map_err(tree_error_to_vfs)?;

        // Update parent inode
        parent_inode.extent_root = parent_dir_tree.root_block;
        InodeOps::insert(&mut ops, &mut inner.state, parent_inode)
            .map_err(tree_error_to_vfs)?;

        // Delete directory inode
        InodeOps::delete(&mut ops, &mut inner.state, dir_inode.inode_num)
            .map_err(tree_error_to_vfs)?;

        // TODO: Free directory tree block

        Ok(())
    }

    fn readdir(&self, path: &str) -> VfsResult<Vec<VfsDirEntry>> {
        let mut inner = self.inner.lock();
        let inode = inner.resolve_inode(path)?;

        // Check it's a directory
        if (inode.mode & S_IFMT) != S_IFDIR {
            return Err(VfsError::NotADirectory);
        }

        // TODO: Implement B+tree iteration to read directory entries
        // For now, return empty - this requires implementing tree scan functionality
        Ok(Vec::new())
    }

    fn rename(&self, old_path: &str, new_path: &str) -> VfsResult<()> {
        let mut inner = self.inner.lock();

        // Get mutable references for TreeOps
        let dev_ptr = &mut inner.device as *mut _;
        let (dev_ref, alloc_ref) = unsafe { (&mut *dev_ptr, &mut *dev_ptr) };
        let mut ops = TreeOps::new(dev_ref, alloc_ref);

        // Parse paths
        let (old_parent_path, old_name) = split_path(old_path)?;
        let (new_parent_path, new_name) = split_path(new_path)?;

        // Resolve file and parents
        let file_inode = inner.resolve_inode(old_path)?;
        let mut old_parent = resolve_path(&mut ops, &inner.state, old_parent_path)
            .map_err(tree_error_to_vfs)?
            .ok_or(VfsError::NotFound)?;
        let mut new_parent = resolve_path(&mut ops, &inner.state, new_parent_path)
            .map_err(tree_error_to_vfs)?
            .ok_or(VfsError::NotFound)?;

        // Remove from old parent
        let mut old_parent_tree = BPlusTree::new(
            old_parent.extent_root,
            NodeType::Directory,
            inner.state.superblock.root_generation,
        );

        let old_entry = DirOps::delete(&mut ops, &mut old_parent_tree, old_name)
            .map_err(tree_error_to_vfs)?;

        old_parent.extent_root = old_parent_tree.root_block;
        InodeOps::insert(&mut ops, &mut inner.state, old_parent)
            .map_err(tree_error_to_vfs)?;

        // Create new entry with new name
        let new_entry = DirEntry::new(new_name, file_inode.inode_num, old_entry.entry_type())
            .ok_or(VfsError::InvalidArgument)?;

        // Add to new parent
        let mut new_parent_tree = BPlusTree::new(
            new_parent.extent_root,
            NodeType::Directory,
            inner.state.superblock.root_generation,
        );

        DirOps::insert(&mut ops, &mut new_parent_tree, new_entry)
            .map_err(tree_error_to_vfs)?;

        new_parent.extent_root = new_parent_tree.root_block;
        InodeOps::insert(&mut ops, &mut inner.state, new_parent)
            .map_err(tree_error_to_vfs)?;

        Ok(())
    }

    fn sync(&self) -> VfsResult<()> {
        // WFS is CoW - writes are atomic
        // TODO: Flush pending transactions
        Ok(())
    }

    fn statfs(&self) -> VfsResult<FsStats> {
        let inner = self.inner.lock();
        let sb = &inner.state.superblock;

        Ok(FsStats {
            block_size: BLOCK_SIZE,
            total_blocks: sb.total_blocks,
            free_blocks: sb.free_blocks,
            total_inodes: sb.inode_count,
            free_inodes: u64::MAX, // Unlimited in B+tree
            max_name_len: 255,     // Standard max filename length
        })
    }

    fn chmod(&self, path: &str, mode: u32) -> VfsResult<()> {
        let mut inner = self.inner.lock();

        // Get mutable references for TreeOps
        let dev_ptr = &mut inner.device as *mut _;
        let (dev_ref, alloc_ref) = unsafe { (&mut *dev_ptr, &mut *dev_ptr) };
        let mut ops = TreeOps::new(dev_ref, alloc_ref);

        let mut inode = inner.resolve_inode(path)?;

        // Update mode (preserve file type bits)
        inode.mode = (inode.mode & S_IFMT) | (mode & !S_IFMT);

        // Update inode (takes inode by value)
        InodeOps::insert(&mut ops, &mut inner.state, inode)
            .map_err(tree_error_to_vfs)?;

        Ok(())
    }

    fn chown(&self, path: &str, uid: u32, gid: u32) -> VfsResult<()> {
        let mut inner = self.inner.lock();

        // Get mutable references for TreeOps
        let dev_ptr = &mut inner.device as *mut _;
        let (dev_ref, alloc_ref) = unsafe { (&mut *dev_ptr, &mut *dev_ptr) };
        let mut ops = TreeOps::new(dev_ref, alloc_ref);

        let mut inode = inner.resolve_inode(path)?;

        inode.uid = uid;
        inode.gid = gid;

        // Update inode (takes inode by value)
        InodeOps::insert(&mut ops, &mut inner.state, inode)
            .map_err(tree_error_to_vfs)?;

        Ok(())
    }
}

/// WFS file handle
struct WfsFile<D: VfsBlockDevice + Send + Sync + 'static> {
    fs: Arc<Mutex<WfsInner<WfsBlockDeviceAdapter<D>>>>,
    inode: Inode,
    position: u64,
    mode: FileMode,
}

impl<D: VfsBlockDevice + Send + Sync + 'static> FileOperations for WfsFile<D> {
    fn read(&mut self, buffer: &mut [u8]) -> VfsResult<usize> {
        if !self.mode.read {
            return Err(VfsError::PermissionDenied);
        }

        // For now, only support inline data
        if self.inode.inline_size > 0 {
            let inline_len = self.inode.inline_size as u64;
            if self.position >= inline_len {
                return Ok(0); // EOF
            }

            let available = (inline_len - self.position) as usize;
            let to_read = buffer.len().min(available);

            buffer[..to_read].copy_from_slice(
                &self.inode.inline_data[self.position as usize..self.position as usize + to_read]
            );

            self.position += to_read as u64;
            Ok(to_read)
        } else {
            // TODO: Implement extent-based file read
            // This requires reading from extent tree and data blocks
            Err(VfsError::NotSupported)
        }
    }

    fn write(&mut self, buffer: &[u8]) -> VfsResult<usize> {
        if !self.mode.write {
            return Err(VfsError::PermissionDenied);
        }

        // For now, only support inline data
        let max_inline = INODE_INLINE_SIZE as u64;
        if self.position + buffer.len() as u64 <= max_inline {
            // Write to inline data
            let end_pos = self.position as usize + buffer.len();

            // Update inode inline data
            let mut fs = self.fs.lock();
            self.inode.inline_data[self.position as usize..end_pos].copy_from_slice(buffer);
            self.inode.inline_size = end_pos as u16;
            self.inode.size = end_pos as u64;

            // Update inode on disk
            let dev_ptr = &mut fs.device as *mut _;
            let (dev_ref, alloc_ref) = unsafe { (&mut *dev_ptr, &mut *dev_ptr) };
            let mut ops = TreeOps::new(dev_ref, alloc_ref);

            InodeOps::insert(&mut ops, &mut fs.state, self.inode)
                .map_err(tree_error_to_vfs)?;

            self.position += buffer.len() as u64;
            Ok(buffer.len())
        } else {
            // TODO: Implement extent-based file write
            Err(VfsError::NotSupported)
        }
    }

    fn seek(&mut self, offset: i64, whence: SeekFrom) -> VfsResult<u64> {
        let new_pos = match whence {
            SeekFrom::Start => offset as u64,
            SeekFrom::Current => (self.position as i64 + offset) as u64,
            SeekFrom::End => (self.inode.size as i64 + offset) as u64,
        };

        self.position = new_pos;
        Ok(new_pos)
    }

    fn tell(&self) -> u64 {
        self.position
    }

    fn sync(&mut self) -> VfsResult<()> {
        // WFS is CoW - writes are atomic
        Ok(())
    }

    fn stat(&self) -> VfsResult<FileStat> {
        let file_type = match self.inode.mode & S_IFMT {
            S_IFDIR => FileType::Directory,
            S_IFREG => FileType::Regular,
            S_IFLNK => FileType::Symlink,
            _ => FileType::Regular,
        };

        Ok(FileStat {
            file_type,
            size: self.inode.size,
            nlink: self.inode.nlink,
            inode: self.inode.inode_num,
            dev: 0,  // TODO: Device ID
            mode: self.inode.mode,
            uid: self.inode.uid,
            gid: self.inode.gid,
            blksize: BLOCK_SIZE,
            blocks: self.inode.blocks,
            atime: self.inode.atime,
            mtime: self.inode.mtime,
            ctime: self.inode.ctime,
        })
    }

    fn truncate(&mut self, size: u64) -> VfsResult<()> {
        // TODO: Implement truncate
        self.inode.size = size;
        Ok(())
    }
}

/// Adapter to bridge VFS BlockDevice trait to WFS BlockDevice trait
struct WfsBlockDeviceAdapter<D: VfsBlockDevice> {
    // UnsafeCell provides interior mutability for read_node which takes &self
    device: UnsafeCell<D>,
}

impl<D: VfsBlockDevice> WfsBlockDeviceAdapter<D> {
    fn new(device: D) -> Self {
        Self { device: UnsafeCell::new(device) }
    }
}

impl<D: VfsBlockDevice> BlockDevice for WfsBlockDeviceAdapter<D> {
    fn read_node(&self, block: u64) -> Result<TreeNode, TreeError> {
        let mut node = TreeNode::default();
        let sector = block * (BLOCK_SIZE / 512) as u64;

        // Read entire node (header + data) from disk
        // SAFETY: TreeNode is repr(C) and exactly BLOCK_SIZE bytes
        let node_bytes = unsafe {
            core::slice::from_raw_parts_mut(
                &mut node as *mut TreeNode as *mut u8,
                BLOCK_SIZE as usize,
            )
        };

        // SAFETY: UnsafeCell provides interior mutability. Safe because device is
        // protected by Mutex at VfsFilesystem level.
        let device = unsafe { &mut *self.device.get() };
        device.read_sectors(sector, node_bytes)
            .map_err(|_| TreeError::IoError)?;

        Ok(node)
    }

    fn write_node(&mut self, block: u64, node: &TreeNode) -> Result<(), TreeError> {
        let sector = block * (BLOCK_SIZE / 512) as u64;

        // Write entire node (header + data) to disk
        // SAFETY: TreeNode is repr(C) and exactly BLOCK_SIZE bytes
        let node_bytes = unsafe {
            core::slice::from_raw_parts(
                node as *const TreeNode as *const u8,
                BLOCK_SIZE as usize,
            )
        };

        // SAFETY: Safe to access UnsafeCell with mutable reference
        let device = unsafe { &mut *self.device.get() };
        device.write_sectors(sector, node_bytes)
            .map_err(|_| TreeError::IoError)?;

        Ok(())
    }

    fn sync(&mut self) -> Result<(), TreeError> {
        // VFS BlockDevice trait doesn't have sync method
        // For now, assume writes are synchronous
        Ok(())
    }
}

impl<D: VfsBlockDevice> BlockAllocator for WfsBlockDeviceAdapter<D> {
    fn allocate_block(&mut self) -> Result<u64, TreeError> {
        // TODO: Implement using FreeSpaceOps
        Err(TreeError::IoError)
    }

    fn free_block(&mut self, _block: u64) -> Result<(), TreeError> {
        // TODO: Implement using FreeSpaceOps
        Ok(())
    }
}

use crate::core::transaction::TransactionError;

// FilesystemOps has some methods that need implementation
impl<D: VfsBlockDevice> FilesystemOps for WfsBlockDeviceAdapter<D> {
    fn allocate_blocks(&mut self, _state: &mut FilesystemState, _count: u64) -> Result<u64, TransactionError> {
        // TODO: Implement block allocation
        Err(TransactionError::IoError)
    }

    fn free_blocks(&mut self, _state: &mut FilesystemState, _start: u64, _count: u64) -> Result<(), TransactionError> {
        // TODO: Implement block freeing
        Ok(())
    }
}

/// Split path into (parent, name)
fn split_path(path: &str) -> VfsResult<(&str, &str)> {
    let path = path.trim_end_matches('/');

    if let Some(pos) = path.rfind('/') {
        let parent = if pos == 0 { "/" } else { &path[..pos] };
        let name = &path[pos + 1..];
        Ok((parent, name))
    } else {
        Ok(("/", path))
    }
}

/// Convert TreeError to VfsError
fn tree_error_to_vfs(err: TreeError) -> VfsError {
    match err {
        TreeError::NodeNotFound => VfsError::NotFound,
        TreeError::KeyNotFound => VfsError::NotFound,
        TreeError::CrcError => VfsError::Corrupted,
        TreeError::InvalidNode => VfsError::Corrupted,
        TreeError::NodeFull => VfsError::NoSpace,
        _ => VfsError::IoError,
    }
}
