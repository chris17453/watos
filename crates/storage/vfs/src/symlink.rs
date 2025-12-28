//! Symbolic link support for VFS
//!
//! Symlinks are special files that point to another path. When accessed,
//! the VFS transparently follows the link to the target.
//!
//! # Features
//!
//! - Relative symlinks: `../other/file`
//! - Absolute symlinks: `/path/to/target`
//! - Drive letter symlinks: `C:/path/to/target` (follows drive jail rules)
//! - Loop detection: Prevents infinite symlink chains
//!
//! # Filesystem Support
//!
//! Filesystems that support symlinks implement the extended `SymlinkFilesystem` trait.

use alloc::string::String;

use crate::{VfsError, VfsResult};
use crate::path::{Path, PathType, parse};

/// Maximum symlink depth to prevent infinite loops
pub const MAX_SYMLINK_DEPTH: usize = 40;

/// Symlink target - where a symlink points to
#[derive(Debug, Clone)]
pub struct SymlinkTarget {
    /// The raw target path as stored
    pub raw: String,
    /// Parsed path information
    pub parsed: crate::path::ParsedPath,
}

impl SymlinkTarget {
    /// Create a new symlink target
    pub fn new(target: &str) -> Self {
        let parsed = parse(target);
        SymlinkTarget {
            raw: String::from(target),
            parsed,
        }
    }

    /// Check if this is a relative symlink
    pub fn is_relative(&self) -> bool {
        !self.parsed.path.starts_with('/')
    }

    /// Check if this is an absolute symlink
    pub fn is_absolute(&self) -> bool {
        self.parsed.path.starts_with('/')
    }

    /// Check if this symlink points to a drive letter path
    pub fn is_drive(&self) -> bool {
        matches!(self.parsed.path_type, PathType::Drive(_))
    }

    /// Resolve this symlink target relative to a base path
    ///
    /// For relative symlinks, the target is resolved relative to the
    /// directory containing the symlink.
    ///
    /// For absolute symlinks, the target is used as-is.
    pub fn resolve(&self, symlink_dir: &str) -> String {
        if self.is_absolute() || self.is_drive() {
            // Absolute or drive path - use as-is
            self.raw.clone()
        } else {
            // Relative path - resolve from symlink's directory
            let base = Path::new(symlink_dir);
            let joined = base.join(&self.raw);
            joined.to_display(false)
        }
    }
}

/// Extended filesystem trait for symlink operations
///
/// Filesystems that support symlinks should implement this trait
/// in addition to the base `Filesystem` trait.
pub trait SymlinkFilesystem {
    /// Create a symbolic link
    ///
    /// Creates a symlink at `link_path` pointing to `target`.
    fn symlink(&self, target: &str, link_path: &str) -> VfsResult<()>;

    /// Read a symbolic link
    ///
    /// Returns the target path of the symlink without following it.
    fn readlink(&self, path: &str) -> VfsResult<String>;

    /// Check if a path is a symbolic link
    fn is_symlink(&self, path: &str) -> bool;
}

/// Symlink resolution context
///
/// Tracks the current depth to prevent infinite loops.
pub struct SymlinkResolver {
    depth: usize,
}

impl SymlinkResolver {
    /// Create a new resolver
    pub fn new() -> Self {
        SymlinkResolver { depth: 0 }
    }

    /// Begin following a symlink
    ///
    /// Returns an error if we've exceeded the maximum depth.
    pub fn enter(&mut self) -> VfsResult<()> {
        self.depth += 1;
        if self.depth > MAX_SYMLINK_DEPTH {
            Err(VfsError::InvalidPath) // Too many levels of symlinks (ELOOP)
        } else {
            Ok(())
        }
    }

    /// Finish following a symlink
    pub fn leave(&mut self) {
        if self.depth > 0 {
            self.depth -= 1;
        }
    }

    /// Get current depth
    pub fn depth(&self) -> usize {
        self.depth
    }
}

impl Default for SymlinkResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of resolving a path that may contain symlinks
#[derive(Debug, Clone)]
pub struct ResolvedPath {
    /// The final resolved path
    pub path: String,
    /// Number of symlinks followed
    pub symlinks_followed: usize,
    /// Whether any symlinks were encountered
    pub has_symlinks: bool,
}

impl ResolvedPath {
    /// Create a resolved path with no symlinks
    pub fn no_symlinks(path: String) -> Self {
        ResolvedPath {
            path,
            symlinks_followed: 0,
            has_symlinks: false,
        }
    }

    /// Create a resolved path that followed symlinks
    pub fn with_symlinks(path: String, count: usize) -> Self {
        ResolvedPath {
            path,
            symlinks_followed: count,
            has_symlinks: count > 0,
        }
    }
}

/// Options for path resolution
#[derive(Debug, Clone, Copy)]
pub struct ResolveOptions {
    /// Follow symlinks (default: true)
    pub follow_symlinks: bool,
    /// Follow the final component if it's a symlink (default: true)
    /// If false, operations like lstat() won't follow the final symlink
    pub follow_final: bool,
}

impl Default for ResolveOptions {
    fn default() -> Self {
        ResolveOptions {
            follow_symlinks: true,
            follow_final: true,
        }
    }
}

impl ResolveOptions {
    /// Don't follow symlinks at all (for operations on symlinks themselves)
    pub fn no_follow() -> Self {
        ResolveOptions {
            follow_symlinks: false,
            follow_final: false,
        }
    }

    /// Follow all symlinks except the final component (for lstat-like operations)
    pub fn no_follow_final() -> Self {
        ResolveOptions {
            follow_symlinks: true,
            follow_final: false,
        }
    }
}
