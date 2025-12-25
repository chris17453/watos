//! WFS Common - Shared WFS (WATOS File System) structures
//!
//! This crate provides the canonical definitions for all WFS on-disk structures.
//! Both the kernel driver and mkfs tool MUST use these definitions.
//!
//! ## Versions
//!
//! - **WFS v2**: Simple flat file system (legacy)
//! - **WFS v3**: Copy-on-Write filesystem with B+tree directories
//!
//! ## Disk Layout (v2 - Legacy)
//!
//! ```text
//! Block 0:        Superblock (primary)
//! Block 1:        Boundary marker (SUPERBLOCK -> BITMAP)
//! Block 2..B:     Allocation bitmap
//! Block B+1:      Boundary marker (BITMAP -> FILETABLE)
//! Block B+2..F:   File table
//! Block F+1:      Boundary marker (FILETABLE -> DATA)
//! Block F+2..D:   Data blocks
//! ...
//! Block mid-1:    Boundary marker (DATA -> SUPERBLOCK)
//! Block mid:      Superblock (backup 1)
//! Block mid+1:    Boundary marker (SUPERBLOCK -> DATA)
//! ...
//! Block last-1:   Boundary marker (DATA -> SUPERBLOCK)
//! Block last:     Superblock (backup 2)
//! ```
//!
//! ## Disk Layout (v3 - CoW)
//!
//! ```text
//! Block 0:        Superblock (primary)
//! Block 1:        Superblock (backup)
//! Block 2+:       Tree nodes (inodes, directories, extents, free-space)
//! ...
//! Block N+:       Data blocks (file content)
//! ```

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;

// WFS v3 - Copy-on-Write filesystem
pub mod v3;

// ============================================================================
// VERSION AND MAGIC
// ============================================================================

pub const WFS_MAGIC: u32 = 0x57465332;        // "WFS2" - version 2
pub const WFS_VERSION: u16 = 2;
pub const BOUNDARY_MAGIC: u64 = 0x574653_424E4459;  // "WFS_BNDY"

// Filesystem signature - visible in hex dumps (56 chars + 8 null padding = 64 bytes)
pub const WFS_SIGNATURE: &[u8; 64] = b"WATOS FileSystem - Chris Watkins <chris@watkinslabs.com>\0\0\0\0\0\0\0\0";

// ============================================================================
// BLOCK CONSTANTS
// ============================================================================

pub const BLOCK_SIZE: u32 = 4096;
pub const SECTORS_PER_BLOCK: u32 = BLOCK_SIZE / 512;

// ============================================================================
// FILE LIMITS
// ============================================================================

pub const MAX_FILENAME: usize = 56;
pub const DEFAULT_MAX_FILES: usize = 4096;     // Default: support up to 4096 files
pub const MIN_MAX_FILES: usize = 256;          // Minimum for tiny disks
pub const MAX_MAX_FILES: usize = 65536;        // Maximum supported

// ============================================================================
// BOUNDARY MARKER TYPES
// ============================================================================

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundaryType {
    SuperblockToBitmap = 0x01,
    BitmapToFiletable = 0x02,
    FiletableToData = 0x03,
    DataToSuperblock = 0x04,
    SuperblockToData = 0x05,
}

// ============================================================================
// FILE FLAGS
// ============================================================================

pub const FLAG_EXEC: u16 = 0x0001;     // Executable
pub const FLAG_READONLY: u16 = 0x0002; // Read only
pub const FLAG_SYSTEM: u16 = 0x0004;   // System file
pub const FLAG_HIDDEN: u16 = 0x0008;   // Hidden
pub const FLAG_DIR: u16 = 0x0010;      // Directory

// ============================================================================
// SUPERBLOCK (Block 0, mid, last)
// ============================================================================

/// Superblock - stored at block 0, mid, and last
///
/// Size: 256 bytes
///
/// Hex dump will show:
/// 00000000: 3253 4657 0200 0000  5741 544f 5320 4669  2SFW....WATOS Fi
/// 00000010: 6c65 5379 7374 656d  202d 2043 6872 6973  leSystem - Chris
/// ...
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Superblock {
    // Header (8 bytes)
    pub magic: u32,              // "WFS2"
    pub version: u16,            // Filesystem version
    pub flags: u16,              // Mount flags

    // Signature (64 bytes) - visible in hex dumps
    pub signature: [u8; 64],     // "WATOS FileSystem - Chris Watkins <chris@watkinslabs.com>"

    // Block info (8 bytes)
    pub block_size: u32,         // Block size (always 4096 for now)
    pub _pad0: u32,              // Alignment padding

    // Disk geometry (24 bytes)
    pub total_blocks: u64,       // Total blocks on disk
    pub free_blocks: u64,        // Free blocks available
    pub data_start_block: u64,   // First data block

    // Structure locations (32 bytes) - reordered for alignment
    pub bitmap_block: u64,       // First bitmap block
    pub filetable_block: u64,    // First file table block
    pub bitmap_blocks: u32,      // Number of bitmap blocks
    pub filetable_blocks: u32,   // Number of file table blocks

    // File info (8 bytes)
    pub max_files: u32,          // Maximum files supported
    pub root_files: u32,         // Current file count in root

    // Reserved for future use (112 bytes)
    pub reserved: [u8; 112],

    // Integrity (8 bytes)
    pub crc32: u32,              // CRC of superblock (offset 248)
    pub _pad1: u32,              // Pad to 256 bytes
}

/// Size of superblock data to CRC (everything before crc32 field)
pub const SUPERBLOCK_CRC_OFFSET: usize = 248;
pub const SUPERBLOCK_SIZE: usize = 256;

impl Default for Superblock {
    fn default() -> Self {
        Self {
            magic: WFS_MAGIC,
            version: WFS_VERSION,
            flags: 0,
            signature: *WFS_SIGNATURE,
            block_size: BLOCK_SIZE,
            _pad0: 0,
            total_blocks: 0,
            free_blocks: 0,
            data_start_block: 0,
            bitmap_block: 0,
            filetable_block: 0,
            bitmap_blocks: 0,
            filetable_blocks: 0,
            max_files: DEFAULT_MAX_FILES as u32,
            root_files: 0,
            reserved: [0; 112],
            crc32: 0,
            _pad1: 0,
        }
    }
}

// ============================================================================
// BOUNDARY MARKER BLOCK
// ============================================================================

/// Boundary marker - visual separator between disk regions
///
/// Filled with recognizable pattern for easy hex dump reading
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BoundaryBlock {
    pub magic: u64,                      // BOUNDARY_MAGIC
    pub boundary_type: u32,              // BoundaryType
    pub sequence: u32,                   // Boundary sequence number
    pub prev_section_end: u64,           // Last block of previous section
    pub next_section_start: u64,         // First block of next section
    pub pattern: [u8; BLOCK_SIZE as usize - 32],  // Fill pattern
}

pub const BOUNDARY_PATTERN: u8 = 0xBD;   // "BD" for BounDary
pub const BOUNDARY_PATTERN_WORD: u32 = 0xBDBDBDBD;

impl BoundaryBlock {
    pub fn new(btype: BoundaryType, seq: u32, prev_end: u64, next_start: u64) -> Self {
        let mut block = Self {
            magic: BOUNDARY_MAGIC,
            boundary_type: btype as u32,
            sequence: seq,
            prev_section_end: prev_end,
            next_section_start: next_start,
            pattern: [BOUNDARY_PATTERN; BLOCK_SIZE as usize - 32],
        };
        // Add visual markers every 64 bytes
        for i in (0..block.pattern.len()).step_by(64) {
            if i + 8 <= block.pattern.len() {
                block.pattern[i..i+4].copy_from_slice(&BOUNDARY_PATTERN_WORD.to_le_bytes());
                block.pattern[i+4..i+8].copy_from_slice(&(btype as u32).to_le_bytes());
            }
        }
        block
    }

    pub fn is_valid(&self) -> bool {
        self.magic == BOUNDARY_MAGIC
    }

    pub fn boundary_type(&self) -> Option<BoundaryType> {
        match self.boundary_type {
            0x01 => Some(BoundaryType::SuperblockToBitmap),
            0x02 => Some(BoundaryType::BitmapToFiletable),
            0x03 => Some(BoundaryType::FiletableToData),
            0x04 => Some(BoundaryType::DataToSuperblock),
            0x05 => Some(BoundaryType::SuperblockToData),
            _ => None,
        }
    }
}

// ============================================================================
// FILE ENTRY
// ============================================================================

/// File entry - 128 bytes each for alignment and future expansion
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FileEntry {
    pub name: [u8; MAX_FILENAME],    // 56 bytes - Null-terminated filename
    pub size: u64,                   // 8 bytes  - File size in bytes
    pub start_block: u64,            // 8 bytes  - First data block
    pub blocks: u32,                 // 4 bytes  - Number of blocks allocated
    pub flags: u16,                  // 2 bytes  - File flags
    pub _pad0: u16,                  // 2 bytes  - Alignment
    pub created: u64,                // 8 bytes  - Creation timestamp (reserved)
    pub modified: u64,               // 8 bytes  - Modification timestamp (reserved)
    pub crc32: u32,                  // 4 bytes  - CRC of entry
    pub data_crc32: u32,             // 4 bytes  - CRC of file data
    pub reserved: [u8; 24],          // 24 bytes - Future use
}

pub const FILEENTRY_SIZE: usize = 128;
pub const ENTRIES_PER_BLOCK: usize = BLOCK_SIZE as usize / FILEENTRY_SIZE;  // 32 entries/block

impl Default for FileEntry {
    fn default() -> Self {
        Self {
            name: [0; MAX_FILENAME],
            size: 0,
            start_block: 0,
            blocks: 0,
            flags: 0,
            _pad0: 0,
            created: 0,
            modified: 0,
            crc32: 0,
            data_crc32: 0,
            reserved: [0; 24],
        }
    }
}

impl FileEntry {
    /// Get filename as string slice
    pub fn name_str(&self) -> &str {
        let len = self.name.iter().position(|&b| b == 0).unwrap_or(MAX_FILENAME);
        core::str::from_utf8(&self.name[..len]).unwrap_or("")
    }

    /// Set filename from string
    pub fn set_name(&mut self, name: &str) {
        self.name = [0; MAX_FILENAME];
        let bytes = name.as_bytes();
        let len = bytes.len().min(MAX_FILENAME - 1);
        self.name[..len].copy_from_slice(&bytes[..len]);
    }

    /// Check if entry is valid (has a name)
    pub fn is_valid(&self) -> bool {
        self.name[0] != 0
    }

    pub fn is_executable(&self) -> bool {
        self.flags & FLAG_EXEC != 0
    }

    pub fn is_directory(&self) -> bool {
        self.flags & FLAG_DIR != 0
    }

    pub fn is_system(&self) -> bool {
        self.flags & FLAG_SYSTEM != 0
    }

    pub fn is_hidden(&self) -> bool {
        self.flags & FLAG_HIDDEN != 0
    }

    pub fn is_readonly(&self) -> bool {
        self.flags & FLAG_READONLY != 0
    }
}

// ============================================================================
// CRC32 - IEEE polynomial
// ============================================================================

const CRC32_TABLE: [u32; 256] = generate_crc32_table();

const fn generate_crc32_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;
        while j < 8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
}

/// Calculate CRC32 of data
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFFFFFF_u32;
    for &byte in data {
        let index = ((crc ^ byte as u32) & 0xFF) as usize;
        crc = (crc >> 8) ^ CRC32_TABLE[index];
    }
    !crc
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Calculate CRC for a superblock (everything before crc32 field)
pub fn superblock_crc(sb: &Superblock) -> u32 {
    let bytes = unsafe {
        core::slice::from_raw_parts(sb as *const _ as *const u8, SUPERBLOCK_CRC_OFFSET)
    };
    crc32(bytes)
}

/// Calculate CRC for a file entry (everything before crc32 field)
/// CRC covers bytes 0-83 (before crc32 at offset 84)
pub const FILEENTRY_CRC_OFFSET: usize = 96;  // Everything before crc32 field

pub fn file_entry_crc(entry: &FileEntry) -> u32 {
    let bytes = unsafe {
        core::slice::from_raw_parts(entry as *const _ as *const u8, FILEENTRY_CRC_OFFSET)
    };
    crc32(bytes)
}

/// Data bytes per block (leaving room for CRC footer)
pub const DATA_PER_BLOCK: usize = BLOCK_SIZE as usize - 4;

/// Calculate number of blocks needed for data
pub const fn blocks_needed(data_size: usize) -> u32 {
    if data_size == 0 {
        1 // At least 1 block for empty files
    } else {
        ((data_size + DATA_PER_BLOCK - 1) / DATA_PER_BLOCK) as u32
    }
}

/// Calculate disk layout for given parameters
pub struct DiskLayout {
    pub total_blocks: u64,
    pub bitmap_block: u64,
    pub bitmap_blocks: u32,
    pub filetable_block: u64,
    pub filetable_blocks: u32,
    pub data_start_block: u64,
    pub max_files: u32,
    pub mid_superblock: u64,
    pub last_superblock: u64,
}

impl DiskLayout {
    /// Calculate layout for a disk of given size with given max files
    pub fn calculate(total_blocks: u64, max_files: u32) -> Self {
        // Bitmap: 1 bit per block, round up to blocks
        let bitmap_bits = total_blocks as usize;
        let bitmap_bytes = (bitmap_bits + 7) / 8;
        let bitmap_blocks = ((bitmap_bytes + BLOCK_SIZE as usize - 1) / BLOCK_SIZE as usize) as u32;

        // File table: 32 entries per block
        let filetable_blocks = ((max_files as usize + ENTRIES_PER_BLOCK - 1) / ENTRIES_PER_BLOCK) as u32;

        // Layout with boundary markers:
        // 0: superblock
        // 1: boundary (SB->BITMAP)
        // 2..2+bitmap_blocks-1: bitmap
        // 2+bitmap_blocks: boundary (BITMAP->FT)
        // 2+bitmap_blocks+1...: file table
        // ...: boundary (FT->DATA)
        // ...: data blocks

        let bitmap_block = 2;  // After superblock and first boundary
        let filetable_block = bitmap_block + bitmap_blocks as u64 + 1;  // +1 for boundary
        let data_start_block = filetable_block + filetable_blocks as u64 + 1;  // +1 for boundary

        let mid_superblock = total_blocks / 2;
        let last_superblock = total_blocks - 1;

        Self {
            total_blocks,
            bitmap_block,
            bitmap_blocks,
            filetable_block,
            filetable_blocks,
            data_start_block,
            max_files,
            mid_superblock,
            last_superblock,
        }
    }

    /// Get file entry location
    pub fn file_entry_offset(&self, index: usize) -> (u64, usize) {
        let block_offset = index / ENTRIES_PER_BLOCK;
        let entry_offset = index % ENTRIES_PER_BLOCK;
        (self.filetable_block + block_offset as u64, entry_offset * FILEENTRY_SIZE)
    }

    /// Calculate free blocks (excluding metadata)
    pub fn free_blocks(&self) -> u64 {
        // Total - superblocks(3) - boundaries(~6) - bitmap - filetable
        let used = 3  // superblocks
            + 2  // boundaries around primary superblock
            + self.bitmap_blocks as u64
            + 1  // boundary after bitmap
            + self.filetable_blocks as u64
            + 1  // boundary after filetable
            + 2; // boundaries around mid superblock
        self.total_blocks.saturating_sub(used)
    }
}

// ============================================================================
// COMPILE-TIME CHECKS
// ============================================================================

const _: () = assert!(core::mem::size_of::<FileEntry>() == FILEENTRY_SIZE);
const _: () = assert!(core::mem::size_of::<Superblock>() == SUPERBLOCK_SIZE);
const _: () = assert!(SUPERBLOCK_SIZE == 256);
const _: () = assert!(ENTRIES_PER_BLOCK == 32);
const _: () = assert!(core::mem::size_of::<BoundaryBlock>() == BLOCK_SIZE as usize);
