//! Mount table management

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use crate::{Filesystem, VfsError, VfsResult, MAX_MOUNTS};
use crate::path::normalize;

/// A mount point in the VFS
pub struct MountPoint {
    /// Mount path (normalized)
    pub path: String,
    /// Mounted filesystem
    pub filesystem: Box<dyn Filesystem>,
}

impl MountPoint {
    /// Create a new mount point
    pub fn new(path: &str, filesystem: Box<dyn Filesystem>) -> Self {
        MountPoint {
            path: normalize(path),
            filesystem,
        }
    }
}

/// Mount table managing all mounted filesystems
pub struct MountTable {
    mounts: Vec<MountPoint>,
}

impl MountTable {
    /// Create a new empty mount table
    pub fn new() -> Self {
        MountTable {
            mounts: Vec::new(),
        }
    }

    /// Mount a filesystem at the given path
    pub fn mount(&mut self, path: &str, filesystem: Box<dyn Filesystem>) -> VfsResult<()> {
        let normalized = normalize(path);

        // Check if already mounted
        if self.mounts.iter().any(|m| m.path == normalized) {
            return Err(VfsError::AlreadyMounted);
        }

        // Check mount limit
        if self.mounts.len() >= MAX_MOUNTS {
            return Err(VfsError::TooManyOpenFiles);
        }

        self.mounts.push(MountPoint::new(&normalized, filesystem));

        // Sort by path length descending for longest-prefix matching
        self.mounts.sort_by(|a, b| b.path.len().cmp(&a.path.len()));

        Ok(())
    }

    /// Unmount the filesystem at the given path
    pub fn unmount(&mut self, path: &str) -> VfsResult<()> {
        let normalized = normalize(path);

        let pos = self.mounts.iter().position(|m| m.path == normalized);
        match pos {
            Some(idx) => {
                self.mounts.remove(idx);
                Ok(())
            }
            None => Err(VfsError::NotMounted),
        }
    }

    /// Find the filesystem for a given path
    ///
    /// Returns the filesystem and the path relative to the mount point
    pub fn resolve(&self, path: &str) -> VfsResult<(&dyn Filesystem, String)> {
        let normalized = normalize(path);

        // Find the longest matching mount point
        for mount in &self.mounts {
            if normalized == mount.path {
                // Exact match - root of mount
                return Ok((mount.filesystem.as_ref(), String::from("/")));
            } else if normalized.starts_with(&mount.path) {
                // Check for proper prefix (must be followed by / or be root)
                let after = &normalized[mount.path.len()..];
                if mount.path == "/" || after.starts_with('/') {
                    let rel_path = if mount.path == "/" {
                        normalized.clone()
                    } else {
                        String::from(after)
                    };
                    return Ok((mount.filesystem.as_ref(), rel_path));
                }
            }
        }

        Err(VfsError::NotMounted)
    }

    /// Get list of all mount points
    pub fn list(&self) -> &[MountPoint] {
        &self.mounts
    }

    /// Check if a path is a mount point
    pub fn is_mount_point(&self, path: &str) -> bool {
        let normalized = normalize(path);
        self.mounts.iter().any(|m| m.path == normalized)
    }

    /// Get the mount point for a path
    pub fn get_mount(&self, path: &str) -> Option<&MountPoint> {
        let normalized = normalize(path);
        self.mounts.iter().find(|m| m.path == normalized)
    }
}

impl Default for MountTable {
    fn default() -> Self {
        Self::new()
    }
}
