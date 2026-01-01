//! FAT12/16/32 Filesystem implementation for WATOS
//!
//! Provides a VFS-compatible FAT filesystem driver supporting:
//! - FAT12 (floppy disks, small volumes)
//! - FAT16 (small to medium volumes)
//! - FAT32 (large volumes)

#![no_std]

extern crate alloc;

mod bpb;
mod cluster;
mod dir;
mod file;
mod table;

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

use watos_vfs::{
    DirEntry, FileMode, FileOperations, FileStat, FileType, Filesystem, FsStats,
    SeekFrom, VfsError, VfsResult,
};
use watos_driver_traits::block::BlockDevice;

pub use bpb::{BiosParameterBlock, FatType};
pub use dir::{FatDirEntry, DirEntryIterator};

// Debug macro - only outputs when 'debug' feature is enabled
#[cfg(feature = "debug")]
macro_rules! fat_debug {
    ($($arg:tt)*) => {{
        unsafe {
            $($arg)*
        }
    }};
}

#[cfg(not(feature = "debug"))]
macro_rules! fat_debug {
    ($($arg:tt)*) => {};
}

/// Shared inner state for FAT filesystem
/// This is wrapped in Arc<Mutex<>> so both the filesystem and file handles can access it
struct FatInner<D: BlockDevice> {
    /// Underlying block device
    device: D,
    /// BIOS Parameter Block (parsed from boot sector)
    bpb: BiosParameterBlock,
    /// FAT type (12, 16, or 32)
    fat_type: FatType,
    /// First data sector
    first_data_sector: u64,
    /// Sectors per cluster
    sectors_per_cluster: u32,
    /// Sector size
    sector_size: u32,
}

impl<D: BlockDevice> FatInner<D> {
    /// Convert cluster number to sector number
    fn cluster_to_sector(&self, cluster: u32) -> u64 {
        self.first_data_sector + ((cluster as u64 - 2) * self.sectors_per_cluster as u64)
    }

    /// Read a cluster into a buffer
    fn read_cluster(&mut self, cluster: u32, buffer: &mut [u8]) -> VfsResult<()> {
        let sector = self.cluster_to_sector(cluster);
        let sectors = self.sectors_per_cluster;

        fat_debug! {
            watos_arch::serial_write(b"        [read_cluster] cluster=");
            watos_arch::serial_hex(cluster as u64);
            watos_arch::serial_write(b" sector=");
            watos_arch::serial_hex(sector);
            watos_arch::serial_write(b" count=");
            watos_arch::serial_hex(sectors as u64);
            watos_arch::serial_write(b"\r\n");
        }

        for i in 0..sectors {
            fat_debug! {
                watos_arch::serial_write(b"        [read_cluster] Reading sector ");
                watos_arch::serial_hex(sector + i as u64);
                watos_arch::serial_write(b"...\r\n");
            }

            let offset = (i * self.sector_size) as usize;
            let end = offset + self.sector_size as usize;
            if end > buffer.len() {
                break;
            }

            fat_debug! {
                watos_arch::serial_write(b"        [read_cluster] Calling device.read_sectors...\r\n");
            }

            self.device
                .read_sectors(sector + i as u64, &mut buffer[offset..end])
                .map_err(|_| VfsError::IoError)?;

            fat_debug! {
                watos_arch::serial_write(b"        [read_cluster] Sector read OK\r\n");
            }
        }

        fat_debug! {
            watos_arch::serial_write(b"        [read_cluster] All sectors read\r\n");
        }

        Ok(())
    }

    /// Read next cluster from FAT
    fn next_cluster(&mut self, cluster: u32) -> VfsResult<Option<u32>> {
        table::read_fat_entry(&mut self.device, &self.bpb, self.fat_type, cluster)
    }

    /// Get cluster size in bytes
    fn cluster_size(&self) -> u32 {
        self.sectors_per_cluster * self.sector_size
    }

    /// Find a file/directory by path
    fn find_entry(&mut self, path: &str) -> VfsResult<FatDirEntry> {
        let path = path.trim_start_matches('/');
        if path.is_empty() {
            // Return root directory pseudo-entry
            return Ok(FatDirEntry::root_dir(&self.bpb, self.fat_type));
        }

        let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = FatDirEntry::root_dir(&self.bpb, self.fat_type);

        for component in components {
            if !current.is_directory() {
                return Err(VfsError::NotADirectory);
            }

            current = self.find_in_directory(&current, component)?;
        }

        Ok(current)
    }

    /// Find an entry in a directory
    fn find_in_directory(&mut self, dir: &FatDirEntry, name: &str) -> VfsResult<FatDirEntry> {
        let cluster_size = self.cluster_size() as usize;
        let mut buffer = alloc::vec![0u8; cluster_size];

        let mut cluster = dir.first_cluster();
        if cluster == 0 && self.fat_type != FatType::Fat32 {
            // FAT12/16 root directory
            return self.find_in_root_dir(name);
        }

        while cluster >= 2 {
            self.read_cluster(cluster, &mut buffer)?;

            for entry in DirEntryIterator::new(&buffer) {
                if entry.matches_name(name) {
                    return Ok(entry);
                }
            }

            match self.next_cluster(cluster)? {
                Some(next) => cluster = next,
                None => break,
            }
        }

        Err(VfsError::NotFound)
    }

    /// Find entry in FAT12/16 root directory
    fn find_in_root_dir(&mut self, name: &str) -> VfsResult<FatDirEntry> {
        let root_dir_sectors = ((self.bpb.root_entry_count as u32 * 32)
            + (self.bpb.bytes_per_sector as u32 - 1))
            / self.bpb.bytes_per_sector as u32;

        let root_start = self.bpb.reserved_sector_count as u64
            + (self.bpb.num_fats as u64 * self.bpb.fat_size_16 as u64);

        let mut sector_buf = [0u8; 512];

        for i in 0..root_dir_sectors {
            if self.device.read_sectors(root_start + i as u64, &mut sector_buf).is_err() {
                return Err(VfsError::IoError);
            }

            for entry in DirEntryIterator::new(&sector_buf) {
                if entry.matches_name(name) {
                    return Ok(entry);
                }
            }
        }

        Err(VfsError::NotFound)
    }

    /// Read directory entries
    fn read_directory(&mut self, dir: &FatDirEntry) -> VfsResult<Vec<DirEntry>> {
        let cluster_size = self.cluster_size() as usize;
        let mut buffer = alloc::vec![0u8; cluster_size];
        let mut entries = Vec::new();

        let mut cluster = dir.first_cluster();
        if cluster == 0 && self.fat_type != FatType::Fat32 {
            // FAT12/16 root directory
            return self.read_root_dir_entries();
        }

        while cluster >= 2 {
            self.read_cluster(cluster, &mut buffer)?;

            for fat_entry in DirEntryIterator::new(&buffer) {
                if let Some(vfs_entry) = fat_entry.to_vfs_entry() {
                    entries.push(vfs_entry);
                }
            }

            match self.next_cluster(cluster)? {
                Some(next) => cluster = next,
                None => break,
            }
        }

        Ok(entries)
    }

    /// Read FAT12/16 root directory entries
    fn read_root_dir_entries(&mut self) -> VfsResult<Vec<DirEntry>> {
        let root_dir_sectors = ((self.bpb.root_entry_count as u32 * 32)
            + (self.bpb.bytes_per_sector as u32 - 1))
            / self.bpb.bytes_per_sector as u32;

        let root_start = self.bpb.reserved_sector_count as u64
            + (self.bpb.num_fats as u64 * self.bpb.fat_size_16 as u64);

        let mut sector_buf = [0u8; 512];
        let mut entries = Vec::new();

        for i in 0..root_dir_sectors {
            self.device
                .read_sectors(root_start + i as u64, &mut sector_buf)
                .map_err(|_| VfsError::IoError)?;

            for fat_entry in DirEntryIterator::new(&sector_buf) {
                if let Some(vfs_entry) = fat_entry.to_vfs_entry() {
                    entries.push(vfs_entry);
                }
            }
        }

        Ok(entries)
    }

    /// Read file data from clusters with caching
    fn read_file_data_cached(
        &mut self,
        start_cluster: u32,
        file_size: u64,
        position: u64,
        buffer: &mut [u8],
        cached_cluster: u32,
        cached_position: u64,
    ) -> VfsResult<(usize, u32, u64)> {
        if position >= file_size {
            return Ok((0, cached_cluster, cached_position));
        }

        let cluster_size = self.cluster_size() as u64;
        let bytes_to_read = core::cmp::min(buffer.len() as u64, file_size - position) as usize;

        // Find the cluster containing the current position
        let target_cluster_index = position / cluster_size;
        let target_cluster_pos = target_cluster_index * cluster_size;

        // Use cached cluster if we're reading sequentially forward
        let (mut current_cluster, start_index) = if cached_position <= position
            && cached_cluster >= 2
            && cached_position / cluster_size <= target_cluster_index {
            // Start from cached position
            fat_debug! {
                watos_arch::serial_write(b"      [FAT] Using cache, start_index=");
                watos_arch::serial_hex(cached_position / cluster_size);
                watos_arch::serial_write(b" target=");
                watos_arch::serial_hex(target_cluster_index);
                watos_arch::serial_write(b"\r\n");
            }
            (cached_cluster, cached_position / cluster_size)
        } else {
            // Start from beginning
            fat_debug! {
                watos_arch::serial_write(b"      [FAT] Cache miss, walking from start, target=");
                watos_arch::serial_hex(target_cluster_index);
                watos_arch::serial_write(b"\r\n");
            }
            (start_cluster, 0)
        };

        // Walk the cluster chain to find the right cluster
        let mut walk_count = 0;
        for _ in start_index..target_cluster_index {
            walk_count += 1;
            if walk_count > 10000 {
                // Infinite loop protection
                fat_debug! {
                    watos_arch::serial_write(b"      [FAT] INFINITE LOOP DETECTED\r\n");
                }
                return Err(VfsError::IoError);
            }
            match self.next_cluster(current_cluster)? {
                Some(next) => current_cluster = next,
                None => return Ok((0, cached_cluster, cached_position)), // Unexpected end of chain
            }
        }

        fat_debug! {
            watos_arch::serial_write(b"      [FAT] Walked ");
            watos_arch::serial_hex(walk_count);
            watos_arch::serial_write(b" clusters, current=");
            watos_arch::serial_hex(current_cluster as u64);
            watos_arch::serial_write(b"\r\n");
        }

        // Read data
        let mut bytes_read = 0;
        let mut offset_in_cluster = (position % cluster_size) as usize;

        // Use stack buffer for DMA instead of heap Vec
        // Stack is guaranteed to be identity-mapped for DMA
        let mut cluster_buf = [0u8; 4096];

        fat_debug! {
            watos_arch::serial_write(b"      [FAT] Using stack buffer at ");
            watos_arch::serial_hex(cluster_buf.as_ptr() as u64);
            watos_arch::serial_write(b"\r\n");
        }

        let mut read_count = 0;
        while bytes_read < bytes_to_read && current_cluster >= 2 {
            read_count += 1;
            if read_count > 100 {
                fat_debug! {
                    watos_arch::serial_write(b"      [FAT] Too many cluster reads!\r\n");
                }
                return Err(VfsError::IoError);
            }

            fat_debug! {
                watos_arch::serial_write(b"      [FAT] Reading cluster ");
                watos_arch::serial_hex(current_cluster as u64);
                watos_arch::serial_write(b"\r\n");
            }

            self.read_cluster(current_cluster, &mut cluster_buf[..cluster_size as usize])?;

            fat_debug! {
                watos_arch::serial_write(b"      [FAT] Cluster read done\r\n");
            }

            let available = cluster_size as usize - offset_in_cluster;
            let to_copy = core::cmp::min(available, bytes_to_read - bytes_read);

            buffer[bytes_read..bytes_read + to_copy]
                .copy_from_slice(&cluster_buf[offset_in_cluster..offset_in_cluster + to_copy]);

            bytes_read += to_copy;
            offset_in_cluster = 0; // Only first cluster has offset

            // Move to next cluster
            match self.next_cluster(current_cluster)? {
                Some(next) => current_cluster = next,
                None => break,
            }
        }

        // Return bytes read, final cluster, and position of that cluster
        let final_pos = position + bytes_read as u64;
        let final_cluster_pos = (final_pos / cluster_size) * cluster_size;
        Ok((bytes_read, current_cluster, final_cluster_pos))
    }
}

/// FAT filesystem driver with shared state
pub struct FatFilesystem<D: BlockDevice + Send + Sync + 'static> {
    inner: Arc<Mutex<FatInner<D>>>,
}

impl<D: BlockDevice + Send + Sync + 'static> FatFilesystem<D> {
    /// Create a new FAT filesystem from a block device
    pub fn new(mut device: D) -> VfsResult<Self> {
        // Read boot sector
        let mut boot_sector = [0u8; 512];
        if device.read_sectors(0, &mut boot_sector).is_err() {
            // Debug: failed to read boot sector
            return Err(VfsError::IoError);
        }

        // Parse BPB
        let bpb = match BiosParameterBlock::parse(&boot_sector) {
            Ok(b) => b,
            Err(_) => {
                // Debug: failed to parse BPB
                return Err(VfsError::InvalidArgument);
            }
        };
        let fat_type = bpb.fat_type();

        // Calculate first data sector
        let root_dir_sectors = if bpb.root_entry_count > 0 {
            ((bpb.root_entry_count as u32 * 32) + (bpb.bytes_per_sector as u32 - 1))
                / bpb.bytes_per_sector as u32
        } else {
            0
        };

        let fat_size = if bpb.fat_size_16 != 0 {
            bpb.fat_size_16 as u32
        } else {
            bpb.fat_size_32
        };

        let first_data_sector = bpb.reserved_sector_count as u64
            + (bpb.num_fats as u64 * fat_size as u64)
            + root_dir_sectors as u64;

        let sectors_per_cluster = bpb.sectors_per_cluster as u32;
        let sector_size = bpb.bytes_per_sector as u32;

        let inner = FatInner {
            device,
            bpb,
            fat_type,
            first_data_sector,
            sectors_per_cluster,
            sector_size,
        };

        Ok(FatFilesystem {
            inner: Arc::new(Mutex::new(inner)),
        })
    }

    /// Get the FAT type
    pub fn fat_type(&self) -> FatType {
        self.inner.lock().fat_type
    }
}

impl<D: BlockDevice + Send + Sync + 'static> Filesystem for FatFilesystem<D> {
    fn name(&self) -> &'static str {
        // Can't call fat_type() here safely, just return generic name
        "FAT"
    }

    fn open(&self, path: &str, mode: FileMode) -> VfsResult<Box<dyn FileOperations>> {
        let mut inner = self.inner.lock();
        let entry = inner.find_entry(path)?;

        if entry.is_directory() {
            return Err(VfsError::IsADirectory);
        }

        let can_read = mode.read;
        let can_write = mode.write;

        // For now, only read mode is supported
        if can_write {
            return Err(VfsError::ReadOnly);
        }

        let cluster_size = inner.cluster_size();
        let file = FatFileHandle::new(
            Arc::clone(&self.inner),
            entry,
            cluster_size,
            can_read,
            can_write,
        );

        Ok(Box::new(file))
    }

    fn stat(&self, path: &str) -> VfsResult<FileStat> {
        let mut inner = self.inner.lock();
        let entry = inner.find_entry(path)?;
        let cluster_size = inner.cluster_size();

        Ok(FileStat {
            file_type: if entry.is_directory() {
                FileType::Directory
            } else {
                FileType::Regular
            },
            size: entry.file_size as u64,
            nlink: 1,
            inode: entry.first_cluster() as u64,
            dev: 0,
            mode: if entry.attributes & 0x01 != 0 {
                0o444 // Read-only
            } else {
                0o644 // Read-write
            },
            uid: 0,
            gid: 0,
            blksize: cluster_size,
            blocks: (entry.file_size as u64 + 511) / 512,
            atime: 0,
            mtime: 0,
            ctime: 0,
        })
    }

    fn mkdir(&self, path: &str) -> VfsResult<()> {
        let _ = path;
        Err(VfsError::ReadOnly)
    }

    fn unlink(&self, path: &str) -> VfsResult<()> {
        let _ = path;
        Err(VfsError::ReadOnly)
    }

    fn rmdir(&self, path: &str) -> VfsResult<()> {
        let _ = path;
        Err(VfsError::ReadOnly)
    }

    fn readdir(&self, path: &str) -> VfsResult<Vec<DirEntry>> {
        let mut inner = self.inner.lock();
        let entry = inner.find_entry(path)?;

        if !entry.is_directory() {
            return Err(VfsError::NotADirectory);
        }

        inner.read_directory(&entry)
    }

    fn rename(&self, old_path: &str, new_path: &str) -> VfsResult<()> {
        let _ = (old_path, new_path);
        Err(VfsError::ReadOnly)
    }

    fn sync(&self) -> VfsResult<()> {
        Ok(())
    }

    fn statfs(&self) -> VfsResult<FsStats> {
        let inner = self.inner.lock();

        let total_sectors = if inner.bpb.total_sectors_16 != 0 {
            inner.bpb.total_sectors_16 as u64
        } else {
            inner.bpb.total_sectors_32 as u64
        };

        let fat_size = if inner.bpb.fat_size_16 != 0 {
            inner.bpb.fat_size_16 as u64
        } else {
            inner.bpb.fat_size_32 as u64
        };

        let data_sectors = total_sectors
            - inner.bpb.reserved_sector_count as u64
            - (inner.bpb.num_fats as u64 * fat_size);

        let total_clusters = data_sectors / inner.sectors_per_cluster as u64;

        Ok(FsStats {
            total_blocks: total_clusters,
            free_blocks: 0,
            block_size: inner.sectors_per_cluster * inner.sector_size,
            total_inodes: 0,
            free_inodes: 0,
            max_name_len: 255,
        })
    }

    fn chmod(&self, _path: &str, _mode: u32) -> VfsResult<()> {
        // FAT filesystem doesn't support Unix permissions
        Err(VfsError::NotSupported)
    }

    fn chown(&self, _path: &str, _uid: u32, _gid: u32) -> VfsResult<()> {
        // FAT filesystem doesn't support Unix ownership
        Err(VfsError::NotSupported)
    }
}

/// File handle with shared access to filesystem state
struct FatFileHandle<D: BlockDevice + Send + Sync + 'static> {
    /// Shared filesystem state
    inner: Arc<Mutex<FatInner<D>>>,
    /// Starting cluster of the file
    start_cluster: u32,
    /// File size in bytes
    file_size: u64,
    /// Current position in file
    position: u64,
    /// Cluster size
    cluster_size: u32,
    /// Can read
    can_read: bool,
    /// Can write
    can_write: bool,
    /// File attributes
    attributes: u8,
    /// Cached current cluster (to avoid re-walking chain)
    current_cluster: u32,
    /// Position that current_cluster corresponds to
    cluster_position: u64,
}

impl<D: BlockDevice + Send + Sync + 'static> FatFileHandle<D> {
    fn new(
        inner: Arc<Mutex<FatInner<D>>>,
        entry: FatDirEntry,
        cluster_size: u32,
        can_read: bool,
        can_write: bool,
    ) -> Self {
        let start = entry.first_cluster();
        FatFileHandle {
            inner,
            start_cluster: start,
            file_size: entry.file_size as u64,
            position: 0,
            cluster_size,
            can_read,
            can_write,
            attributes: entry.attributes,
            current_cluster: start,
            cluster_position: 0,
        }
    }
}

impl<D: BlockDevice + Send + Sync + 'static> FileOperations for FatFileHandle<D> {
    fn read(&mut self, buffer: &mut [u8]) -> VfsResult<usize> {
        if !self.can_read {
            return Err(VfsError::PermissionDenied);
        }

        if self.position >= self.file_size {
            return Ok(0);
        }

        fat_debug! {
            watos_arch::serial_write(b"    [FAT] Acquiring inner lock, pos=");
            watos_arch::serial_hex(self.position);
            watos_arch::serial_write(b" cached_cluster=");
            watos_arch::serial_hex(self.current_cluster as u64);
            watos_arch::serial_write(b" cached_pos=");
            watos_arch::serial_hex(self.cluster_position);
            watos_arch::serial_write(b"\r\n");
        }

        let mut inner = self.inner.lock();

        fat_debug! {
            watos_arch::serial_write(b"    [FAT] Inner lock acquired, calling read_file_data_cached...\r\n");
        }

        let (bytes_read, new_cluster, new_cluster_pos) = inner.read_file_data_cached(
            self.start_cluster,
            self.file_size,
            self.position,
            buffer,
            self.current_cluster,
            self.cluster_position,
        )?;

        fat_debug! {
            watos_arch::serial_write(b"    [FAT] read_file_data_cached returned ");
            watos_arch::serial_hex(bytes_read as u64);
            watos_arch::serial_write(b" bytes\r\n");
        }

        self.position += bytes_read as u64;
        self.current_cluster = new_cluster;
        self.cluster_position = new_cluster_pos;
        Ok(bytes_read)
    }

    fn write(&mut self, buffer: &[u8]) -> VfsResult<usize> {
        if !self.can_write {
            return Err(VfsError::PermissionDenied);
        }

        // Write not implemented yet
        let _ = buffer;
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

        // Clamp to file size
        self.position = new_pos.min(self.file_size);

        // Reset cluster cache on seek (will be recalculated on next read)
        self.current_cluster = self.start_cluster;
        self.cluster_position = 0;

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
            inode: self.start_cluster as u64,
            dev: 0,
            mode: if self.attributes & 0x01 != 0 {
                0o444
            } else {
                0o644
            },
            uid: 0,
            gid: 0,
            blksize: self.cluster_size,
            blocks: (self.file_size + 511) / 512,
            atime: 0,
            mtime: 0,
            ctime: 0,
        })
    }

    fn truncate(&mut self, size: u64) -> VfsResult<()> {
        let _ = size;
        Err(VfsError::ReadOnly)
    }
}
