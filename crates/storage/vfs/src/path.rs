//! Path handling utilities

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// Path separator
pub const SEPARATOR: char = '/';

/// Path wrapper with utilities
#[derive(Debug, Clone)]
pub struct Path {
    inner: String,
}

impl Path {
    /// Create a new path
    pub fn new(s: &str) -> Self {
        Path {
            inner: normalize(s),
        }
    }

    /// Get the path as a string slice
    pub fn as_str(&self) -> &str {
        &self.inner
    }

    /// Check if path is absolute
    pub fn is_absolute(&self) -> bool {
        self.inner.starts_with(SEPARATOR)
    }

    /// Check if path is relative
    pub fn is_relative(&self) -> bool {
        !self.is_absolute()
    }

    /// Get parent directory
    pub fn parent(&self) -> Option<Path> {
        if self.inner == "/" || self.inner.is_empty() {
            return None;
        }

        let trimmed = self.inner.trim_end_matches(SEPARATOR);
        if let Some(pos) = trimmed.rfind(SEPARATOR) {
            if pos == 0 {
                Some(Path::new("/"))
            } else {
                Some(Path::new(&trimmed[..pos]))
            }
        } else {
            Some(Path::new("."))
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
        if other.starts_with(SEPARATOR) {
            // Other is absolute, use it directly
            Path::new(other)
        } else if self.inner.ends_with(SEPARATOR) || self.inner.is_empty() {
            Path::new(&format!("{}{}", self.inner, other))
        } else {
            Path::new(&format!("{}/{}", self.inner, other))
        }
    }

    /// Get path components
    pub fn components(&self) -> Vec<&str> {
        self.inner
            .split(SEPARATOR)
            .filter(|s| !s.is_empty())
            .collect()
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
