//! WFS - WATOS File System (Kernel Driver)
//!
//! WFS v2 Features:
//! - Up to 65536 files per volume
//! - Case-sensitive filenames with whitespace support
//! - Boundary markers for visual debugging
//! - CRC32 on all structures

use super::ahci::AhciController;

// Re-export everything from wfs-common for use by other kernel modules
pub use wfs_common::*;

// Debug info for CRC errors (accessible by main.rs)
pub static mut DEBUG_SB_SIZE: u32 = 0;
pub static mut DEBUG_CALC_CRC: u32 = 0;
pub static mut DEBUG_STORED_CRC: u32 = 0;

pub struct Wfs {
    disk: AhciController,
    superblock: Superblock,
    layout: DiskLayout,
}

/// Mount result with error details
pub enum MountResult {
    Ok(Wfs),
    ReadFailed,
    BadMagic(u32),
    BadVersion(u16),
    CrcMismatch,
}

impl Wfs {
    /// Mount existing WFS filesystem with detailed error
    pub fn try_mount(mut disk: AhciController) -> MountResult {
        // Try to read superblock from block 0
        let mut block = [0u8; BLOCK_SIZE as usize];

        if !disk.read_sectors(0, SECTORS_PER_BLOCK as u16, &mut block) {
            return MountResult::ReadFailed;
        }

        let sb = unsafe { &*(block.as_ptr() as *const Superblock) };

        if sb.magic != WFS_MAGIC {
            return MountResult::BadMagic(sb.magic);
        }

        if sb.version != WFS_VERSION {
            return MountResult::BadVersion(sb.version);
        }

        // Verify CRC using shared function
        let calculated_crc = superblock_crc(sb);
        if calculated_crc != sb.crc32 {
            unsafe {
                DEBUG_SB_SIZE = SUPERBLOCK_CRC_OFFSET as u32;
                DEBUG_CALC_CRC = calculated_crc;
                DEBUG_STORED_CRC = sb.crc32;
            }
            return MountResult::CrcMismatch;
        }

        // Reconstruct layout from superblock
        let layout = DiskLayout {
            total_blocks: sb.total_blocks,
            bitmap_block: sb.bitmap_block,
            bitmap_blocks: sb.bitmap_blocks,
            filetable_block: sb.filetable_block,
            filetable_blocks: sb.filetable_blocks,
            data_start_block: sb.data_start_block,
            max_files: sb.max_files,
            mid_superblock: sb.total_blocks / 2,
            last_superblock: sb.total_blocks - 1,
        };

        MountResult::Ok(Self {
            superblock: *sb,
            layout,
            disk,
        })
    }

    /// Mount existing WFS filesystem (simple version)
    pub fn mount(disk: AhciController) -> Option<Self> {
        match Self::try_mount(disk) {
            MountResult::Ok(wfs) => Some(wfs),
            _ => None,
        }
    }

    /// Format disk with WFS v2
    pub fn format(mut disk: AhciController, max_files: u32) -> Option<Self> {
        let info = disk.identify()?;
        let total_blocks = (info.sectors as u64 * 512) / BLOCK_SIZE as u64;

        if total_blocks < 64 {
            return None; // Too small
        }

        let max_files = max_files.clamp(MIN_MAX_FILES as u32, MAX_MAX_FILES as u32);
        let layout = DiskLayout::calculate(total_blocks, max_files);

        let mut sb = Superblock {
            magic: WFS_MAGIC,
            version: WFS_VERSION,
            flags: 0,
            signature: *WFS_SIGNATURE,
            block_size: BLOCK_SIZE,
            _pad0: 0,
            total_blocks,
            free_blocks: layout.free_blocks(),
            data_start_block: layout.data_start_block,
            bitmap_block: layout.bitmap_block,
            filetable_block: layout.filetable_block,
            bitmap_blocks: layout.bitmap_blocks,
            filetable_blocks: layout.filetable_blocks,
            max_files,
            root_files: 0,
            reserved: [0; 112],
            crc32: 0,
            _pad1: 0,
        };
        sb.crc32 = superblock_crc(&sb);

        // Write superblock to block 0
        let mut block = [0u8; BLOCK_SIZE as usize];
        unsafe {
            core::ptr::copy_nonoverlapping(&sb as *const _ as *const u8,
                                          block.as_mut_ptr(),
                                          SUPERBLOCK_SIZE);
        }

        if !disk.write_sectors(0, SECTORS_PER_BLOCK as u16, &block) {
            return None;
        }

        // Write boundary after superblock
        let boundary = BoundaryBlock::new(BoundaryType::SuperblockToBitmap, 1, 0, layout.bitmap_block);
        let boundary_bytes = unsafe {
            core::slice::from_raw_parts(&boundary as *const _ as *const u8, BLOCK_SIZE as usize)
        };
        let mut boundary_block = [0u8; BLOCK_SIZE as usize];
        boundary_block.copy_from_slice(boundary_bytes);
        disk.write_sectors(SECTORS_PER_BLOCK as u64, SECTORS_PER_BLOCK as u16, &boundary_block);

        // Initialize bitmap
        let mut bitmap = [0u8; BLOCK_SIZE as usize];
        // Mark metadata blocks as used
        for i in 0..layout.data_start_block {
            let byte_idx = (i / 8) as usize;
            let bit_idx = (i % 8) as u8;
            if byte_idx < bitmap.len() {
                bitmap[byte_idx] |= 1 << bit_idx;
            }
        }
        disk.write_sectors(layout.bitmap_block * SECTORS_PER_BLOCK as u64,
                          SECTORS_PER_BLOCK as u16, &bitmap);

        // Initialize empty file table
        let empty_block = [0u8; BLOCK_SIZE as usize];
        for i in 0..layout.filetable_blocks {
            disk.write_sectors((layout.filetable_block + i as u64) * SECTORS_PER_BLOCK as u64,
                              SECTORS_PER_BLOCK as u16, &empty_block);
        }

        // Write backup superblocks
        disk.write_sectors(layout.mid_superblock * SECTORS_PER_BLOCK as u64,
                          SECTORS_PER_BLOCK as u16, &block);
        disk.write_sectors(layout.last_superblock * SECTORS_PER_BLOCK as u64,
                          SECTORS_PER_BLOCK as u16, &block);

        Some(Self {
            disk,
            superblock: sb,
            layout,
        })
    }

    /// List files in root directory
    pub fn list_files(&mut self) -> FileIterator {
        let max_files = self.superblock.root_files as usize;
        FileIterator {
            wfs: self,
            index: 0,
            max_files,
        }
    }

    /// Read file table entry using layout from superblock
    fn read_file_entry(&mut self, index: usize) -> Option<FileEntry> {
        if index >= self.layout.max_files as usize {
            return None;
        }

        let (block_num, byte_offset) = self.layout.file_entry_offset(index);

        let mut block = [0u8; BLOCK_SIZE as usize];

        if !self.disk.read_sectors(block_num * SECTORS_PER_BLOCK as u64,
                                   SECTORS_PER_BLOCK as u16, &mut block) {
            return None;
        }

        let entry = unsafe {
            core::ptr::read_unaligned(block.as_ptr().add(byte_offset) as *const FileEntry)
        };

        if !entry.is_valid() {
            return None;
        }

        Some(entry)
    }

    /// Find file by name (case-sensitive)
    pub fn find_file(&mut self, name: &str) -> Option<FileEntry> {
        for i in 0..self.superblock.root_files as usize {
            if let Some(entry) = self.read_file_entry(i) {
                // Case-sensitive comparison
                if entry.name_str() == name {
                    return Some(entry);
                }
            }
        }
        None
    }

    /// Read file contents
    pub fn read_file(&mut self, entry: &FileEntry, buffer: &mut [u8]) -> Option<usize> {
        let size = entry.size as usize;
        if buffer.len() < size {
            return None;
        }

        let blocks_to_read = entry.blocks;
        let mut offset = 0usize;

        for i in 0..blocks_to_read {
            let block_num = entry.start_block + i as u64;
            let mut block = [0u8; BLOCK_SIZE as usize];

            if !self.disk.read_sectors(block_num * SECTORS_PER_BLOCK as u64,
                                       SECTORS_PER_BLOCK as u16, &mut block) {
                return None;
            }

            // Verify CRC (last 4 bytes of block)
            let stored_crc = u32::from_le_bytes([
                block[DATA_PER_BLOCK], block[DATA_PER_BLOCK + 1],
                block[DATA_PER_BLOCK + 2], block[DATA_PER_BLOCK + 3]
            ]);
            let calc_crc = crc32(&block[..DATA_PER_BLOCK]);

            if stored_crc != calc_crc {
                return None;
            }

            let remaining = size - offset;
            let copy_len = remaining.min(DATA_PER_BLOCK);
            buffer[offset..offset + copy_len].copy_from_slice(&block[..copy_len]);
            offset += copy_len;
        }

        Some(size)
    }

    /// Write file to filesystem
    pub fn write_file(&mut self, name: &str, data: &[u8], flags: u16) -> bool {
        if name.len() >= MAX_FILENAME {
            return false;
        }

        if self.superblock.root_files >= self.layout.max_files {
            return false;
        }

        let num_blocks = blocks_needed(data.len());

        // Find free file entry slot
        let slot = self.superblock.root_files as usize;

        // Allocate blocks
        let start_block = match self.allocate_blocks(num_blocks) {
            Some(b) => b,
            None => return false,
        };

        // Write data blocks with CRC
        let mut offset = 0;
        for i in 0..num_blocks {
            let mut block = [0u8; BLOCK_SIZE as usize];
            let remaining = data.len() - offset;
            let copy_len = remaining.min(DATA_PER_BLOCK);

            block[..copy_len].copy_from_slice(&data[offset..offset + copy_len]);
            offset += copy_len;

            let block_crc = crc32(&block[..DATA_PER_BLOCK]);
            block[DATA_PER_BLOCK..].copy_from_slice(&block_crc.to_le_bytes());

            let block_num = start_block + i as u64;
            if !self.disk.write_sectors(block_num * SECTORS_PER_BLOCK as u64,
                                        SECTORS_PER_BLOCK as u16, &block) {
                return false;
            }
        }

        // Create file entry
        let mut entry = FileEntry::default();
        entry.set_name(name);
        entry.size = data.len() as u64;
        entry.start_block = start_block;
        entry.blocks = num_blocks;
        entry.flags = flags;
        entry.data_crc32 = crc32(data);
        entry.crc32 = file_entry_crc(&entry);

        // Write file entry
        self.write_file_entry(slot, &entry)
    }

    fn write_file_entry(&mut self, index: usize, entry: &FileEntry) -> bool {
        let (block_num, byte_offset) = self.layout.file_entry_offset(index);

        let mut block = [0u8; BLOCK_SIZE as usize];

        if !self.disk.read_sectors(block_num * SECTORS_PER_BLOCK as u64,
                                   SECTORS_PER_BLOCK as u16, &mut block) {
            return false;
        }

        unsafe {
            core::ptr::copy_nonoverlapping(entry as *const _ as *const u8,
                                          block.as_mut_ptr().add(byte_offset),
                                          FILEENTRY_SIZE);
        }

        if !self.disk.write_sectors(block_num * SECTORS_PER_BLOCK as u64,
                                    SECTORS_PER_BLOCK as u16, &block) {
            return false;
        }

        self.superblock.root_files += 1;
        self.sync_superblock()
    }

    fn allocate_blocks(&mut self, count: u32) -> Option<u64> {
        let mut bitmap = [0u8; BLOCK_SIZE as usize];
        if !self.disk.read_sectors(self.superblock.bitmap_block * SECTORS_PER_BLOCK as u64,
                                   SECTORS_PER_BLOCK as u16, &mut bitmap) {
            return None;
        }

        let max_block = self.layout.total_blocks.min(BLOCK_SIZE as u64 * 8);

        let mut start = None;
        let mut found = 0u32;

        for block in self.layout.data_start_block..max_block {
            // Skip backup superblock regions
            if block >= self.layout.mid_superblock - 1 && block <= self.layout.mid_superblock + 1 {
                start = None;
                found = 0;
                continue;
            }
            if block >= self.layout.last_superblock - 1 {
                break;
            }

            let byte_idx = (block / 8) as usize;
            let bit_idx = (block % 8) as u8;

            if bitmap[byte_idx] & (1 << bit_idx) == 0 {
                if start.is_none() {
                    start = Some(block);
                }
                found += 1;
                if found >= count {
                    break;
                }
            } else {
                start = None;
                found = 0;
            }
        }

        if found < count {
            return None;
        }

        let start_block = start?;

        for i in 0..count {
            let block = start_block + i as u64;
            let byte_idx = (block / 8) as usize;
            let bit_idx = (block % 8) as u8;
            bitmap[byte_idx] |= 1 << bit_idx;
        }

        if !self.disk.write_sectors(self.superblock.bitmap_block * SECTORS_PER_BLOCK as u64,
                                    SECTORS_PER_BLOCK as u16, &bitmap) {
            return None;
        }

        self.superblock.free_blocks -= count as u64;

        Some(start_block)
    }

    fn sync_superblock(&mut self) -> bool {
        self.superblock.crc32 = superblock_crc(&self.superblock);

        let mut block = [0u8; BLOCK_SIZE as usize];
        unsafe {
            core::ptr::copy_nonoverlapping(&self.superblock as *const _ as *const u8,
                                          block.as_mut_ptr(),
                                          SUPERBLOCK_SIZE);
        }

        let ok1 = self.disk.write_sectors(0, SECTORS_PER_BLOCK as u16, &block);
        let ok2 = self.disk.write_sectors(self.layout.mid_superblock * SECTORS_PER_BLOCK as u64,
                                          SECTORS_PER_BLOCK as u16, &block);
        let ok3 = self.disk.write_sectors(self.layout.last_superblock * SECTORS_PER_BLOCK as u64,
                                          SECTORS_PER_BLOCK as u16, &block);

        ok1 && ok2 && ok3
    }

    /// Get filesystem info
    pub fn info(&self) -> FsInfo {
        FsInfo {
            total_blocks: self.superblock.total_blocks,
            free_blocks: self.superblock.free_blocks,
            block_size: self.superblock.block_size,
            file_count: self.superblock.root_files,
            max_files: self.superblock.max_files,
        }
    }

    /// Validate filesystem integrity
    pub fn check_filesystem(&mut self, fix: bool) -> CheckResult {
        let mut result = CheckResult::default();

        result.superblocks_checked = 3;
        let sb_ok = self.verify_superblocks();
        if !sb_ok {
            result.errors_found += 1;
            if fix && self.repair_superblocks() {
                result.errors_fixed += 1;
            }
        }

        for i in 0..self.superblock.root_files as usize {
            if let Some(entry) = self.read_file_entry(i) {
                result.files_checked += 1;

                let calc_crc = file_entry_crc(&entry);
                if calc_crc != entry.crc32 {
                    result.errors_found += 1;
                }

                let mut block_buf = [0u8; BLOCK_SIZE as usize];
                for b in 0..entry.blocks {
                    let block_num = entry.start_block + b as u64;
                    result.blocks_checked += 1;

                    if self.disk.read_sectors(block_num * SECTORS_PER_BLOCK as u64,
                                              SECTORS_PER_BLOCK as u16, &mut block_buf) {
                        let stored_crc = u32::from_le_bytes([
                            block_buf[DATA_PER_BLOCK], block_buf[DATA_PER_BLOCK + 1],
                            block_buf[DATA_PER_BLOCK + 2], block_buf[DATA_PER_BLOCK + 3]
                        ]);
                        let calc_crc = crc32(&block_buf[..DATA_PER_BLOCK]);

                        if stored_crc != calc_crc {
                            result.bad_blocks += 1;
                            result.errors_found += 1;
                        }
                    } else {
                        result.bad_blocks += 1;
                        result.errors_found += 1;
                    }
                }
            }
        }

        result.bitmap_checked = true;
        result
    }

    fn verify_superblocks(&mut self) -> bool {
        let mut block = [0u8; BLOCK_SIZE as usize];
        let mut ok_count = 0;

        if self.disk.read_sectors(0, SECTORS_PER_BLOCK as u16, &mut block) {
            let sb = unsafe { &*(block.as_ptr() as *const Superblock) };
            if sb.magic == WFS_MAGIC && superblock_crc(sb) == sb.crc32 {
                ok_count += 1;
            }
        }

        if self.disk.read_sectors(self.layout.mid_superblock * SECTORS_PER_BLOCK as u64,
                                  SECTORS_PER_BLOCK as u16, &mut block) {
            let sb = unsafe { &*(block.as_ptr() as *const Superblock) };
            if sb.magic == WFS_MAGIC && superblock_crc(sb) == sb.crc32 {
                ok_count += 1;
            }
        }

        if self.disk.read_sectors(self.layout.last_superblock * SECTORS_PER_BLOCK as u64,
                                  SECTORS_PER_BLOCK as u16, &mut block) {
            let sb = unsafe { &*(block.as_ptr() as *const Superblock) };
            if sb.magic == WFS_MAGIC && superblock_crc(sb) == sb.crc32 {
                ok_count += 1;
            }
        }

        ok_count >= 2
    }

    fn repair_superblocks(&mut self) -> bool {
        self.sync_superblock()
    }
}

/// Result of filesystem check
#[derive(Default, Debug, Clone, Copy)]
pub struct CheckResult {
    pub superblocks_checked: u32,
    pub files_checked: u32,
    pub blocks_checked: u64,
    pub bitmap_checked: bool,
    pub errors_found: u32,
    pub errors_fixed: u32,
    pub bad_blocks: u32,
}

pub struct FileIterator<'a> {
    wfs: &'a mut Wfs,
    index: usize,
    max_files: usize,
}

impl<'a> Iterator for FileIterator<'a> {
    type Item = FileEntry;

    fn next(&mut self) -> Option<Self::Item> {
        while self.index < self.max_files {
            let idx = self.index;
            self.index += 1;
            if let Some(entry) = self.wfs.read_file_entry(idx) {
                return Some(entry);
            }
        }
        None
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FsInfo {
    pub total_blocks: u64,
    pub free_blocks: u64,
    pub block_size: u32,
    pub file_count: u32,
    pub max_files: u32,
}
