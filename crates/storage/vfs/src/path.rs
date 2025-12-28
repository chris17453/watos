//! Path handling utilities
//!
//! Supports both Unix-style paths (`/mount/disk/file`) and Windows-style
//! drive letter paths (`C:\folder\file` or `C:/folder/file`).
//!
//! Drive letter paths are "jailed" - you cannot escape the drive root with `..`.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// Path separator (Unix-style, canonical)
pub const SEPARATOR: char = '/';

/// Windows path separator
pub const WIN_SEPARATOR: char = '\\';

/// Path type - Unix or Drive letter
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathType {
    /// Unix-style path (starts with / or is relative)
    Unix,
    /// Drive letter path (e.g., C:, D:)
    /// The char is the uppercase drive letter
    Drive(char),
}

/// Parsed path with type information
#[derive(Debug, Clone)]
pub struct ParsedPath {
    /// The path type
    pub path_type: PathType,
    /// The normalized path (always uses forward slashes)
    /// For drive paths, this is relative to the drive root (e.g., "/folder/file")
    /// For unix paths, this is the full path
    pub path: String,
}

impl ParsedPath {
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

    /// Convert back to a display string
    pub fn to_display(&self, windows_style: bool) -> String {
        match self.path_type {
            PathType::Unix => self.path.clone(),
            PathType::Drive(letter) => {
                if windows_style {
                    // C:\folder\file
                    format!("{}:{}", letter, self.path.replace('/', "\\"))
                } else {
                    // C:/folder/file
                    format!("{}:{}", letter, self.path)
                }
            }
        }
    }
}

/// Path wrapper with utilities
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
        // Drive paths are always "absolute" within their drive
        self.is_drive() || self.inner.starts_with(SEPARATOR)
    }

    /// Check if path is relative
    pub fn is_relative(&self) -> bool {
        !self.is_absolute()
    }

    /// Get parent directory
    /// For drive paths, cannot go above drive root
    pub fn parent(&self) -> Option<Path> {
        if self.inner == "/" || self.inner.is_empty() {
            return None;
        }

        let trimmed = self.inner.trim_end_matches(SEPARATOR);
        if let Some(pos) = trimmed.rfind(SEPARATOR) {
            if pos == 0 {
                // At root - for drive paths, stay at root
                if self.is_drive() {
                    None // Can't go above drive root
                } else {
                    Some(Path::with_type("/", self.path_type))
                }
            } else {
                Some(Path::with_type(&trimmed[..pos], self.path_type))
            }
        } else {
            if self.is_drive() {
                None // Can't go above drive root
            } else {
                Some(Path::with_type(".", PathType::Unix))
            }
        }
    }

    /// Get filename component
    pub fn filename(&self) -> Option<&str> {
        if self.inner == "/" || self.inner.is_empty() {
            return None;
        }

        let trimmed = self.inner.trim_end_matches(SEPARATOR);
        if let Some(pos) = trimmed.rfind(SEPARATOR) {
            Some(&trimmed[pos + 1..])
        } else {
            Some(trimmed)
        }
    }

    /// Get file extension
    pub fn extension(&self) -> Option<&str> {
        self.filename().and_then(|name| {
            let pos = name.rfind('.')?;
            if pos == 0 {
                None // Hidden file, not an extension
            } else {
                Some(&name[pos + 1..])
            }
        })
    }

    /// Join with another path component
    pub fn join(&self, other: &str) -> Path {
        // Check if other is an absolute path or drive path
        let other_parsed = parse(other);
        if other_parsed.path_type != PathType::Unix || other_parsed.path.starts_with('/') {
            // Other is absolute or a different drive, use it directly
            return Path {
                inner: other_parsed.path,
                path_type: other_parsed.path_type,
            };
        }

        // Join relative path
        let joined = if self.inner.ends_with(SEPARATOR) || self.inner.is_empty() {
            format!("{}{}", self.inner, other_parsed.path)
        } else {
            format!("{}/{}", self.inner, other_parsed.path)
        };

        // Re-normalize (with jailing for drive paths)
        Path::with_type(&joined, self.path_type)
    }

    /// Get path components
    pub fn components(&self) -> Vec<&str> {
        self.inner
            .split(SEPARATOR)
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// Convert to display string
    pub fn to_display(&self, windows_style: bool) -> String {
        match self.path_type {
            PathType::Unix => self.inner.clone(),
            PathType::Drive(letter) => {
                if windows_style {
                    format!("{}:{}", letter, self.inner.replace('/', "\\"))
                } else {
                    format!("{}:{}", letter, self.inner)
                }
            }
        }
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

/// Check if a string starts with a drive letter (e.g., "C:" or "C:\")
pub fn is_drive_letter(s: &str) -> Option<char> {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let second = bytes[1];
        // Check for A-Z or a-z followed by :
        if second == b':' && ((first >= b'A' && first <= b'Z') || (first >= b'a' && first <= b'z')) {
            return Some((first as char).to_ascii_uppercase());
        }
    }
    None
}

/// Parse a path string into type and normalized path
///
/// Handles:
/// - Unix paths: `/foo/bar`, `./relative`, `../parent`
/// - Drive paths: `C:\folder\file`, `C:/folder/file`, `D:`
pub fn parse(path: &str) -> ParsedPath {
    // Check for drive letter
    if let Some(drive) = is_drive_letter(path) {
        // Extract the part after "X:"
        let rest = &path[2..];
        // Convert backslashes to forward slashes and normalize
        let converted: String = rest.chars().map(|c| if c == WIN_SEPARATOR { SEPARATOR } else { c }).collect();
        let normalized = normalize_jailed(&converted);
        return ParsedPath {
            path_type: PathType::Drive(drive),
            path: normalized,
        };
    }

    // Unix path - also convert any backslashes for convenience
    let converted: String = path.chars().map(|c| if c == WIN_SEPARATOR { SEPARATOR } else { c }).collect();
    let normalized = normalize(&converted);

    ParsedPath {
        path_type: PathType::Unix,
        path: normalized,
    }
}

/// Normalize a path string
///
/// - Removes duplicate slashes
/// - Resolves . and ..
/// - Ensures absolute paths start with /
pub fn normalize(path: &str) -> String {
    if path.is_empty() {
        return String::from(".");
    }

    let is_absolute = path.starts_with(SEPARATOR);
    let mut components: Vec<&str> = Vec::new();

    for part in path.split(SEPARATOR) {
        match part {
            "" | "." => continue,
            ".." => {
                if !is_absolute && components.is_empty() {
                    components.push("..");
                } else if components.last() == Some(&"..") {
                    components.push("..");
                } else if !components.is_empty() {
                    components.pop();
                }
            }
            _ => components.push(part),
        }
    }

    if is_absolute {
        if components.is_empty() {
            String::from("/")
        } else {
            format!("/{}", components.join("/"))
        }
    } else if components.is_empty() {
        String::from(".")
    } else {
        components.join("/")
    }
}

/// Normalize a path with jail semantics (cannot escape root)
///
/// Used for drive letter paths where `..` at root stays at root
pub fn normalize_jailed(path: &str) -> String {
    if path.is_empty() {
        return String::from("/");
    }

    // For jailed paths, treat everything as absolute (rooted at drive)
    let mut components: Vec<&str> = Vec::new();

    for part in path.split(SEPARATOR) {
        match part {
            "" | "." => continue,
            ".." => {
                // Can only pop if we have components - never go negative
                if !components.is_empty() {
                    components.pop();
                }
                // If empty, just stay at root (jailed behavior)
            }
            _ => components.push(part),
        }
    }

    if components.is_empty() {
        String::from("/")
    } else {
        format!("/{}", components.join("/"))
    }
}

/// Split path into directory and filename
pub fn split(path: &str) -> (&str, &str) {
    let normalized = path.trim_end_matches(SEPARATOR);

    if let Some(pos) = normalized.rfind(SEPARATOR) {
        if pos == 0 {
            ("/", &normalized[1..])
        } else {
            (&normalized[..pos], &normalized[pos + 1..])
        }
    } else {
        (".", normalized)
    }
}

/// Check if a path component is valid
pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(SEPARATOR)
        && !name.contains('\0')
        && name.len() <= super::MAX_FILENAME
}
