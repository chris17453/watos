//! DOS Memory Management
//!
//! Implements conventional memory (640KB) with MCB-based allocation.

extern crate alloc;
use alloc::vec;
use alloc::vec::Vec;

use crate::cpu::Cpu16;

/// Size of conventional memory (640KB)
pub const CONVENTIONAL_MEMORY_SIZE: usize = 0xA0000;

/// Block size for memory allocation
pub const PARAGRAPH_SIZE: usize = 16;

// Memory layout constants
const FIRST_MCB_SEG: u16 = 0x0060;  // First MCB at segment 0x60
const CONV_MEM_END_SEG: u16 = 0xA000;  // Conventional memory ends at 640KB

// MCB types
const MCB_TYPE_MORE: u8 = 0x4D;  // 'M' - more blocks follow
const MCB_TYPE_LAST: u8 = 0x5A;  // 'Z' - last block
const MCB_OWNER_FREE: u16 = 0x0000;

/// DOS Memory - 1MB address space emulation
pub struct DosMemory {
    /// The actual memory (1MB)
    data: Vec<u8>,
}

impl DosMemory {
    /// Create new DOS memory with 1MB address space
    pub fn new() -> Self {
        let mut mem = DosMemory {
            data: vec![0u8; 0x100000],  // 1MB
        };

        // Initialize interrupt vector table (IVT) at 0x0000-0x03FF
        // Each vector is 4 bytes: offset (2), segment (2)
        // Point all to a default IRET handler

        // Initialize first MCB - entire conventional memory is free
        mem.init_mcb_chain();

        mem
    }

    /// Initialize the MCB chain
    fn init_mcb_chain(&mut self) {
        let mcb_addr = (FIRST_MCB_SEG as usize) * 16;
        let total_paras = (CONV_MEM_END_SEG - FIRST_MCB_SEG - 1) as u16;

        // Create a single free block covering all conventional memory
        self.data[mcb_addr] = MCB_TYPE_LAST;  // 'Z' - last block
        self.write16(mcb_addr + 1, MCB_OWNER_FREE);  // Free
        self.write16(mcb_addr + 3, total_paras);  // Size in paragraphs
    }

    /// Read a byte from memory
    #[inline]
    pub fn read8(&self, addr: usize) -> u8 {
        if addr < self.data.len() {
            self.data[addr]
        } else {
            0xFF
        }
    }

    /// Write a byte to memory
    #[inline]
    pub fn write8(&mut self, addr: usize, val: u8) {
        if addr < self.data.len() {
            self.data[addr] = val;
        }
    }

    /// Read a 16-bit word from memory (little-endian)
    #[inline]
    pub fn read16(&self, addr: usize) -> u16 {
        if addr + 1 < self.data.len() {
            u16::from_le_bytes([self.data[addr], self.data[addr + 1]])
        } else {
            0xFFFF
        }
    }

    /// Write a 16-bit word to memory (little-endian)
    #[inline]
    pub fn write16(&mut self, addr: usize, val: u16) {
        if addr + 1 < self.data.len() {
            let bytes = val.to_le_bytes();
            self.data[addr] = bytes[0];
            self.data[addr + 1] = bytes[1];
        }
    }

    /// Read from segment:offset address
    #[inline]
    pub fn read8_segoff(&self, seg: u16, off: u16) -> u8 {
        let addr = ((seg as usize) << 4) + (off as usize);
        self.read8(addr)
    }

    /// Write to segment:offset address
    #[inline]
    pub fn write8_segoff(&mut self, seg: u16, off: u16, val: u8) {
        let addr = ((seg as usize) << 4) + (off as usize);
        self.write8(addr, val);
    }

    /// Read 16-bit from segment:offset
    #[inline]
    pub fn read16_segoff(&self, seg: u16, off: u16) -> u16 {
        let addr = ((seg as usize) << 4) + (off as usize);
        self.read16(addr)
    }

    /// Write 16-bit to segment:offset
    #[inline]
    pub fn write16_segoff(&mut self, seg: u16, off: u16, val: u16) {
        let addr = ((seg as usize) << 4) + (off as usize);
        self.write16(addr, val);
    }

    /// Allocate memory for a program
    /// Returns the segment of allocated block, or None if no memory
    pub fn alloc_program(&mut self, size: usize) -> Option<u16> {
        // Convert size to paragraphs (round up)
        let paras = ((size + 15) / 16) as u16;
        self.alloc_paragraphs(paras)
    }

    /// Allocate paragraphs of memory (DOS INT 21h AH=48h)
    pub fn alloc_paragraphs(&mut self, paragraphs: u16) -> Option<u16> {
        let mut mcb_seg = FIRST_MCB_SEG;

        loop {
            let mcb_addr = (mcb_seg as usize) * 16;
            let mcb_type = self.read8(mcb_addr);
            let owner = self.read16(mcb_addr + 1);
            let size = self.read16(mcb_addr + 3);

            // Check if this block is free and large enough
            if owner == MCB_OWNER_FREE && size >= paragraphs {
                // Allocate from this block
                let alloc_seg = mcb_seg + 1;  // Memory starts after MCB

                if size > paragraphs + 1 {
                    // Split the block - create new MCB for remainder
                    let new_mcb_seg = mcb_seg + paragraphs + 1;
                    let new_mcb_addr = (new_mcb_seg as usize) * 16;
                    let remaining = size - paragraphs - 1;

                    self.data[new_mcb_addr] = mcb_type;  // Inherit type
                    self.write16(new_mcb_addr + 1, MCB_OWNER_FREE);
                    self.write16(new_mcb_addr + 3, remaining);

                    // Update current MCB
                    self.data[mcb_addr] = MCB_TYPE_MORE;  // Now has more blocks after
                    self.write16(mcb_addr + 3, paragraphs);
                }

                // Mark as owned (use segment as owner for now)
                self.write16(mcb_addr + 1, alloc_seg);

                return Some(alloc_seg);
            }

            // Move to next MCB
            if mcb_type == MCB_TYPE_LAST {
                break;
            }
            mcb_seg += size + 1;  // +1 for MCB itself
        }

        None
    }

    /// Free allocated memory (DOS INT 21h AH=49h)
    pub fn free_paragraphs(&mut self, segment: u16) -> bool {
        // The MCB is at segment - 1
        let mcb_seg = segment - 1;
        let mcb_addr = (mcb_seg as usize) * 16;

        let owner = self.read16(mcb_addr + 1);
        if owner != segment {
            return false;  // Not a valid allocation
        }

        // Mark as free
        self.write16(mcb_addr + 1, MCB_OWNER_FREE);

        // TODO: Coalesce adjacent free blocks
        true
    }

    /// Setup PSP (Program Segment Prefix) at segment
    pub fn setup_psp(&mut self, seg: u16, _filename: &str) {
        let base = (seg as usize) * 16;

        // INT 20h at offset 0
        self.data[base] = 0xCD;
        self.data[base + 1] = 0x20;

        // Memory size (top of memory)
        self.write16(base + 2, CONV_MEM_END_SEG);

        // Far call to DOS (INT 21h, RETF)
        self.data[base + 5] = 0xCD;
        self.data[base + 6] = 0x21;
        self.data[base + 7] = 0xCB;

        // Terminate address (parent PSP:0000)
        self.write16(base + 0x0A, 0);
        self.write16(base + 0x0C, seg);

        // Default FCBs at 5Ch and 6Ch
        self.data[base + 0x5C] = 0;  // FCB1 - no drive specified
        self.data[base + 0x6C] = 0;  // FCB2

        // Command line at 80h (empty)
        self.data[base + 0x80] = 0;  // Length = 0
        self.data[base + 0x81] = 0x0D;  // CR terminator
    }

    /// Load binary data at segment:offset
    pub fn load_at(&mut self, seg: u16, off: u16, data: &[u8]) {
        let base = ((seg as usize) << 4) + (off as usize);
        for (i, &byte) in data.iter().enumerate() {
            if base + i < self.data.len() {
                self.data[base + i] = byte;
            }
        }
    }

    /// Load DOS EXE file
    pub fn load_exe(&mut self, data: &[u8], cpu: &mut Cpu16) -> Option<u16> {
        if data.len() < 28 || &data[0..2] != b"MZ" {
            return None;
        }

        // Parse MZ header
        let last_page_size = u16::from_le_bytes([data[2], data[3]]);
        let total_pages = u16::from_le_bytes([data[4], data[5]]);
        let reloc_count = u16::from_le_bytes([data[6], data[7]]);
        let header_paras = u16::from_le_bytes([data[8], data[9]]);
        let min_alloc = u16::from_le_bytes([data[10], data[11]]);
        let _max_alloc = u16::from_le_bytes([data[12], data[13]]);
        let init_ss = u16::from_le_bytes([data[14], data[15]]);
        let init_sp = u16::from_le_bytes([data[16], data[17]]);
        let _checksum = u16::from_le_bytes([data[18], data[19]]);
        let init_ip = u16::from_le_bytes([data[20], data[21]]);
        let init_cs = u16::from_le_bytes([data[22], data[23]]);
        let reloc_offset = u16::from_le_bytes([data[24], data[25]]);

        // Calculate load size
        let header_size = (header_paras as usize) * 16;
        let load_size = if last_page_size == 0 {
            (total_pages as usize) * 512 - header_size
        } else {
            ((total_pages as usize) - 1) * 512 + (last_page_size as usize) - header_size
        };

        // Allocate memory for program
        let needed_paras = ((load_size + 15) / 16) as u16 + min_alloc + 16;  // +16 for PSP
        let psp_seg = self.alloc_paragraphs(needed_paras)?;
        let load_seg = psp_seg + 16;  // Load after PSP

        // Setup PSP
        self.setup_psp(psp_seg, "");

        // Load program
        let program_data = &data[header_size..];
        if program_data.len() >= load_size {
            self.load_at(load_seg, 0, &program_data[..load_size]);
        }

        // Apply relocations
        let reloc_table = &data[reloc_offset as usize..];
        for i in 0..(reloc_count as usize) {
            let entry_off = i * 4;
            if entry_off + 3 < reloc_table.len() {
                let off = u16::from_le_bytes([reloc_table[entry_off], reloc_table[entry_off + 1]]);
                let seg = u16::from_le_bytes([reloc_table[entry_off + 2], reloc_table[entry_off + 3]]);

                let addr = ((load_seg + seg) as usize) * 16 + (off as usize);
                if addr + 1 < self.data.len() {
                    let val = self.read16(addr);
                    self.write16(addr, val.wrapping_add(load_seg));
                }
            }
        }

        // Set up CPU registers
        cpu.cs = load_seg + init_cs;
        cpu.ip = init_ip;
        cpu.ss = load_seg + init_ss;
        cpu.sp = init_sp;
        cpu.ds = psp_seg;
        cpu.es = psp_seg;

        Some(psp_seg)
    }

    /// Get a slice of memory
    pub fn slice(&self, start: usize, len: usize) -> &[u8] {
        let end = (start + len).min(self.data.len());
        &self.data[start..end]
    }

    /// Get a mutable slice of memory
    pub fn slice_mut(&mut self, start: usize, len: usize) -> &mut [u8] {
        let end = (start + len).min(self.data.len());
        &mut self.data[start..end]
    }
}

impl Default for DosMemory {
    fn default() -> Self {
        Self::new()
    }
}
