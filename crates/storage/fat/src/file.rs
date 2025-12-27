//! FAT file operations

use watos_vfs::{FileOperations, FileStat, FileType, SeekFrom, VfsError, VfsResult};

use crate::dir::FatDirEntry;

/// Open file handle for FAT filesystem
pub struct FatFile {
    /// Directory entry for this file
    entry: FatDirEntry,
    /// Current position in file
    position: u64,
    /// Current cluster in chain
    current_cluster: u32,
    /// Offset within current cluster
    cluster_offset: u32,
    /// Cluster size in bytes
    cluster_size: u32,
    /// Can read
    can_read: bool,
    /// Can write
    can_write: bool,
}

impl FatFile {
    /// Create a new file handle
    pub fn new(
        entry: FatDirEntry,
        cluster_size: u32,
        can_read: bool,
        can_write: bool,
    ) -> Self {
        FatFile {
            current_cluster: entry.first_cluster(),
            entry,
            position: 0,
            cluster_offset: 0,
            cluster_size,
            can_read,
            can_write,
        }
    }

    /// Get the file size
    pub fn size(&self) -> u64 {
        self.entry.file_size as u64
    }

    /// Check if at end of file
    pub fn is_eof(&self) -> bool {
        self.position >= self.size()
    }
}

impl FileOperations for FatFile {
    fn read(&mut self, buffer: &mut [u8]) -> VfsResult<usize> {
        if !self.can_read {
            return Err(VfsError::PermissionDenied);
        }

        if self.is_eof() {
            return Ok(0);
        }

        // TODO: Implement actual reading from clusters
        // This requires access to the filesystem to read clusters
        // For now, return error indicating incomplete implementation
        let _ = buffer;
        Err(VfsError::IoError)
    }

    fn write(&mut self, buffer: &[u8]) -> VfsResult<usize> {
        if !self.can_write {
            return Err(VfsError::PermissionDenied);
        }

        // TODO: Implement actual writing to clusters
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
                let size = self.size();
                if offset < 0 {
                    size.saturating_sub((-offset) as u64)
                } else {
                    size.saturating_add(offset as u64)
                }
            }
        };

        // Clamp to file size
        self.position = new_pos.min(self.size());

        // Recalculate cluster position
        // TODO: Walk cluster chain to find correct cluster
        self.cluster_offset = (self.position % self.cluster_size as u64) as u32;

        Ok(self.position)
    }

    fn tell(&self) -> u64 {
        self.position
    }

    fn sync(&mut self) -> VfsResult<()> {
        // TODO: Flush any pending writes
        Ok(())
    }

    fn stat(&self) -> VfsResult<FileStat> {
        Ok(FileStat {
            file_type: if self.entry.is_directory() {
                FileType::Directory
            } else {
                FileType::Regular
            },
            size: self.entry.file_size as u64,
            nlink: 1,
            inode: self.entry.first_cluster() as u64,
            dev: 0,
            mode: if self.entry.attributes & 0x01 != 0 {
                0o444 // Read-only
            } else {
                0o644 // Read-write
            },
            uid: 0,
            gid: 0,
            blksize: self.cluster_size,
            blocks: (self.entry.file_size as u64 + 511) / 512,
            atime: 0, // TODO: Convert FAT timestamps
            mtime: 0,
            ctime: 0,
        })
    }

    fn truncate(&mut self, size: u64) -> VfsResult<()> {
        if !self.can_write {
            return Err(VfsError::PermissionDenied);
        }

        // TODO: Implement truncate
        let _ = size;
        Err(VfsError::ReadOnly)
    }
}
