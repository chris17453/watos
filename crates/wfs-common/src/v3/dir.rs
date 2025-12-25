//! Directory Entry Structure
//!
//! Directory entries are stored in per-directory B+trees.
//! Key: filename hash + filename
//! Value: target inode number + entry type

use super::structures::MAX_FILENAME;

// ============================================================================
// DIRECTORY ENTRY TYPE
// ============================================================================

/// Type of directory entry
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryType {
    /// Unknown type
    Unknown = 0,
    /// Regular file
    File = 1,
    /// Directory
    Directory = 2,
    /// Symbolic link
    Symlink = 3,
    /// Block device
    BlockDevice = 4,
    /// Character device
    CharDevice = 5,
    /// Named pipe (FIFO)
    Fifo = 6,
    /// Socket
    Socket = 7,
}

impl EntryType {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => EntryType::File,
            2 => EntryType::Directory,
            3 => EntryType::Symlink,
            4 => EntryType::BlockDevice,
            5 => EntryType::CharDevice,
            6 => EntryType::Fifo,
            7 => EntryType::Socket,
            _ => EntryType::Unknown,
        }
    }
}

// ============================================================================
// DIRECTORY ENTRY (FIXED SIZE FOR SIMPLICITY)
// ============================================================================

/// Directory entry - fixed 272 bytes for simple implementation
///
/// In a production filesystem, this would be variable-length.
/// For simplicity, we use fixed-size entries.
///
/// Directory entries are stored in a B+tree per directory, where:
/// - Key: name hash (u64) for fast lookup
/// - Value: DirEntry structure
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DirEntry {
    // Target (8 bytes)
    pub inode_num: u64,                // Target inode number

    // Metadata (4 bytes)
    pub entry_type: u8,                // EntryType
    pub name_len: u8,                  // Length of name (1-255)
    pub _pad: u16,

    // Name hash for fast lookup (8 bytes)
    pub name_hash: u64,                // Hash of name for tree key

    // Filename (256 bytes) - null-terminated, case-sensitive
    pub name: [u8; 256],
}

/// Directory entry size (includes alignment padding)
pub const DIRENTRY_SIZE: usize = 280;

impl DirEntry {
    /// Create a new directory entry
    pub fn new(name: &str, inode_num: u64, entry_type: EntryType) -> Option<Self> {
        let name_bytes = name.as_bytes();
        if name_bytes.is_empty() || name_bytes.len() > MAX_FILENAME {
            return None;
        }

        let mut entry = Self {
            inode_num,
            entry_type: entry_type as u8,
            name_len: name_bytes.len() as u8,
            _pad: 0,
            name_hash: Self::hash_name(name),
            name: [0; 256],
        };
        entry.name[..name_bytes.len()].copy_from_slice(name_bytes);
        Some(entry)
    }

    /// Hash a filename for tree lookup
    ///
    /// Uses FNV-1a hash for good distribution.
    /// Case-sensitive (doesn't normalize case).
    pub fn hash_name(name: &str) -> u64 {
        const FNV_OFFSET: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;

        let mut hash = FNV_OFFSET;
        for byte in name.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        hash
    }

    /// Get the filename as a string slice
    pub fn name_str(&self) -> &str {
        let len = self.name_len as usize;
        // Safety: we control name storage and ensure valid UTF-8 on creation
        core::str::from_utf8(&self.name[..len]).unwrap_or("")
    }

    /// Check if this entry matches a name
    pub fn matches(&self, name: &str) -> bool {
        // First check hash for fast rejection
        if self.name_hash != Self::hash_name(name) {
            return false;
        }
        // Then check actual name (handles hash collisions)
        self.name_str() == name
    }

    /// Get the entry type
    pub fn entry_type(&self) -> EntryType {
        EntryType::from_u8(self.entry_type)
    }

    /// Check if this is a directory entry
    pub fn is_directory(&self) -> bool {
        self.entry_type() == EntryType::Directory
    }

    /// Check if this is a file entry
    pub fn is_file(&self) -> bool {
        self.entry_type() == EntryType::File
    }

    /// Check if entry is valid
    pub fn is_valid(&self) -> bool {
        self.inode_num != 0 && self.name_len > 0 && self.name_len <= MAX_FILENAME as u8
    }
}

impl Default for DirEntry {
    fn default() -> Self {
        Self {
            inode_num: 0,
            entry_type: EntryType::Unknown as u8,
            name_len: 0,
            _pad: 0,
            name_hash: 0,
            name: [0; 256],
        }
    }
}

// ============================================================================
// SPECIAL DIRECTORY ENTRIES
// ============================================================================

/// "." directory entry (self-reference)
pub fn dot_entry(inode_num: u64) -> DirEntry {
    DirEntry::new(".", inode_num, EntryType::Directory).unwrap()
}

/// ".." directory entry (parent reference)
pub fn dotdot_entry(parent_inode: u64) -> DirEntry {
    DirEntry::new("..", parent_inode, EntryType::Directory).unwrap()
}

// ============================================================================
// COMPILE-TIME CHECKS
// ============================================================================

const _: () = assert!(core::mem::size_of::<DirEntry>() == DIRENTRY_SIZE);
const _: () = assert!(DIRENTRY_SIZE == 280);
