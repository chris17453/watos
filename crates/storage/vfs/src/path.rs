//! Path handling utilities
//!
//! This module re-exports the universal path handling from watos-path
//! and provides VFS-specific compatibility wrappers.

// Re-export everything from the universal path module
pub use watos_path::{
    SEPARATOR,
    WIN_SEPARATOR,
    MAX_PATH,
    MAX_FILENAME,
    PathType,
    ParsedPath,
    is_drive_letter,
    parse,
    normalize,
    normalize_jailed,
    split,
    parent,
    filename,
    extension,
    join,
    is_valid_name,
    paths_equal,
    filenames_equal,
    components,
    is_absolute,
    is_relative,
};

// Path wrapper type for backward compatibility
// This delegates to watos_path internally
use alloc::string::String;

/// Path wrapper with utilities (delegates to watos_path)
#[derive(Debug, Clone)]
pub struct Path {
    inner: String,
    path_type: PathType,
}

impl Path {
    /// Create a new path
    pub fn new(s: &str) -> Self {
        let parsed = parse(s);
        Path {
            inner: parsed.path,
            path_type: parsed.path_type,
        }
    }

    /// Create a new path with explicit type
    pub fn with_type(s: &str, path_type: PathType) -> Self {
        let normalized = match path_type {
            PathType::Unix => normalize(s),
            PathType::Drive(_) => normalize_jailed(s),
        };
        Path {
            inner: normalized,
            path_type,
        }
    }

    /// Get the path as a string slice
    pub fn as_str(&self) -> &str {
        &self.inner
    }

    /// Get the path type
    pub fn path_type(&self) -> PathType {
        self.path_type
    }

    /// Check if this is a drive letter path
    pub fn is_drive(&self) -> bool {
        matches!(self.path_type, PathType::Drive(_))
    }

    /// Get drive letter if this is a drive path
    pub fn drive_letter(&self) -> Option<char> {
        match self.path_type {
            PathType::Drive(c) => Some(c),
            PathType::Unix => None,
        }
    }

    /// Check if path is absolute
    pub fn is_absolute(&self) -> bool {
        self.is_drive() || self.inner.starts_with(SEPARATOR)
    }

    /// Check if path is relative
    pub fn is_relative(&self) -> bool {
        !self.is_absolute()
    }

    /// Get parent directory
    pub fn parent(&self) -> Option<Path> {
        parent(&self.inner).map(|p| Path::with_type(&p, self.path_type))
    }

    /// Get filename component
    pub fn filename(&self) -> Option<&str> {
        filename(&self.inner)
    }

    /// Get file extension
    pub fn extension(&self) -> Option<&str> {
        extension(&self.inner)
    }

    /// Join with another path component
    pub fn join(&self, other: &str) -> Path {
        let joined = join(&self.inner, other);
        Path::new(&joined)
    }

    /// Get path components
    pub fn components(&self) -> alloc::vec::Vec<&str> {
        components(&self.inner)
    }

    /// Convert to display string
    pub fn to_display(&self, windows_style: bool) -> String {
        let parsed = ParsedPath {
            path_type: self.path_type,
            path: self.inner.clone(),
        };
        parsed.to_display(windows_style)
    }
}

impl From<&str> for Path {
    fn from(s: &str) -> Self {
        Path::new(s)
    }
}

impl From<String> for Path {
    fn from(s: String) -> Self {
        Path::new(&s)
    }
}
