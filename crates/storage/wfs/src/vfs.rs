//! WFS VFS Adapter
//!
//! Implements the VFS Filesystem trait for WFS v2 (flat file system).

#![cfg(feature = "vfs")]

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use watos_vfs::{
    DirEntry, FileMode, FileOperations, FileStat, FileType, Filesystem, FsStats,
    SeekFrom, VfsError, VfsResult,
};
use watos_driver_traits::block::BlockDevice;

use crate::{
    Superblock, FileEntry, WFS_MAGIC, BLOCK_SIZE, ENTRIES_PER_BLOCK,
    FLAG_DIR, FLAG_EXEC, FLAG_READONLY,
};

/// WFS Filesystem VFS adapter
pub struct WfsFilesystem<D: BlockDevice + Send + Sync + 'static> {
    inner: Mutex<WfsInner<D>>,
}

struct WfsInner<D: BlockDevice> {
    device: D,
    superblock: Superblock,
}

impl<D: BlockDevice + Send + Sync + 'static> WfsFilesystem<D> {
    /// Create a new WFS filesystem from a block device
    pub fn new(mut device: D) -> VfsResult<Self> {
        // Read superblock from block 0
        let mut buf = [0u8; 4096];
        if device.read_sectors(0, &mut buf[..512]).is_err() {
            return Err(VfsError::IoError);
        }
        // WFS uses 4096-byte blocks, so read the rest
        for i in 1..8 {
            if device.read_sectors(i, &mut buf[i as usize * 512..(i as usize + 1) * 512]).is_err() {
                return Err(VfsError::IoError);
            }
        }

        // Parse superblock
        let superblock = unsafe {
            core::ptr::read(buf.as_ptr() as *const Superblock)
        };

        // Verify magic
        if superblock.magic != WFS_MAGIC {
            return Err(VfsError::InvalidArgument);
        }

        Ok(WfsFilesystem {
            inner: Mutex::new(WfsInner { device, superblock }),
        })
    }

    /// Probe if a device contains a valid WFS filesystem
    pub fn probe(device: &mut D) -> bool {
        let mut buf = [0u8; 512];
        if device.read_sectors(0, &mut buf).is_err() {
            return false;
        }
        let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        magic == WFS_MAGIC
    }
}

impl<D: BlockDevice + Send + Sync + 'static> WfsInner<D> {
    /// Read a file entry by index
    fn read_file_entry(&mut self, index: usize) -> VfsResult<FileEntry> {
        if index >= self.superblock.max_files as usize {
            return Err(VfsError::NotFound);
        }

        let entries_per_block = ENTRIES_PER_BLOCK;
        let block_offset = index / entries_per_block;
        let entry_offset = index % entries_per_block;

        let block_num = self.superblock.filetable_block + block_offset as u64;

        // Read the block
        let mut buf = [0u8; 4096];
        for i in 0..8 {
            let sector = block_num * 8 + i;
            if self.device.read_sectors(sector, &mut buf[i as usize * 512..(i as usize + 1) * 512]).is_err() {
                return Err(VfsError::IoError);
            }
        }

        // Parse file entry
        let entry_size = 128; // FILEENTRY_SIZE
        let offset = entry_offset * entry_size;
        let entry = unsafe {
            core::ptr::read(buf[offset..].as_ptr() as *const FileEntry)
        };

        Ok(entry)
    }

    /// Find a file by name in the root directory
    /// Optimized to read blocks efficiently instead of one entry at a time
    fn find_file(&mut self, name: &str) -> VfsResult<(usize, FileEntry)> {
        let max_files = self.superblock.max_files as usize;
        let root_files = self.superblock.root_files as usize;
        let entries_per_block = ENTRIES_PER_BLOCK;
        let blocks_needed = (max_files + entries_per_block - 1) / entries_per_block;
        let filetable_start = self.superblock.filetable_block;

        let mut found_count = 0;
        let mut block_buf = [0u8; 4096];

        for block_idx in 0..blocks_needed {
            // Stop if we've checked all files
            if found_count >= root_files {
                break;
            }

            let block_num = filetable_start + block_idx as u64;

            // Read the entire block (8 sectors)
            for i in 0..8u64 {
                let sector = block_num * 8 + i;
                if self.device.read_sectors(sector, &mut block_buf[i as usize * 512..(i as usize + 1) * 512]).is_err() {
                    return Err(VfsError::IoError);
                }
            }

            // Check all 32 entries in this block
            for entry_idx in 0..entries_per_block {
                let global_idx = block_idx * entries_per_block + entry_idx;
                if global_idx >= max_files {
                    break;
                }

                let offset = entry_idx * 128; // FILEENTRY_SIZE
                let entry = unsafe {
                    core::ptr::read(block_buf[offset..].as_ptr() as *const FileEntry)
                };

                if entry.is_valid() {
                    found_count += 1;
                    if entry.name_str().eq_ignore_ascii_case(name) {
                        return Ok((global_idx, entry));
                    }
                }
            }
        }

        Err(VfsError::NotFound)
    }

    /// Read file data
    fn read_file_data(&mut self, entry: &FileEntry, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        if offset >= entry.size {
            return Ok(0);
        }

        let to_read = core::cmp::min(buf.len() as u64, entry.size - offset) as usize;
        let start_block = entry.start_block;

        // Calculate which block and offset within block
        let block_size = BLOCK_SIZE as u64;
        let start_block_offset = offset / block_size;
        let offset_in_block = (offset % block_size) as usize;

        let mut bytes_read = 0;
        let mut current_block = start_block + start_block_offset;
        let mut current_offset = offset_in_block;

        let mut block_buf = [0u8; 4096];

        while bytes_read < to_read {
            // Read the block
            for i in 0..8 {
                let sector = current_block * 8 + i;
                if self.device.read_sectors(sector, &mut block_buf[i as usize * 512..(i as usize + 1) * 512]).is_err() {
                    return Err(VfsError::IoError);
                }
            }

            let available = BLOCK_SIZE as usize - current_offset;
            let to_copy = core::cmp::min(available, to_read - bytes_read);

            buf[bytes_read..bytes_read + to_copy]
                .copy_from_slice(&block_buf[current_offset..current_offset + to_copy]);

            bytes_read += to_copy;
            current_block += 1;
            current_offset = 0; // Only first block has offset
        }

        Ok(bytes_read)
    }
}

impl<D: BlockDevice + Send + Sync + 'static> Filesystem for WfsFilesystem<D> {
    fn name(&self) -> &'static str {
        "WFS"
    }

    fn open(&self, path: &str, mode: FileMode) -> VfsResult<Box<dyn FileOperations>> {
        let mut inner = self.inner.lock();

        let path = path.trim_start_matches('/').trim_start_matches('\\');
        if path.is_empty() {
            return Err(VfsError::IsADirectory);
        }

        let (index, entry) = inner.find_file(path)?;

        if entry.is_directory() {
            return Err(VfsError::IsADirectory);
        }

        if mode.write && entry.is_readonly() {
            return Err(VfsError::PermissionDenied);
        }

        // For now, only read mode is supported
        if mode.write {
            return Err(VfsError::ReadOnly);
        }

        Ok(Box::new(WfsFileHandle {
            index,
            start_block: entry.start_block,
            file_size: entry.size,
            position: 0,
            can_read: mode.read,
            can_write: mode.write,
            flags: entry.flags,
        }))
    }

    fn stat(&self, path: &str) -> VfsResult<FileStat> {
        let mut inner = self.inner.lock();

        let path = path.trim_start_matches('/').trim_start_matches('\\');

        // Root directory
        if path.is_empty() {
            return Ok(FileStat {
                file_type: FileType::Directory,
                size: 0,
                nlink: 1,
                inode: 0,
                dev: 0,
                mode: 0o755,
                uid: 0,
                gid: 0,
                blksize: BLOCK_SIZE,
                blocks: 0,
                atime: 0,
                mtime: 0,
                ctime: 0,
            });
        }

        let (_index, entry) = inner.find_file(path)?;

        Ok(FileStat {
            file_type: if entry.is_directory() {
                FileType::Directory
            } else {
                FileType::Regular
            },
            size: entry.size,
            nlink: 1,
            inode: entry.start_block,
            dev: 0,
            mode: if entry.is_readonly() { 0o444 } else { 0o644 },
            uid: 0,
            gid: 0,
            blksize: BLOCK_SIZE,
            blocks: entry.blocks as u64,
            atime: 0,
            mtime: entry.modified,
            ctime: entry.created,
        })
    }

    fn mkdir(&self, _path: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn unlink(&self, _path: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn rmdir(&self, _path: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn readdir(&self, path: &str) -> VfsResult<Vec<DirEntry>> {
        let mut inner = self.inner.lock();

        let path = path.trim_start_matches('/').trim_start_matches('\\');

        // For non-root paths, verify it's a valid directory
        if !path.is_empty() {
            let (_index, entry) = inner.find_file(path)?;
            if !entry.is_directory() {
                return Err(VfsError::NotADirectory);
            }
        }

        // Build prefix for filtering (e.g., "apps" -> "apps/")
        let prefix = if path.is_empty() {
            String::new()
        } else {
            let mut p = String::from(path);
            if !p.ends_with('/') {
                p.push('/');
            }
            p
        };

        let mut entries = Vec::new();
        let root_files = inner.superblock.root_files as usize;
        let max_files = inner.superblock.max_files as usize;

        let entries_per_block = ENTRIES_PER_BLOCK;
        let blocks_needed = (max_files + entries_per_block - 1) / entries_per_block;
        let filetable_start = inner.superblock.filetable_block;

        let mut found_count = 0;
        let mut block_buf = [0u8; 4096];

        for block_idx in 0..blocks_needed {
            if found_count >= root_files {
                break;
            }

            let block_num = filetable_start + block_idx as u64;

            for i in 0..8u64 {
                let sector = block_num * 8 + i;
                if inner.device.read_sectors(sector, &mut block_buf[i as usize * 512..(i as usize + 1) * 512]).is_err() {
                    return Err(VfsError::IoError);
                }
            }

            for entry_idx in 0..entries_per_block {
                let global_idx = block_idx * entries_per_block + entry_idx;
                if global_idx >= max_files {
                    break;
                }

                let offset = entry_idx * 128;
                let entry = unsafe {
                    core::ptr::read(block_buf[offset..].as_ptr() as *const FileEntry)
                };

                if entry.is_valid() {
                    found_count += 1;
                    let entry_name = entry.name_str();

                    // Filter entries based on path:
                    // - For root (""): show entries without '/' (top-level only)
                    // - For subdir ("apps/"): show entries starting with "apps/"
                    //   and extract just the next component
                    if prefix.is_empty() {
                        // Root directory: only show top-level entries (no '/' in name)
                        if !entry_name.contains('/') {
                            entries.push(DirEntry {
                                name: String::from(entry_name),
                                file_type: if entry.is_directory() {
                                    FileType::Directory
                                } else {
                                    FileType::Regular
                                },
                                size: entry.size,
                                inode: entry.start_block,
                            });
                        }
                    } else {
                        // Subdirectory: check if entry starts with prefix
                        if entry_name.eq_ignore_ascii_case(&prefix[..prefix.len()-1]) {
                            // This is the directory itself, skip it
                            continue;
                        }

                        // Case-insensitive prefix check
                        let entry_lower = entry_name.to_ascii_lowercase();
                        let prefix_lower = prefix.to_ascii_lowercase();

                        if entry_lower.starts_with(&prefix_lower) {
                            // Extract the part after the prefix
                            let remainder = &entry_name[prefix.len()..];

                            // Get just the next component (up to next '/' or end)
                            let component = if let Some(slash_pos) = remainder.find('/') {
                                &remainder[..slash_pos]
                            } else {
                                remainder
                            };

                            // Check if we already have this entry (dedup for nested paths)
                            let already_exists = entries.iter().any(|e|
                                e.name.eq_ignore_ascii_case(component)
                            );

                            if !already_exists && !component.is_empty() {
                                // Determine if this component is a directory
                                // It's a directory if:
                                // 1. The entry itself is a directory matching exactly prefix+component
                                // 2. Or there's a '/' after the component (nested path)
                                let is_dir = entry.is_directory() && remainder == component
                                    || remainder.len() > component.len();

                                entries.push(DirEntry {
                                    name: String::from(component),
                                    file_type: if is_dir {
                                        FileType::Directory
                                    } else {
                                        FileType::Regular
                                    },
                                    size: if is_dir { 0 } else { entry.size },
                                    inode: entry.start_block,
                                });
                            }
                        }
                    }

                    if found_count >= root_files {
                        break;
                    }
                }
            }
        }

        Ok(entries)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn sync(&self) -> VfsResult<()> {
        Ok(())
    }

    fn statfs(&self) -> VfsResult<FsStats> {
        let inner = self.inner.lock();

        Ok(FsStats {
            total_blocks: inner.superblock.total_blocks,
            free_blocks: inner.superblock.free_blocks,
            block_size: BLOCK_SIZE,
            total_inodes: inner.superblock.max_files as u64,
            free_inodes: inner.superblock.max_files as u64 - inner.superblock.root_files as u64,
            max_name_len: 56,
        })
    }
}

/// WFS file handle
struct WfsFileHandle {
    index: usize,
    start_block: u64,
    file_size: u64,
    position: u64,
    can_read: bool,
    can_write: bool,
    flags: u16,
}

impl FileOperations for WfsFileHandle {
    fn read(&mut self, _buffer: &mut [u8]) -> VfsResult<usize> {
        if !self.can_read {
            return Err(VfsError::PermissionDenied);
        }

        if self.position >= self.file_size {
            return Ok(0);
        }

        // Note: We can't access the device here without storing a reference
        // This is a limitation - would need Arc<Mutex<WfsInner>> for proper file handles
        // For now, return what we can
        Err(VfsError::NotSupported)
    }

    fn write(&mut self, _buffer: &[u8]) -> VfsResult<usize> {
        if !self.can_write {
            return Err(VfsError::PermissionDenied);
        }
        Err(VfsError::ReadOnly)
    }

    fn seek(&mut self, offset: i64, whence: SeekFrom) -> VfsResult<u64> {
        let new_pos = match whence {
            SeekFrom::Start => offset as u64,
            SeekFrom::Current => {
                if offset < 0 {
                    self.position.saturating_sub((-offset) as u64)
                } else {
                    self.position.saturating_add(offset as u64)
                }
            }
            SeekFrom::End => {
                if offset < 0 {
                    self.file_size.saturating_sub((-offset) as u64)
                } else {
                    self.file_size.saturating_add(offset as u64)
                }
            }
        };

        self.position = new_pos.min(self.file_size);
        Ok(self.position)
    }

    fn tell(&self) -> u64 {
        self.position
    }

    fn sync(&mut self) -> VfsResult<()> {
        Ok(())
    }

    fn stat(&self) -> VfsResult<FileStat> {
        Ok(FileStat {
            file_type: FileType::Regular,
            size: self.file_size,
            nlink: 1,
            inode: self.start_block,
            dev: 0,
            mode: if self.flags & FLAG_READONLY != 0 { 0o444 } else { 0o644 },
            uid: 0,
            gid: 0,
            blksize: BLOCK_SIZE,
            blocks: (self.file_size + BLOCK_SIZE as u64 - 1) / BLOCK_SIZE as u64,
            atime: 0,
            mtime: 0,
            ctime: 0,
        })
    }

    fn truncate(&mut self, _size: u64) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }
}
