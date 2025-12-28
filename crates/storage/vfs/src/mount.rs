//! Mount table management
//!
//! Supports two types of mounts:
//! - **Path mounts**: Traditional Unix-style mounts at paths like `/mnt/disk`
//! - **Drive mounts**: Windows-style drive letters like `C:`, `D:`
//!
//! Drive mounts are "jailed" - paths using drive letters cannot escape
//! the mount root via `..`.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use crate::{Filesystem, VfsError, VfsResult, MAX_MOUNTS};
use crate::path::{normalize, parse, PathType};

/// A mount point in the VFS
pub struct MountPoint {
    /// Mount path (normalized, for path mounts)
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

/// A drive letter mount (jailed)
pub struct DriveMount {
    /// Drive letter (uppercase A-Z)
    pub letter: char,
    /// Mounted filesystem
    pub filesystem: Box<dyn Filesystem>,
    /// Optional label for the drive
    pub label: Option<String>,
}

impl DriveMount {
    /// Create a new drive mount
    pub fn new(letter: char, filesystem: Box<dyn Filesystem>) -> Self {
        DriveMount {
            letter: letter.to_ascii_uppercase(),
            filesystem,
            label: None,
        }
    }

    /// Create a new drive mount with label
    pub fn with_label(letter: char, filesystem: Box<dyn Filesystem>, label: &str) -> Self {
        DriveMount {
            letter: letter.to_ascii_uppercase(),
            filesystem,
            label: Some(String::from(label)),
        }
    }
}

/// Maximum number of drive letters (A-Z)
pub const MAX_DRIVES: usize = 26;

/// Mount table managing all mounted filesystems
pub struct MountTable {
    /// Path-based mounts (Unix style)
    mounts: Vec<MountPoint>,
    /// Drive letter mounts (Windows style, jailed)
    drives: [Option<DriveMount>; MAX_DRIVES],
}

/// Helper to convert drive letter to index (A=0, B=1, ..., Z=25)
fn drive_index(letter: char) -> Option<usize> {
    let upper = letter.to_ascii_uppercase();
    if upper >= 'A' && upper <= 'Z' {
        Some((upper as usize) - ('A' as usize))
    } else {
        None
    }
}

impl MountTable {
    /// Create a new empty mount table
    pub fn new() -> Self {
        // Initialize drives array with None
        const NONE_DRIVE: Option<DriveMount> = None;
        MountTable {
            mounts: Vec::new(),
            drives: [NONE_DRIVE; MAX_DRIVES],
        }
    }

    // ========== Path Mount Operations ==========

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

    // ========== Drive Mount Operations ==========

    /// Mount a filesystem as a drive letter
    pub fn mount_drive(&mut self, letter: char, filesystem: Box<dyn Filesystem>) -> VfsResult<()> {
        let idx = drive_index(letter).ok_or(VfsError::InvalidArgument)?;

        if self.drives[idx].is_some() {
            return Err(VfsError::AlreadyMounted);
        }

        self.drives[idx] = Some(DriveMount::new(letter, filesystem));
        Ok(())
    }

    /// Mount a filesystem as a drive letter with a label
    pub fn mount_drive_labeled(&mut self, letter: char, filesystem: Box<dyn Filesystem>, label: &str) -> VfsResult<()> {
        let idx = drive_index(letter).ok_or(VfsError::InvalidArgument)?;

        if self.drives[idx].is_some() {
            return Err(VfsError::AlreadyMounted);
        }

        self.drives[idx] = Some(DriveMount::with_label(letter, filesystem, label));
        Ok(())
    }

    /// Unmount a drive letter
    pub fn unmount_drive(&mut self, letter: char) -> VfsResult<()> {
        let idx = drive_index(letter).ok_or(VfsError::InvalidArgument)?;

        if self.drives[idx].is_none() {
            return Err(VfsError::NotMounted);
        }

        self.drives[idx] = None;
        Ok(())
    }

    /// Get a drive mount by letter
    pub fn get_drive(&self, letter: char) -> Option<&DriveMount> {
        let idx = drive_index(letter)?;
        self.drives[idx].as_ref()
    }

    /// List all mounted drives
    pub fn list_drives(&self) -> impl Iterator<Item = &DriveMount> {
        self.drives.iter().filter_map(|d| d.as_ref())
    }

    // ========== Resolution ==========

    /// Find the filesystem for a given path (auto-detects path type)
    ///
    /// Returns the filesystem and the path relative to the mount point
    pub fn resolve(&self, path: &str) -> VfsResult<(&dyn Filesystem, String)> {
        let parsed = parse(path);

        match parsed.path_type {
            PathType::Drive(letter) => self.resolve_drive(letter, &parsed.path),
            PathType::Unix => self.resolve_path(&parsed.path),
        }
    }

    /// Resolve a drive letter path
    fn resolve_drive(&self, letter: char, rel_path: &str) -> VfsResult<(&dyn Filesystem, String)> {
        let idx = drive_index(letter).ok_or(VfsError::InvalidArgument)?;

        match &self.drives[idx] {
            Some(drive) => {
                // Path is already jailed by the parse function
                Ok((drive.filesystem.as_ref(), String::from(rel_path)))
            }
            None => Err(VfsError::NotMounted),
        }
    }

    /// Resolve a Unix-style path
    fn resolve_path(&self, path: &str) -> VfsResult<(&dyn Filesystem, String)> {
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

    // ========== Query Operations ==========

    /// Get list of all path mount points
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
