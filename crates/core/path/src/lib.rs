//! WATOS Universal Path Module
//!
//! Provides path normalization, parsing, and comparison utilities
//! for use across the entire operating system.
//!
//! Supports:
//! - Unix-style paths (`/foo/bar`)
//! - Windows-style paths (`C:\folder\file`)
//! - Case-insensitive path comparison (configurable)
//! - Path normalization (., .., duplicate slashes)
//! - Drive letter handling

#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use alloc::format;

/// Path separator (Unix-style, canonical)
pub const SEPARATOR: char = '/';

/// Windows path separator
pub const WIN_SEPARATOR: char = '\\';

/// Maximum path length
pub const MAX_PATH: usize = 256;

/// Maximum filename length
pub const MAX_FILENAME: usize = 255;

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
/// - Automatically converts backslashes to forward slashes
pub fn parse(path: &str) -> ParsedPath {
    // Check for drive letter
    if let Some(drive) = is_drive_letter(path) {
        // Extract the part after "X:"
        let rest = &path[2..];
        // Convert backslashes to forward slashes and normalize
        let converted: String = rest.chars()
            .map(|c| if c == WIN_SEPARATOR { SEPARATOR } else { c })
            .collect();
        let normalized = normalize_jailed(&converted);
        return ParsedPath {
            path_type: PathType::Drive(drive),
            path: normalized,
        };
    }

    // Unix path - also convert any backslashes for convenience
    let converted: String = path.chars()
        .map(|c| if c == WIN_SEPARATOR { SEPARATOR } else { c })
        .collect();
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

/// Get the parent directory of a path
pub fn parent(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches(SEPARATOR);

    if trimmed.is_empty() || trimmed == "/" {
        return None;
    }

    if let Some(pos) = trimmed.rfind(SEPARATOR) {
        if pos == 0 {
            Some(String::from("/"))
        } else {
            Some(String::from(&trimmed[..pos]))
        }
    } else {
        Some(String::from("."))
    }
}

/// Get the filename component of a path
pub fn filename(path: &str) -> Option<&str> {
    let trimmed = path.trim_end_matches(SEPARATOR);

    if trimmed.is_empty() || trimmed == "/" {
        return None;
    }

    if let Some(pos) = trimmed.rfind(SEPARATOR) {
        Some(&trimmed[pos + 1..])
    } else {
        Some(trimmed)
    }
}

/// Get the file extension
pub fn extension(path: &str) -> Option<&str> {
    filename(path).and_then(|name| {
        let pos = name.rfind('.')?;
        if pos == 0 {
            None // Hidden file, not an extension
        } else {
            Some(&name[pos + 1..])
        }
    })
}

/// Join two paths together
pub fn join(base: &str, component: &str) -> String {
    if component.starts_with(SEPARATOR) || is_drive_letter(component).is_some() {
        // Component is absolute, use it directly
        return String::from(component);
    }

    if base.is_empty() || base == "." {
        return String::from(component);
    }

    if base.ends_with(SEPARATOR) {
        format!("{}{}", base, component)
    } else {
        format!("{}/{}", base, component)
    }
}

/// Check if a path component is valid
pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(SEPARATOR)
        && !name.contains(WIN_SEPARATOR)
        && !name.contains('\0')
        && name.len() <= MAX_FILENAME
}

/// Compare two paths for equality (case-insensitive on Windows, case-sensitive on Unix)
pub fn paths_equal(a: &str, b: &str, case_insensitive: bool) -> bool {
    let parsed_a = parse(a);
    let parsed_b = parse(b);

    // Different path types are never equal
    if parsed_a.path_type != parsed_b.path_type {
        return false;
    }

    // Compare paths
    if case_insensitive {
        parsed_a.path.to_lowercase() == parsed_b.path.to_lowercase()
    } else {
        parsed_a.path == parsed_b.path
    }
}

/// Compare two filenames for equality (case-insensitive option)
pub fn filenames_equal(a: &str, b: &str, case_insensitive: bool) -> bool {
    if case_insensitive {
        a.to_lowercase() == b.to_lowercase()
    } else {
        a == b
    }
}

/// Get path components as a vector
pub fn components(path: &str) -> Vec<&str> {
    path.split(SEPARATOR)
        .filter(|s| !s.is_empty())
        .collect()
}

/// Check if path is absolute
pub fn is_absolute(path: &str) -> bool {
    path.starts_with(SEPARATOR) || is_drive_letter(path).is_some()
}

/// Check if path is relative
pub fn is_relative(path: &str) -> bool {
    !is_absolute(path)
}
