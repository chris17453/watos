//! DOS 16-bit Runtime - x86 Real Mode Interpreter
//!
//! Implements a 16-bit x86 CPU interpreter for running DOS COM/EXE programs.

extern crate alloc;
use alloc::{string::{String, ToString}, vec::Vec, vec, format};
use super::{Runtime, BinaryFormat, RunResult, schedule_task_record};
use crate::disk;
use crate::console;

pub struct Dos16Runtime;
impl Dos16Runtime {
    pub fn new() -> Self { Dos16Runtime }
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum TaskState { Running, Blocked, Terminated }

// CPU Flags
const FLAG_CF: u16 = 0x0001;  // Carry
const FLAG_PF: u16 = 0x0004;  // Parity
const FLAG_AF: u16 = 0x0010;  // Auxiliary carry
const FLAG_ZF: u16 = 0x0040;  // Zero
const FLAG_SF: u16 = 0x0080;  // Sign
const FLAG_TF: u16 = 0x0100;  // Trap
const FLAG_IF: u16 = 0x0200;  // Interrupt enable
const FLAG_DF: u16 = 0x0400;  // Direction
const FLAG_OF: u16 = 0x0800;  // Overflow

// DOS Memory Control Block (MCB) constants
// MCB structure (16 bytes at start of each memory block):
//   Offset 0x00: Type - 'M' (0x4D) = more blocks, 'Z' (0x5A) = last block
//   Offset 0x01-0x02: Owner PSP segment (0 = free, 8 = DOS)
//   Offset 0x03-0x04: Size in paragraphs (not including MCB header)
//   Offset 0x05-0x07: Reserved
//   Offset 0x08-0x0F: Owner name (DOS 4+, optional)
const MCB_TYPE_MORE: u8 = 0x4D;  // 'M' - more blocks follow
const MCB_TYPE_LAST: u8 = 0x5A;  // 'Z' - last block in chain
const MCB_OWNER_FREE: u16 = 0x0000;  // Free block
const MCB_OWNER_DOS: u16 = 0x0008;   // DOS system block

// Memory layout:
// 0x00000 - 0x003FF: Interrupt Vector Table (256 vectors × 4 bytes)
// 0x00400 - 0x005FF: BIOS Data Area + DOS Data
// 0x00600 - onwards: First MCB, then allocatable memory
// 0xA0000 - 0xFFFFF: Video memory, ROM, etc. (not allocatable)
const FIRST_MCB_SEG: u16 = 0x0060;  // First MCB at segment 0x60 (linear 0x600)
const CONV_MEM_END_SEG: u16 = 0xA000;  // Conventional memory ends at 640KB

#[derive(Clone)]
pub struct Cpu16 {
    // General purpose registers
    pub ax: u16,
    pub bx: u16,
    pub cx: u16,
    pub dx: u16,
    pub si: u16,
    pub di: u16,
    pub bp: u16,
    pub sp: u16,
    // Instruction pointer
    pub ip: u16,
    // Segment registers
    pub cs: u16,
    pub ds: u16,
    pub es: u16,
    pub ss: u16,
    // Flags
    pub flags: u16,
}

impl Cpu16 {
    fn new() -> Self {
        Cpu16 {
            ax: 0, bx: 0, cx: 0, dx: 0,
            si: 0, di: 0, bp: 0, sp: 0xFFFE,
            ip: 0x100,
            cs: 0, ds: 0, es: 0, ss: 0,
            flags: 0x0002, // Bit 1 always set
        }
    }

    // Register accessors by index
    fn get_reg16(&self, idx: u8) -> u16 {
        match idx & 7 {
            0 => self.ax,
            1 => self.cx,
            2 => self.dx,
            3 => self.bx,
            4 => self.sp,
            5 => self.bp,
            6 => self.si,
            7 => self.di,
            _ => 0,
        }
    }

    fn set_reg16(&mut self, idx: u8, val: u16) {
        match idx & 7 {
            0 => self.ax = val,
            1 => self.cx = val,
            2 => self.dx = val,
            3 => self.bx = val,
            4 => self.sp = val,
            5 => self.bp = val,
            6 => self.si = val,
            7 => self.di = val,
            _ => {}
        }
    }

    fn get_reg8(&self, idx: u8) -> u8 {
        match idx & 7 {
            0 => self.ax as u8,        // AL
            1 => self.cx as u8,        // CL
            2 => self.dx as u8,        // DL
            3 => self.bx as u8,        // BL
            4 => (self.ax >> 8) as u8, // AH
            5 => (self.cx >> 8) as u8, // CH
            6 => (self.dx >> 8) as u8, // DH
            7 => (self.bx >> 8) as u8, // BH
            _ => 0,
        }
    }

    fn set_reg8(&mut self, idx: u8, val: u8) {
        match idx & 7 {
            0 => self.ax = (self.ax & 0xFF00) | val as u16,        // AL
            1 => self.cx = (self.cx & 0xFF00) | val as u16,        // CL
            2 => self.dx = (self.dx & 0xFF00) | val as u16,        // DL
            3 => self.bx = (self.bx & 0xFF00) | val as u16,        // BL
            4 => self.ax = (self.ax & 0x00FF) | ((val as u16) << 8), // AH
            5 => self.cx = (self.cx & 0x00FF) | ((val as u16) << 8), // CH
            6 => self.dx = (self.dx & 0x00FF) | ((val as u16) << 8), // DH
            7 => self.bx = (self.bx & 0x00FF) | ((val as u16) << 8), // BH
            _ => {}
        }
    }

    fn get_seg(&self, idx: u8) -> u16 {
        match idx & 3 {
            0 => self.es,
            1 => self.cs,
            2 => self.ss,
            3 => self.ds,
            _ => 0,
        }
    }

    fn set_seg(&mut self, idx: u8, val: u16) {
        match idx & 3 {
            0 => self.es = val,
            1 => self.cs = val,
            2 => self.ss = val,
            3 => self.ds = val,
            _ => {}
        }
    }

    // Flag helpers
    fn set_flag(&mut self, flag: u16, val: bool) {
        if val { self.flags |= flag; } else { self.flags &= !flag; }
    }

    fn get_flag(&self, flag: u16) -> bool {
        (self.flags & flag) != 0
    }

    fn update_flags_logic8(&mut self, result: u8) {
        self.set_flag(FLAG_ZF, result == 0);
        self.set_flag(FLAG_SF, (result & 0x80) != 0);
        self.set_flag(FLAG_PF, (result.count_ones() & 1) == 0);
        self.set_flag(FLAG_CF, false);
        self.set_flag(FLAG_OF, false);
    }

    fn update_flags_logic16(&mut self, result: u16) {
        self.set_flag(FLAG_ZF, result == 0);
        self.set_flag(FLAG_SF, (result & 0x8000) != 0);
        self.set_flag(FLAG_PF, ((result as u8).count_ones() & 1) == 0);
        self.set_flag(FLAG_CF, false);
        self.set_flag(FLAG_OF, false);
    }

    fn update_flags_add8(&mut self, a: u8, b: u8, result: u16) {
        let r = result as u8;
        self.set_flag(FLAG_ZF, r == 0);
        self.set_flag(FLAG_SF, (r & 0x80) != 0);
        self.set_flag(FLAG_PF, (r.count_ones() & 1) == 0);
        self.set_flag(FLAG_CF, result > 0xFF);
        self.set_flag(FLAG_OF, ((a ^ r) & (b ^ r) & 0x80) != 0);
        self.set_flag(FLAG_AF, ((a ^ b ^ r) & 0x10) != 0);
    }

    fn update_flags_add16(&mut self, a: u16, b: u16, result: u32) {
        let r = result as u16;
        self.set_flag(FLAG_ZF, r == 0);
        self.set_flag(FLAG_SF, (r & 0x8000) != 0);
        self.set_flag(FLAG_PF, ((r as u8).count_ones() & 1) == 0);
        self.set_flag(FLAG_CF, result > 0xFFFF);
        self.set_flag(FLAG_OF, ((a ^ r) & (b ^ r) & 0x8000) != 0);
        self.set_flag(FLAG_AF, ((a ^ b ^ r) & 0x10) != 0);
    }

    fn update_flags_sub8(&mut self, a: u8, b: u8, result: u16) {
        let r = result as u8;
        self.set_flag(FLAG_ZF, r == 0);
        self.set_flag(FLAG_SF, (r & 0x80) != 0);
        self.set_flag(FLAG_PF, (r.count_ones() & 1) == 0);
        self.set_flag(FLAG_CF, a < b);
        self.set_flag(FLAG_OF, ((a ^ b) & (a ^ r) & 0x80) != 0);
        self.set_flag(FLAG_AF, (a & 0x0F) < (b & 0x0F));
    }

    fn update_flags_sub16(&mut self, a: u16, b: u16, result: u32) {
        let r = result as u16;
        self.set_flag(FLAG_ZF, r == 0);
        self.set_flag(FLAG_SF, (r & 0x8000) != 0);
        self.set_flag(FLAG_PF, ((r as u8).count_ones() & 1) == 0);
        self.set_flag(FLAG_CF, a < b);
        self.set_flag(FLAG_OF, ((a ^ b) & (a ^ r) & 0x8000) != 0);
        self.set_flag(FLAG_AF, (a & 0x0F) < (b & 0x0F));
    }
}

pub struct DosTask {
    pub id: u32,
    pub filename: String,
    pub cpu: Cpu16,
    pub memory: Vec<u8>,
    pub state: TaskState,
    // Segment override for current instruction
    seg_override: Option<u16>,
    // File handles (maps DOS handle to internal state)
    file_handles: [Option<FileHandle>; 20],
    // Console ID for this task's display
    pub console_id: u8,
    // Instruction trace/debug
    trace_enabled: bool,
    instruction_count: u64,
    // PSP segment for this task
    psp_seg: u16,
}

#[derive(Clone)]
struct FileHandle {
    filename: String,
    position: u32,
    size: u32,
    data: Vec<u8>,
    writable: bool,
}

impl DosTask {
    fn new(id: u32, filename: String, data: &[u8]) -> Self {
        // Log program start to serial
        serial_write_bytes(b"\r\n=== DOS TASK START: ");
        serial_write_bytes(filename.as_bytes());
        serial_write_bytes(b" (0x");
        serial_write_hex16((data.len() >> 16) as u16);
        serial_write_hex16(data.len() as u16);
        serial_write_bytes(b" bytes) ===\r\n");
        serial_write_bytes(b"Instruction tracing ENABLED (every 100 instr)\r\n");

        let mut memory = vec![0u8; 1024 * 1024]; // 1 MiB

        // Initialize the MCB chain for memory management
        Self::init_mcb_chain(&mut memory);

        // Check if MZ (EXE) or COM
        let is_exe = data.len() >= 2 && data[0] == b'M' && data[1] == b'Z';
        serial_write_bytes(if is_exe { b"Format: EXE\r\n" } else { b"Format: COM\r\n" });

        let mut cpu = Cpu16::new();

        // Create a console for this DOS task
        let console_id = console::manager().create_console(&filename, id);
        // Switch to the new console
        console::manager().switch_to(console_id);

        let psp_seg: u16;
        if is_exe {
            // Parse MZ header and load EXE with proper memory allocation
            psp_seg = Self::load_exe(&mut memory, &mut cpu, data);
        } else {
            // For COM files, allocate memory and load at PSP:0100
            // COM files get all available memory (we'll allocate a reasonable amount)
            let com_paragraphs: u16 = 0x1000;  // 64KB should be enough for most COM files
            psp_seg = FIRST_MCB_SEG + 1;  // First available segment after MCB

            // Update MCB to allocate this block
            let mcb_addr = (FIRST_MCB_SEG as usize) << 4;
            let size = u16::from_le_bytes([memory[mcb_addr + 3], memory[mcb_addr + 4]]);
            if size >= com_paragraphs {
                let remaining = size - com_paragraphs - 1;
                memory[mcb_addr] = MCB_TYPE_MORE;
                memory[mcb_addr + 1] = (psp_seg & 0xFF) as u8;
                memory[mcb_addr + 2] = ((psp_seg >> 8) & 0xFF) as u8;
                memory[mcb_addr + 3] = (com_paragraphs & 0xFF) as u8;
                memory[mcb_addr + 4] = ((com_paragraphs >> 8) & 0xFF) as u8;

                // Create new free MCB
                let new_mcb_seg = FIRST_MCB_SEG + 1 + com_paragraphs;
                let new_mcb_addr = (new_mcb_seg as usize) << 4;
                memory[new_mcb_addr] = MCB_TYPE_LAST;
                memory[new_mcb_addr + 1] = 0;
                memory[new_mcb_addr + 2] = 0;
                memory[new_mcb_addr + 3] = (remaining & 0xFF) as u8;
                memory[new_mcb_addr + 4] = ((remaining >> 8) & 0xFF) as u8;
            }

            // Load COM at PSP:0100
            let psp_addr = (psp_seg as usize) << 4;
            let load_addr = psp_addr + 0x100;
            let copy_len = core::cmp::min(data.len(), memory.len() - load_addr);
            memory[load_addr..load_addr + copy_len].copy_from_slice(&data[..copy_len]);

            // Set up PSP
            memory[psp_addr] = 0xCD;  // INT 20h at PSP:0000
            memory[psp_addr + 1] = 0x20;
            let mem_top = psp_seg + com_paragraphs;
            memory[psp_addr + 2] = (mem_top & 0xFF) as u8;
            memory[psp_addr + 3] = ((mem_top >> 8) & 0xFF) as u8;

            cpu.ip = 0x100;
            cpu.cs = psp_seg;
            cpu.ds = psp_seg;
            cpu.es = psp_seg;
            cpu.ss = psp_seg;
            cpu.sp = 0xFFFE;  // Stack at top of segment
        }

        DosTask {
            id,
            filename,
            cpu,
            memory,
            state: TaskState::Running,
            seg_override: None,
            file_handles: [NONE_FILE; 20],
            console_id,
            trace_enabled: true,  // Enable tracing by default for debugging
            instruction_count: 0,
            psp_seg,
        }
    }

    // ============ MCB (Memory Control Block) Methods ============

    /// Initialize the MCB chain with a single free block covering all conventional memory
    fn init_mcb_chain(memory: &mut [u8]) {
        // Create initial MCB at FIRST_MCB_SEG
        let mcb_addr = (FIRST_MCB_SEG as usize) << 4;
        // Calculate size: from (FIRST_MCB_SEG + 1) to CONV_MEM_END_SEG
        // The MCB takes 1 paragraph, usable memory starts at FIRST_MCB_SEG + 1
        let free_paragraphs = CONV_MEM_END_SEG - FIRST_MCB_SEG - 1;

        memory[mcb_addr] = MCB_TYPE_LAST;  // 'Z' - last block
        memory[mcb_addr + 1] = 0;  // Owner low byte (free)
        memory[mcb_addr + 2] = 0;  // Owner high byte
        memory[mcb_addr + 3] = (free_paragraphs & 0xFF) as u8;  // Size low
        memory[mcb_addr + 4] = ((free_paragraphs >> 8) & 0xFF) as u8;  // Size high
        // Bytes 5-15 are reserved/name, leave as zero

        serial_write_bytes(b"MCB chain initialized: first MCB at ");
        serial_write_hex16(FIRST_MCB_SEG);
        serial_write_bytes(b", free paragraphs: ");
        serial_write_hex16(free_paragraphs);
        serial_write_bytes(b"\r\n");
    }

    /// Read MCB at given segment
    fn read_mcb(&self, seg: u16) -> (u8, u16, u16) {
        let addr = (seg as usize) << 4;
        let mcb_type = self.memory[addr];
        let owner = u16::from_le_bytes([self.memory[addr + 1], self.memory[addr + 2]]);
        let size = u16::from_le_bytes([self.memory[addr + 3], self.memory[addr + 4]]);
        (mcb_type, owner, size)
    }

    /// Write MCB at given segment
    fn write_mcb(&mut self, seg: u16, mcb_type: u8, owner: u16, size: u16) {
        let addr = (seg as usize) << 4;
        self.memory[addr] = mcb_type;
        self.memory[addr + 1] = (owner & 0xFF) as u8;
        self.memory[addr + 2] = ((owner >> 8) & 0xFF) as u8;
        self.memory[addr + 3] = (size & 0xFF) as u8;
        self.memory[addr + 4] = ((size >> 8) & 0xFF) as u8;
    }

    /// Allocate memory block of given size in paragraphs
    /// Returns segment of allocated block (block starts at segment + 1, MCB is at segment)
    /// On failure, returns None and sets largest_free to the largest available block
    fn alloc_memory(&mut self, paragraphs: u16) -> Option<u16> {
        let mut mcb_seg = FIRST_MCB_SEG;

        loop {
            let (mcb_type, owner, size) = self.read_mcb(mcb_seg);

            // Is this block free and big enough?
            if owner == MCB_OWNER_FREE && size >= paragraphs {
                // Found a suitable block
                let block_seg = mcb_seg + 1;  // Usable memory starts after MCB

                if size > paragraphs + 1 {
                    // Split the block: create a new MCB after our allocation
                    let new_mcb_seg = mcb_seg + 1 + paragraphs;
                    let remaining = size - paragraphs - 1;  // -1 for new MCB paragraph

                    // Update current MCB
                    self.write_mcb(mcb_seg, MCB_TYPE_MORE, self.psp_seg, paragraphs);

                    // Create new MCB for remaining free space
                    self.write_mcb(new_mcb_seg, mcb_type, MCB_OWNER_FREE, remaining);
                } else {
                    // Use entire block (no split needed)
                    self.write_mcb(mcb_seg, mcb_type, self.psp_seg, size);
                }

                serial_write_bytes(b"Allocated ");
                serial_write_hex16(paragraphs);
                serial_write_bytes(b" paragraphs at segment ");
                serial_write_hex16(block_seg);
                serial_write_bytes(b"\r\n");

                return Some(block_seg);
            }

            // Move to next MCB
            if mcb_type == MCB_TYPE_LAST {
                break;  // No more blocks
            }
            mcb_seg = mcb_seg + size + 1;  // +1 for MCB paragraph
        }

        serial_write_bytes(b"Memory allocation failed: need ");
        serial_write_hex16(paragraphs);
        serial_write_bytes(b" paragraphs\r\n");
        None
    }

    /// Free memory block at given segment
    fn free_memory(&mut self, seg: u16) -> bool {
        // The segment should point to a memory block, MCB is at seg - 1
        let mcb_seg = seg.wrapping_sub(1);

        if mcb_seg < FIRST_MCB_SEG {
            return false;  // Invalid segment
        }

        let (mcb_type, _owner, size) = self.read_mcb(mcb_seg);

        // Mark as free
        self.write_mcb(mcb_seg, mcb_type, MCB_OWNER_FREE, size);

        // TODO: Coalesce adjacent free blocks
        serial_write_bytes(b"Freed memory at segment ");
        serial_write_hex16(seg);
        serial_write_bytes(b"\r\n");

        true
    }

    /// Resize memory block at given segment
    /// Returns true on success, false on failure (sets BX to max available)
    fn resize_memory(&mut self, seg: u16, new_paragraphs: u16) -> Result<(), u16> {
        let mcb_seg = seg.wrapping_sub(1);

        if mcb_seg < FIRST_MCB_SEG {
            return Err(0);
        }

        let (mcb_type, owner, current_size) = self.read_mcb(mcb_seg);

        if new_paragraphs <= current_size {
            // Shrinking - always succeeds
            if current_size > new_paragraphs + 1 && mcb_type == MCB_TYPE_MORE {
                // Create free block with extra space
                let new_mcb_seg = mcb_seg + 1 + new_paragraphs;
                let remaining = current_size - new_paragraphs - 1;

                self.write_mcb(mcb_seg, MCB_TYPE_MORE, owner, new_paragraphs);
                self.write_mcb(new_mcb_seg, mcb_type, MCB_OWNER_FREE, remaining);
            } else {
                self.write_mcb(mcb_seg, mcb_type, owner, new_paragraphs);
            }
            return Ok(());
        }

        // Growing - check if next block is free and big enough
        if mcb_type == MCB_TYPE_MORE {
            let next_mcb_seg = mcb_seg + current_size + 1;
            let (next_type, next_owner, next_size) = self.read_mcb(next_mcb_seg);

            if next_owner == MCB_OWNER_FREE {
                let _extra_needed = new_paragraphs - current_size;
                let total_available = current_size + 1 + next_size;  // +1 for absorbed MCB

                if total_available >= new_paragraphs {
                    // Can expand into next block
                    if total_available > new_paragraphs + 1 {
                        // Split remaining
                        let new_mcb_seg = mcb_seg + 1 + new_paragraphs;
                        let remaining = total_available - new_paragraphs - 1;
                        self.write_mcb(mcb_seg, MCB_TYPE_MORE, owner, new_paragraphs);
                        self.write_mcb(new_mcb_seg, next_type, MCB_OWNER_FREE, remaining);
                    } else {
                        self.write_mcb(mcb_seg, next_type, owner, total_available);
                    }
                    return Ok(());
                }
            }
        }

        // Cannot resize - return max available
        Err(current_size)
    }

    /// Write a character to this task's console
    fn console_putchar(&self, ch: u8) {
        if let Some(con) = console::manager().get(self.console_id) as Option<&mut console::Console> {
            con.putchar(ch);
        }
    }

    /// Write bytes to this task's console
    fn console_write(&self, data: &[u8]) {
        if let Some(con) = console::manager().get(self.console_id) as Option<&mut console::Console> {
            con.print(data);
        }
    }

    /// Load an EXE file with proper memory allocation
    /// Returns the PSP segment on success
    fn load_exe(memory: &mut [u8], cpu: &mut Cpu16, data: &[u8]) -> u16 {
        if data.len() < 28 { return 0; }

        // MZ header parsing
        let last_page_size = u16::from_le_bytes([data[2], data[3]]);
        let page_count = u16::from_le_bytes([data[4], data[5]]);
        let reloc_count = u16::from_le_bytes([data[6], data[7]]) as usize;
        let header_paras = u16::from_le_bytes([data[8], data[9]]) as usize;
        let min_alloc = u16::from_le_bytes([data[10], data[11]]);
        let _max_alloc = u16::from_le_bytes([data[12], data[13]]);
        let init_ss = u16::from_le_bytes([data[14], data[15]]);
        let init_sp = u16::from_le_bytes([data[16], data[17]]);
        let _checksum = u16::from_le_bytes([data[18], data[19]]);
        let init_ip = u16::from_le_bytes([data[20], data[21]]);
        let init_cs = u16::from_le_bytes([data[22], data[23]]);
        let reloc_offset = u16::from_le_bytes([data[24], data[25]]) as usize;

        // Calculate code size in bytes and paragraphs
        let header_size = header_paras * 16;
        let code_size = if last_page_size == 0 {
            (page_count as usize) * 512 - header_size
        } else {
            ((page_count as usize) - 1) * 512 + (last_page_size as usize) - header_size
        };
        let code_paragraphs = ((code_size + 15) / 16) as u16;

        // Total memory needed: PSP (0x10 paragraphs) + code + min_alloc
        // min_alloc is extra memory the program needs beyond its code
        let psp_paragraphs: u16 = 0x10;  // PSP is 256 bytes = 16 paragraphs
        let total_paragraphs = psp_paragraphs + code_paragraphs + min_alloc;

        serial_write_bytes(b"EXE memory: code=");
        serial_write_hex16(code_paragraphs);
        serial_write_bytes(b" min_alloc=");
        serial_write_hex16(min_alloc);
        serial_write_bytes(b" total=");
        serial_write_hex16(total_paragraphs);
        serial_write_bytes(b" paragraphs\r\n");

        // Allocate memory from the MCB chain
        // Find a free block big enough
        let mut mcb_seg = FIRST_MCB_SEG;
        let mut psp_seg: u16 = 0;

        loop {
            let mcb_addr = (mcb_seg as usize) << 4;
            let mcb_type = memory[mcb_addr];
            let owner = u16::from_le_bytes([memory[mcb_addr + 1], memory[mcb_addr + 2]]);
            let size = u16::from_le_bytes([memory[mcb_addr + 3], memory[mcb_addr + 4]]);

            if owner == MCB_OWNER_FREE && size >= total_paragraphs {
                // Found a suitable block - allocate it
                psp_seg = mcb_seg + 1;  // PSP starts right after MCB

                if size > total_paragraphs + 1 {
                    // Split the block
                    let new_mcb_seg = mcb_seg + 1 + total_paragraphs;
                    let remaining = size - total_paragraphs - 1;

                    // Update current MCB (allocated to program, PSP is owner)
                    memory[mcb_addr] = MCB_TYPE_MORE;
                    memory[mcb_addr + 1] = (psp_seg & 0xFF) as u8;
                    memory[mcb_addr + 2] = ((psp_seg >> 8) & 0xFF) as u8;
                    memory[mcb_addr + 3] = (total_paragraphs & 0xFF) as u8;
                    memory[mcb_addr + 4] = ((total_paragraphs >> 8) & 0xFF) as u8;

                    // Create new free MCB for remaining space
                    let new_mcb_addr = (new_mcb_seg as usize) << 4;
                    memory[new_mcb_addr] = mcb_type;  // Keep original type (Z if last)
                    memory[new_mcb_addr + 1] = 0;  // Free
                    memory[new_mcb_addr + 2] = 0;
                    memory[new_mcb_addr + 3] = (remaining & 0xFF) as u8;
                    memory[new_mcb_addr + 4] = ((remaining >> 8) & 0xFF) as u8;
                } else {
                    // Use entire block
                    memory[mcb_addr + 1] = (psp_seg & 0xFF) as u8;
                    memory[mcb_addr + 2] = ((psp_seg >> 8) & 0xFF) as u8;
                }

                serial_write_bytes(b"Allocated EXE block: PSP at segment ");
                serial_write_hex16(psp_seg);
                serial_write_bytes(b"\r\n");
                break;
            }

            if mcb_type == MCB_TYPE_LAST {
                serial_write_bytes(b"ERROR: Not enough memory for EXE!\r\n");
                return 0;
            }
            mcb_seg = mcb_seg + size + 1;
        }

        // Load segment is right after PSP (PSP is 0x10 paragraphs)
        let load_seg = psp_seg + psp_paragraphs;
        let load_addr = (load_seg as usize) << 4;

        // Copy code to load address
        if header_size < data.len() {
            let code_start = header_size;
            let code_end = core::cmp::min(code_start + code_size, data.len());
            let copy_len = core::cmp::min(code_end - code_start, memory.len() - load_addr);
            memory[load_addr..load_addr + copy_len]
                .copy_from_slice(&data[code_start..code_start + copy_len]);
        }

        // Apply relocations (using load_seg as base)
        for i in 0..reloc_count {
            let rel_off = reloc_offset + i * 4;
            if rel_off + 4 <= data.len() {
                let off = u16::from_le_bytes([data[rel_off], data[rel_off + 1]]) as usize;
                let seg = u16::from_le_bytes([data[rel_off + 2], data[rel_off + 3]]) as usize;
                let addr = load_addr + (seg << 4) + off;
                if addr + 2 <= memory.len() {
                    let val = u16::from_le_bytes([memory[addr], memory[addr + 1]]);
                    let new_val = val.wrapping_add(load_seg);
                    memory[addr] = new_val as u8;
                    memory[addr + 1] = (new_val >> 8) as u8;
                }
            }
        }

        // Set up PSP at psp_seg
        let psp_addr = (psp_seg as usize) << 4;
        // INT 20h at PSP:0000
        memory[psp_addr] = 0xCD;
        memory[psp_addr + 1] = 0x20;
        // Memory size at PSP:0002 (segment of first byte beyond allocated memory)
        let mem_top = psp_seg + total_paragraphs;
        memory[psp_addr + 2] = (mem_top & 0xFF) as u8;
        memory[psp_addr + 3] = ((mem_top >> 8) & 0xFF) as u8;

        // Set up registers - segments relative to load_seg, not psp_seg
        cpu.cs = load_seg.wrapping_add(init_cs);
        cpu.ip = init_ip;
        cpu.ss = load_seg.wrapping_add(init_ss);
        cpu.sp = init_sp;
        cpu.ds = psp_seg;  // DS and ES point to PSP
        cpu.es = psp_seg;

        // Log initial EXE state
        serial_write_bytes(b"EXE load: pages=");
        serial_write_hex16(page_count);
        serial_write_bytes(b" relocs=");
        serial_write_hex16(reloc_count as u16);
        serial_write_bytes(b" hdr_paras=");
        serial_write_hex16(header_paras as u16);
        serial_write_bytes(b"\r\n");
        serial_write_bytes(b"EXE init: PSP=");
        serial_write_hex16(psp_seg);
        serial_write_bytes(b" load_seg=");
        serial_write_hex16(load_seg);
        serial_write_bytes(b" CS=");
        serial_write_hex16(cpu.cs);
        serial_write_bytes(b" IP=");
        serial_write_hex16(cpu.ip);
        serial_write_bytes(b"\r\n");
        serial_write_bytes(b"SS=");
        serial_write_hex16(cpu.ss);
        serial_write_bytes(b" SP=");
        serial_write_hex16(cpu.sp);
        serial_write_bytes(b" DS/ES=");
        serial_write_hex16(cpu.ds);
        serial_write_bytes(b"\r\n");
        serial_write_bytes(b"First 8 bytes at CS:IP: ");
        let start_addr = ((cpu.cs as usize) << 4) + (cpu.ip as usize);
        for i in 0..8 {
            if start_addr + i < memory.len() {
                serial_write_hex8(memory[start_addr + i]);
                serial_write_bytes(b" ");
            }
        }
        serial_write_bytes(b"\r\n");

        psp_seg
    }

    // Linear address calculation
    fn lin(&self, seg: u16, off: u16) -> usize {
        ((seg as usize) << 4).wrapping_add(off as usize) & 0xFFFFF
    }

    // Memory access
    fn read_u8(&self, seg: u16, off: u16) -> u8 {
        let addr = self.lin(seg, off);
        if addr < self.memory.len() { self.memory[addr] } else { 0 }
    }

    fn read_u16(&self, seg: u16, off: u16) -> u16 {
        let lo = self.read_u8(seg, off) as u16;
        let hi = self.read_u8(seg, off.wrapping_add(1)) as u16;
        lo | (hi << 8)
    }

    fn write_u8(&mut self, seg: u16, off: u16, val: u8) {
        let addr = self.lin(seg, off);
        if addr < self.memory.len() { self.memory[addr] = val; }
    }

    fn write_u16(&mut self, seg: u16, off: u16, val: u16) {
        self.write_u8(seg, off, val as u8);
        self.write_u8(seg, off.wrapping_add(1), (val >> 8) as u8);
    }

    // Fetch from CS:IP
    fn fetch_u8(&mut self) -> u8 {
        let val = self.read_u8(self.cpu.cs, self.cpu.ip);
        self.cpu.ip = self.cpu.ip.wrapping_add(1);
        val
    }

    fn fetch_u16(&mut self) -> u16 {
        let lo = self.fetch_u8() as u16;
        let hi = self.fetch_u8() as u16;
        lo | (hi << 8)
    }

    fn fetch_i8(&mut self) -> i8 {
        self.fetch_u8() as i8
    }

    // Stack operations
    fn push16(&mut self, val: u16) {
        self.cpu.sp = self.cpu.sp.wrapping_sub(2);
        self.write_u16(self.cpu.ss, self.cpu.sp, val);
    }

    fn pop16(&mut self) -> u16 {
        let val = self.read_u16(self.cpu.ss, self.cpu.sp);
        self.cpu.sp = self.cpu.sp.wrapping_add(2);
        val
    }

    // Get effective segment (with override support)
    fn get_seg(&self, default: u16) -> u16 {
        self.seg_override.unwrap_or(default)
    }

    // ModR/M decoding - returns (reg field, effective address or register value, rm field, is_memory)
    fn decode_modrm(&mut self, wide: bool) -> (u8, u16, u8, bool) {
        let modrm = self.fetch_u8();
        let mode = (modrm >> 6) & 3;
        let reg = (modrm >> 3) & 7;
        let rm = modrm & 7;

        if mode == 3 {
            // Register mode - ea holds register value, rm is register index for writes
            let val = if wide { self.cpu.get_reg16(rm) } else { self.cpu.get_reg8(rm) as u16 };
            return (reg, val, rm, false);
        }

        // Memory mode - calculate effective address
        let mut ea: u16 = match rm {
            0 => self.cpu.bx.wrapping_add(self.cpu.si),
            1 => self.cpu.bx.wrapping_add(self.cpu.di),
            2 => self.cpu.bp.wrapping_add(self.cpu.si),
            3 => self.cpu.bp.wrapping_add(self.cpu.di),
            4 => self.cpu.si,
            5 => self.cpu.di,
            6 => if mode == 0 { self.fetch_u16() } else { self.cpu.bp },
            7 => self.cpu.bx,
            _ => 0,
        };

        // Add displacement
        match mode {
            1 => ea = ea.wrapping_add(self.fetch_i8() as i16 as u16),
            2 => ea = ea.wrapping_add(self.fetch_u16()),
            _ => {}
        }

        // Default segment
        let _default_seg = if rm == 2 || rm == 3 || (rm == 6 && mode != 0) {
            self.cpu.ss // BP-based addressing uses SS
        } else {
            self.cpu.ds
        };

        (reg, ea, rm, true)
    }

    // Read operand from ModR/M - returns (reg, val, ea, rm, is_mem)
    fn read_rm8(&mut self) -> (u8, u8, u16, u8, bool) {
        let (reg, ea, rm, is_mem) = self.decode_modrm(false);
        let val = if is_mem {
            let seg = self.get_seg(self.cpu.ds);
            self.read_u8(seg, ea)
        } else {
            ea as u8
        };
        (reg, val, ea, rm, is_mem)
    }

    fn read_rm16(&mut self) -> (u8, u16, u16, u8, bool) {
        let (reg, ea, rm, is_mem) = self.decode_modrm(true);
        let val = if is_mem {
            let seg = self.get_seg(self.cpu.ds);
            self.read_u16(seg, ea)
        } else {
            ea
        };
        (reg, val, ea, rm, is_mem)
    }

    // Write to ModR/M destination
    fn write_rm8(&mut self, ea: u16, is_mem: bool, rm: u8, val: u8) {
        if is_mem {
            let seg = self.get_seg(self.cpu.ds);
            self.write_u8(seg, ea, val);
        } else {
            self.cpu.set_reg8(rm, val);
        }
    }

    fn write_rm16(&mut self, ea: u16, is_mem: bool, rm: u8, val: u16) {
        if is_mem {
            let seg = self.get_seg(self.cpu.ds);
            self.write_u16(seg, ea, val);
        } else {
            self.cpu.set_reg16(rm, val);
        }
    }

    // Execute one instruction
    pub fn step(&mut self) {
        if self.state != TaskState::Running { return; }
        self.seg_override = None;
        self.step_inner(false);
    }

    // Inner step that handles segment override prefixes
    fn step_inner(&mut self, has_seg_prefix: bool) {
        // Trace: print CS:IP and opcode
        let trace_cs = self.cpu.cs;
        let trace_ip = self.cpu.ip;

        let opcode = self.fetch_u8();

        // Debug trace output - enable for instruction tracing (only count once per full instruction)
        if !has_seg_prefix {
            self.instruction_count += 1;
        }
        // Trace first 20 instructions in detail, then every 100th, BUT always trace decompressor
        let trace_decompressor = trace_cs == 0x267A;  // PKLITE decompressor segment
        if self.trace_enabled && !has_seg_prefix && (trace_decompressor || self.instruction_count <= 20 || self.instruction_count % 100 == 0) {
            serial_write_bytes(b"[");
            serial_write_hex16(trace_cs);
            serial_write_bytes(b":");
            serial_write_hex16(trace_ip);
            serial_write_bytes(b"] op=");
            serial_write_hex8(opcode);
            // For decompressor, show BP (bit buffer), DS:SI (data pointer)
            if trace_decompressor {
                serial_write_bytes(b" DS=");
                serial_write_hex16(self.cpu.ds);
                serial_write_bytes(b" SI=");
                serial_write_hex16(self.cpu.si);
                serial_write_bytes(b" BP=");
                serial_write_hex16(self.cpu.bp);
                serial_write_bytes(b" AX=");
                serial_write_hex16(self.cpu.ax);
            } else {
                serial_write_bytes(b" AX=");
                serial_write_hex16(self.cpu.ax);
                serial_write_bytes(b" SP=");
                serial_write_hex16(self.cpu.sp);
            }
            serial_write_bytes(b" SS:SP=");
            serial_write_hex16(self.cpu.ss);
            serial_write_bytes(b":");
            serial_write_hex16(self.cpu.sp);
            serial_write_bytes(b"\r\n");
        }
        let _ = (trace_cs, trace_ip); // Silence unused warnings

        match opcode {
            // Segment override prefixes - set override and continue with the actual instruction
            0x26 => { self.seg_override = Some(self.cpu.es); self.step_inner(true); }
            0x2E => { self.seg_override = Some(self.cpu.cs); self.step_inner(true); }
            0x36 => { self.seg_override = Some(self.cpu.ss); self.step_inner(true); }
            0x3E => { self.seg_override = Some(self.cpu.ds); self.step_inner(true); }

            // ADD r/m8, r8
            0x00 => {
                let (reg, val, ea, rm, is_mem) = self.read_rm8();
                let src = self.cpu.get_reg8(reg);
                let result = (val as u16).wrapping_add(src as u16);
                self.cpu.update_flags_add8(val, src, result);
                self.write_rm8(ea, is_mem, rm, result as u8);
            }
            // ADD r/m16, r16
            0x01 => {
                let (reg, val, ea, rm, is_mem) = self.read_rm16();
                let src = self.cpu.get_reg16(reg);
                let result = (val as u32).wrapping_add(src as u32);
                self.cpu.update_flags_add16(val, src, result);
                self.write_rm16(ea, is_mem, rm, result as u16);
            }
            // ADD r8, r/m8
            0x02 => {
                let (reg, val, _, _, _) = self.read_rm8();
                let dst = self.cpu.get_reg8(reg);
                let result = (dst as u16).wrapping_add(val as u16);
                self.cpu.update_flags_add8(dst, val, result);
                self.cpu.set_reg8(reg, result as u8);
            }
            // ADD r16, r/m16
            0x03 => {
                let (reg, val, _, _, _) = self.read_rm16();
                let dst = self.cpu.get_reg16(reg);
                let result = (dst as u32).wrapping_add(val as u32);
                self.cpu.update_flags_add16(dst, val, result);
                self.cpu.set_reg16(reg, result as u16);
            }
            // ADD AL, imm8
            0x04 => {
                let imm = self.fetch_u8();
                let al = self.cpu.ax as u8;
                let result = (al as u16).wrapping_add(imm as u16);
                self.cpu.update_flags_add8(al, imm, result);
                self.cpu.ax = (self.cpu.ax & 0xFF00) | (result as u16 & 0xFF);
            }
            // ADD AX, imm16
            0x05 => {
                let imm = self.fetch_u16();
                let ax = self.cpu.ax;
                let result = (ax as u32).wrapping_add(imm as u32);
                // Debug: log ADD AX details
                serial_write_bytes(b"  ADD AX: ");
                serial_write_hex16(ax);
                serial_write_bytes(b" + ");
                serial_write_hex16(imm);
                serial_write_bytes(b" = ");
                serial_write_hex16(result as u16);
                serial_write_bytes(b"\r\n");
                self.cpu.update_flags_add16(ax, imm, result);
                self.cpu.ax = result as u16;
            }

            // PUSH ES
            0x06 => self.push16(self.cpu.es),
            // POP ES
            0x07 => self.cpu.es = self.pop16(),

            // OR r/m8, r8
            0x08 => {
                let (reg, val, ea, rm, is_mem) = self.read_rm8();
                let src = self.cpu.get_reg8(reg);
                let result = val | src;
                self.cpu.update_flags_logic8(result);
                self.write_rm8(ea, is_mem, rm, result);
            }
            // OR r/m16, r16
            0x09 => {
                let (reg, val, ea, rm, is_mem) = self.read_rm16();
                let src = self.cpu.get_reg16(reg);
                let result = val | src;
                self.cpu.update_flags_logic16(result);
                self.write_rm16(ea, is_mem, rm, result);
            }
            // OR r8, r/m8
            0x0A => {
                let (reg, val, _, _, _) = self.read_rm8();
                let dst = self.cpu.get_reg8(reg);
                let result = dst | val;
                self.cpu.update_flags_logic8(result);
                self.cpu.set_reg8(reg, result);
            }
            // OR r16, r/m16
            0x0B => {
                let (reg, val, _, _, _) = self.read_rm16();
                let dst = self.cpu.get_reg16(reg);
                let result = dst | val;
                self.cpu.update_flags_logic16(result);
                self.cpu.set_reg16(reg, result);
            }
            // OR AL, imm8
            0x0C => {
                let imm = self.fetch_u8();
                let result = (self.cpu.ax as u8) | imm;
                self.cpu.update_flags_logic8(result);
                self.cpu.ax = (self.cpu.ax & 0xFF00) | result as u16;
            }
            // OR AX, imm16
            0x0D => {
                let imm = self.fetch_u16();
                let result = self.cpu.ax | imm;
                self.cpu.update_flags_logic16(result);
                self.cpu.ax = result;
            }

            // PUSH CS
            0x0E => self.push16(self.cpu.cs),

            // ADC (with carry) - 0x10-0x15
            0x10..=0x15 => self.exec_adc(opcode),

            // PUSH SS
            0x16 => self.push16(self.cpu.ss),
            // POP SS
            0x17 => self.cpu.ss = self.pop16(),

            // SBB (with borrow) - 0x18-0x1D
            0x18..=0x1D => self.exec_sbb(opcode),

            // PUSH DS
            0x1E => self.push16(self.cpu.ds),
            // POP DS
            0x1F => self.cpu.ds = self.pop16(),

            // AND r/m8, r8
            0x20 => {
                let (reg, val, ea, rm, is_mem) = self.read_rm8();
                let src = self.cpu.get_reg8(reg);
                let result = val & src;
                self.cpu.update_flags_logic8(result);
                self.write_rm8(ea, is_mem, rm, result);
            }
            // AND r/m16, r16
            0x21 => {
                let (reg, val, ea, rm, is_mem) = self.read_rm16();
                let src = self.cpu.get_reg16(reg);
                let result = val & src;
                self.cpu.update_flags_logic16(result);
                self.write_rm16(ea, is_mem, rm, result);
            }
            // AND r8, r/m8
            0x22 => {
                let (reg, val, _, _, _) = self.read_rm8();
                let dst = self.cpu.get_reg8(reg);
                let result = dst & val;
                self.cpu.update_flags_logic8(result);
                self.cpu.set_reg8(reg, result);
            }
            // AND r16, r/m16
            0x23 => {
                let (reg, val, _, _, _) = self.read_rm16();
                let dst = self.cpu.get_reg16(reg);
                let result = dst & val;
                self.cpu.update_flags_logic16(result);
                self.cpu.set_reg16(reg, result);
            }
            // AND AL, imm8
            0x24 => {
                let imm = self.fetch_u8();
                let result = (self.cpu.ax as u8) & imm;
                self.cpu.update_flags_logic8(result);
                self.cpu.ax = (self.cpu.ax & 0xFF00) | result as u16;
            }
            // AND AX, imm16
            0x25 => {
                let imm = self.fetch_u16();
                let result = self.cpu.ax & imm;
                self.cpu.update_flags_logic16(result);
                self.cpu.ax = result;
            }

            // DAA
            0x27 => self.exec_daa(),

            // SUB r/m8, r8
            0x28 => {
                let (reg, val, ea, rm, is_mem) = self.read_rm8();
                let src = self.cpu.get_reg8(reg);
                let result = (val as u16).wrapping_sub(src as u16);
                self.cpu.update_flags_sub8(val, src, result);
                self.write_rm8(ea, is_mem, rm, result as u8);
            }
            // SUB r/m16, r16
            0x29 => {
                let (reg, val, ea, rm, is_mem) = self.read_rm16();
                let src = self.cpu.get_reg16(reg);
                let result = (val as u32).wrapping_sub(src as u32);
                self.cpu.update_flags_sub16(val, src, result);
                self.write_rm16(ea, is_mem, rm, result as u16);
            }
            // SUB r8, r/m8
            0x2A => {
                let (reg, val, _, _, _) = self.read_rm8();
                let dst = self.cpu.get_reg8(reg);
                let result = (dst as u16).wrapping_sub(val as u16);
                self.cpu.update_flags_sub8(dst, val, result);
                self.cpu.set_reg8(reg, result as u8);
            }
            // SUB r16, r/m16
            0x2B => {
                let (reg, val, _, _, _) = self.read_rm16();
                let dst = self.cpu.get_reg16(reg);
                let result = (dst as u32).wrapping_sub(val as u32);
                self.cpu.update_flags_sub16(dst, val, result);
                self.cpu.set_reg16(reg, result as u16);
            }
            // SUB AL, imm8
            0x2C => {
                let imm = self.fetch_u8();
                let al = self.cpu.ax as u8;
                let result = (al as u16).wrapping_sub(imm as u16);
                self.cpu.update_flags_sub8(al, imm, result);
                self.cpu.ax = (self.cpu.ax & 0xFF00) | (result as u16 & 0xFF);
            }
            // SUB AX, imm16
            0x2D => {
                let imm = self.fetch_u16();
                let ax = self.cpu.ax;
                let result = (ax as u32).wrapping_sub(imm as u32);
                self.cpu.update_flags_sub16(ax, imm, result);
                self.cpu.ax = result as u16;
            }

            // DAS
            0x2F => self.exec_das(),

            // XOR r/m8, r8
            0x30 => {
                let (reg, val, ea, rm, is_mem) = self.read_rm8();
                let src = self.cpu.get_reg8(reg);
                let result = val ^ src;
                self.cpu.update_flags_logic8(result);
                self.write_rm8(ea, is_mem, rm, result);
            }
            // XOR r/m16, r16
            0x31 => {
                let (reg, val, ea, rm, is_mem) = self.read_rm16();
                let src = self.cpu.get_reg16(reg);
                let result = val ^ src;
                self.cpu.update_flags_logic16(result);
                self.write_rm16(ea, is_mem, rm, result);
            }
            // XOR r8, r/m8
            0x32 => {
                let (reg, val, _, _, _) = self.read_rm8();
                let dst = self.cpu.get_reg8(reg);
                let result = dst ^ val;
                self.cpu.update_flags_logic8(result);
                self.cpu.set_reg8(reg, result);
            }
            // XOR r16, r/m16
            0x33 => {
                let (reg, val, _, _, _) = self.read_rm16();
                let dst = self.cpu.get_reg16(reg);
                let result = dst ^ val;
                self.cpu.update_flags_logic16(result);
                self.cpu.set_reg16(reg, result);
            }
            // XOR AL, imm8
            0x34 => {
                let imm = self.fetch_u8();
                let result = (self.cpu.ax as u8) ^ imm;
                self.cpu.update_flags_logic8(result);
                self.cpu.ax = (self.cpu.ax & 0xFF00) | result as u16;
            }
            // XOR AX, imm16
            0x35 => {
                let imm = self.fetch_u16();
                let result = self.cpu.ax ^ imm;
                self.cpu.update_flags_logic16(result);
                self.cpu.ax = result;
            }

            // AAA
            0x37 => self.exec_aaa(),

            // CMP r/m8, r8
            0x38 => {
                let (reg, val, _, _, _) = self.read_rm8();
                let src = self.cpu.get_reg8(reg);
                let result = (val as u16).wrapping_sub(src as u16);
                self.cpu.update_flags_sub8(val, src, result);
            }
            // CMP r/m16, r16
            0x39 => {
                let (reg, val, _, _, _) = self.read_rm16();
                let src = self.cpu.get_reg16(reg);
                let result = (val as u32).wrapping_sub(src as u32);
                self.cpu.update_flags_sub16(val, src, result);
            }
            // CMP r8, r/m8
            0x3A => {
                let (reg, val, _, _, _) = self.read_rm8();
                let dst = self.cpu.get_reg8(reg);
                let result = (dst as u16).wrapping_sub(val as u16);
                self.cpu.update_flags_sub8(dst, val, result);
            }
            // CMP r16, r/m16
            0x3B => {
                let (reg, val, _, _, _) = self.read_rm16();
                let dst = self.cpu.get_reg16(reg);
                let result = (dst as u32).wrapping_sub(val as u32);
                self.cpu.update_flags_sub16(dst, val, result);
            }
            // CMP AL, imm8
            0x3C => {
                let imm = self.fetch_u8();
                let al = self.cpu.ax as u8;
                let result = (al as u16).wrapping_sub(imm as u16);
                self.cpu.update_flags_sub8(al, imm, result);
            }
            // CMP AX, imm16
            0x3D => {
                let imm = self.fetch_u16();
                let ax = self.cpu.ax;
                let result = (ax as u32).wrapping_sub(imm as u32);
                self.cpu.update_flags_sub16(ax, imm, result);
            }

            // AAS
            0x3F => self.exec_aas(),

            // INC r16
            0x40..=0x47 => {
                let reg = opcode - 0x40;
                let val = self.cpu.get_reg16(reg);
                let cf = self.cpu.get_flag(FLAG_CF);
                let result = (val as u32).wrapping_add(1);
                self.cpu.update_flags_add16(val, 1, result);
                self.cpu.set_flag(FLAG_CF, cf); // INC doesn't affect CF
                self.cpu.set_reg16(reg, result as u16);
            }

            // DEC r16
            0x48..=0x4F => {
                let reg = opcode - 0x48;
                let val = self.cpu.get_reg16(reg);
                let cf = self.cpu.get_flag(FLAG_CF);
                let result = (val as u32).wrapping_sub(1);
                self.cpu.update_flags_sub16(val, 1, result);
                self.cpu.set_flag(FLAG_CF, cf); // DEC doesn't affect CF
                self.cpu.set_reg16(reg, result as u16);
            }

            // PUSH r16
            0x50..=0x57 => {
                let reg = opcode - 0x50;
                let val = self.cpu.get_reg16(reg);
                self.push16(val);
            }

            // POP r16
            0x58..=0x5F => {
                let reg = opcode - 0x58;
                let val = self.pop16();
                self.cpu.set_reg16(reg, val);
            }

            // PUSHA (80186+)
            0x60 => {
                let sp = self.cpu.sp;
                self.push16(self.cpu.ax);
                self.push16(self.cpu.cx);
                self.push16(self.cpu.dx);
                self.push16(self.cpu.bx);
                self.push16(sp);
                self.push16(self.cpu.bp);
                self.push16(self.cpu.si);
                self.push16(self.cpu.di);
            }

            // POPA (80186+)
            0x61 => {
                self.cpu.di = self.pop16();
                self.cpu.si = self.pop16();
                self.cpu.bp = self.pop16();
                let _ = self.pop16(); // Skip SP
                self.cpu.bx = self.pop16();
                self.cpu.dx = self.pop16();
                self.cpu.cx = self.pop16();
                self.cpu.ax = self.pop16();
            }

            // JO rel8
            0x70 => {
                let rel = self.fetch_i8();
                if self.cpu.get_flag(FLAG_OF) {
                    self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16);
                }
            }
            // JNO rel8
            0x71 => {
                let rel = self.fetch_i8();
                if !self.cpu.get_flag(FLAG_OF) {
                    self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16);
                }
            }
            // JB/JC/JNAE rel8
            0x72 => {
                let rel = self.fetch_i8();
                if self.cpu.get_flag(FLAG_CF) {
                    self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16);
                }
            }
            // JNB/JNC/JAE rel8
            0x73 => {
                let rel = self.fetch_i8();
                if !self.cpu.get_flag(FLAG_CF) {
                    self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16);
                }
            }
            // JE/JZ rel8
            0x74 => {
                let rel = self.fetch_i8();
                if self.cpu.get_flag(FLAG_ZF) {
                    self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16);
                }
            }
            // JNE/JNZ rel8
            0x75 => {
                let rel = self.fetch_i8();
                if !self.cpu.get_flag(FLAG_ZF) {
                    self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16);
                }
            }
            // JBE/JNA rel8
            0x76 => {
                let rel = self.fetch_i8();
                if self.cpu.get_flag(FLAG_CF) || self.cpu.get_flag(FLAG_ZF) {
                    self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16);
                }
            }
            // JNBE/JA rel8
            0x77 => {
                let rel = self.fetch_i8();
                if !self.cpu.get_flag(FLAG_CF) && !self.cpu.get_flag(FLAG_ZF) {
                    self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16);
                }
            }
            // JS rel8
            0x78 => {
                let rel = self.fetch_i8();
                if self.cpu.get_flag(FLAG_SF) {
                    self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16);
                }
            }
            // JNS rel8
            0x79 => {
                let rel = self.fetch_i8();
                if !self.cpu.get_flag(FLAG_SF) {
                    self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16);
                }
            }
            // JP/JPE rel8
            0x7A => {
                let rel = self.fetch_i8();
                if self.cpu.get_flag(FLAG_PF) {
                    self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16);
                }
            }
            // JNP/JPO rel8
            0x7B => {
                let rel = self.fetch_i8();
                if !self.cpu.get_flag(FLAG_PF) {
                    self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16);
                }
            }
            // JL/JNGE rel8
            0x7C => {
                let rel = self.fetch_i8();
                if self.cpu.get_flag(FLAG_SF) != self.cpu.get_flag(FLAG_OF) {
                    self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16);
                }
            }
            // JNL/JGE rel8
            0x7D => {
                let rel = self.fetch_i8();
                if self.cpu.get_flag(FLAG_SF) == self.cpu.get_flag(FLAG_OF) {
                    self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16);
                }
            }
            // JLE/JNG rel8
            0x7E => {
                let rel = self.fetch_i8();
                if self.cpu.get_flag(FLAG_ZF) || (self.cpu.get_flag(FLAG_SF) != self.cpu.get_flag(FLAG_OF)) {
                    self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16);
                }
            }
            // JNLE/JG rel8
            0x7F => {
                let rel = self.fetch_i8();
                if !self.cpu.get_flag(FLAG_ZF) && (self.cpu.get_flag(FLAG_SF) == self.cpu.get_flag(FLAG_OF)) {
                    self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16);
                }
            }

            // Group 1 r/m8, imm8
            0x80 => self.exec_group1_8(false),
            // Group 1 r/m16, imm16
            0x81 => self.exec_group1_16(false),
            // Group 1 r/m8, imm8 (same as 0x80)
            0x82 => self.exec_group1_8(false),
            // Group 1 r/m16, imm8 (sign-extended)
            0x83 => self.exec_group1_16(true),

            // TEST r/m8, r8
            0x84 => {
                let (reg, val, _, _, _) = self.read_rm8();
                let src = self.cpu.get_reg8(reg);
                let result = val & src;
                self.cpu.update_flags_logic8(result);
            }
            // TEST r/m16, r16
            0x85 => {
                let (reg, val, _, _, _) = self.read_rm16();
                let src = self.cpu.get_reg16(reg);
                let result = val & src;
                self.cpu.update_flags_logic16(result);
            }

            // XCHG r/m8, r8
            0x86 => {
                let (reg, val, ea, rm, is_mem) = self.read_rm8();
                let src = self.cpu.get_reg8(reg);
                self.cpu.set_reg8(reg, val);
                self.write_rm8(ea, is_mem, rm, src);
            }
            // XCHG r/m16, r16
            0x87 => {
                let (reg, val, ea, rm, is_mem) = self.read_rm16();
                let src = self.cpu.get_reg16(reg);
                self.cpu.set_reg16(reg, val);
                self.write_rm16(ea, is_mem, rm, src);
            }

            // MOV r/m8, r8
            0x88 => {
                let (reg, _, ea, rm, is_mem) = self.read_rm8();
                let val = self.cpu.get_reg8(reg);
                self.write_rm8(ea, is_mem, rm, val);
            }
            // MOV r/m16, r16
            0x89 => {
                let (reg, _, ea, rm, is_mem) = self.read_rm16();
                let val = self.cpu.get_reg16(reg);
                self.write_rm16(ea, is_mem, rm, val);
            }
            // MOV r8, r/m8
            0x8A => {
                let (reg, val, _, _, _) = self.read_rm8();
                self.cpu.set_reg8(reg, val);
            }
            // MOV r16, r/m16
            0x8B => {
                let (reg, val, _, _, _) = self.read_rm16();
                self.cpu.set_reg16(reg, val);
            }
            // MOV r/m16, Sreg
            0x8C => {
                let modrm = self.fetch_u8();
                let mode = (modrm >> 6) & 3;
                let reg = (modrm >> 3) & 7;
                let rm = modrm & 7;
                let val = self.cpu.get_seg(reg);
                if mode == 3 {
                    self.cpu.set_reg16(rm, val);
                } else {
                    // Rewind and use full decode
                    self.cpu.ip = self.cpu.ip.wrapping_sub(1);
                    let (_, _, ea, rm, is_mem) = self.read_rm16();
                    self.write_rm16(ea, is_mem, rm, val);
                }
            }
            // LEA r16, m
            0x8D => {
                let (reg, ea, _, _) = self.decode_modrm(true);
                self.cpu.set_reg16(reg, ea);
            }
            // MOV Sreg, r/m16
            0x8E => {
                let (reg, val, _, _, _) = self.read_rm16();
                self.cpu.set_seg(reg, val);
            }
            // POP r/m16
            0x8F => {
                let (_, _, ea, rm, is_mem) = self.read_rm16();
                let val = self.pop16();
                self.write_rm16(ea, is_mem, rm, val);
            }

            // NOP (XCHG AX, AX)
            0x90 => {}

            // XCHG AX, r16
            0x91..=0x97 => {
                let reg = opcode - 0x90;
                let ax = self.cpu.ax;
                let other = self.cpu.get_reg16(reg);
                self.cpu.ax = other;
                self.cpu.set_reg16(reg, ax);
            }

            // CBW
            0x98 => {
                let al = self.cpu.ax as u8;
                self.cpu.ax = (al as i8 as i16) as u16;
            }
            // CWD
            0x99 => {
                if (self.cpu.ax & 0x8000) != 0 {
                    self.cpu.dx = 0xFFFF;
                } else {
                    self.cpu.dx = 0;
                }
            }

            // CALL far ptr
            0x9A => {
                let off = self.fetch_u16();
                let seg = self.fetch_u16();
                self.push16(self.cpu.cs);
                self.push16(self.cpu.ip);
                self.cpu.cs = seg;
                self.cpu.ip = off;
            }

            // PUSHF
            0x9C => self.push16(self.cpu.flags | 0x0002),
            // POPF
            0x9D => self.cpu.flags = (self.pop16() & 0x0FD5) | 0x0002,
            // SAHF
            0x9E => {
                let ah = (self.cpu.ax >> 8) as u8;
                self.cpu.flags = (self.cpu.flags & 0xFF00) | (ah as u16);
            }
            // LAHF
            0x9F => {
                let flags = (self.cpu.flags & 0xFF) as u8;
                self.cpu.ax = (self.cpu.ax & 0x00FF) | ((flags as u16) << 8);
            }

            // MOV AL, [addr]
            0xA0 => {
                let addr = self.fetch_u16();
                let seg = self.get_seg(self.cpu.ds);
                let val = self.read_u8(seg, addr);
                self.cpu.ax = (self.cpu.ax & 0xFF00) | val as u16;
            }
            // MOV AX, [addr]
            0xA1 => {
                let addr = self.fetch_u16();
                let seg = self.get_seg(self.cpu.ds);
                self.cpu.ax = self.read_u16(seg, addr);
            }
            // MOV [addr], AL
            0xA2 => {
                let addr = self.fetch_u16();
                let seg = self.get_seg(self.cpu.ds);
                self.write_u8(seg, addr, self.cpu.ax as u8);
            }
            // MOV [addr], AX
            0xA3 => {
                let addr = self.fetch_u16();
                let seg = self.get_seg(self.cpu.ds);
                self.write_u16(seg, addr, self.cpu.ax);
            }

            // MOVSB
            0xA4 => self.exec_movsb(),
            // MOVSW
            0xA5 => self.exec_movsw(),
            // CMPSB
            0xA6 => self.exec_cmpsb(),
            // CMPSW
            0xA7 => self.exec_cmpsw(),

            // TEST AL, imm8
            0xA8 => {
                let imm = self.fetch_u8();
                let result = (self.cpu.ax as u8) & imm;
                self.cpu.update_flags_logic8(result);
            }
            // TEST AX, imm16
            0xA9 => {
                let imm = self.fetch_u16();
                let result = self.cpu.ax & imm;
                self.cpu.update_flags_logic16(result);
            }

            // STOSB
            0xAA => self.exec_stosb(),
            // STOSW
            0xAB => self.exec_stosw(),
            // LODSB
            0xAC => self.exec_lodsb(),
            // LODSW
            0xAD => self.exec_lodsw(),
            // SCASB
            0xAE => self.exec_scasb(),
            // SCASW
            0xAF => self.exec_scasw(),

            // MOV r8, imm8
            0xB0..=0xB7 => {
                let reg = opcode - 0xB0;
                let imm = self.fetch_u8();
                self.cpu.set_reg8(reg, imm);
            }
            // MOV r16, imm16
            0xB8..=0xBF => {
                let reg = opcode - 0xB8;
                let imm = self.fetch_u16();
                self.cpu.set_reg16(reg, imm);
            }

            // RET imm16 (near)
            0xC2 => {
                let bytes = self.fetch_u16();
                self.cpu.ip = self.pop16();
                self.cpu.sp = self.cpu.sp.wrapping_add(bytes);
            }
            // RET (near)
            0xC3 => {
                self.cpu.ip = self.pop16();
            }

            // LES r16, m16:16
            0xC4 => {
                let (reg, ea, _, _) = self.decode_modrm(true);
                let seg = self.get_seg(self.cpu.ds);
                let off = self.read_u16(seg, ea);
                let new_seg = self.read_u16(seg, ea.wrapping_add(2));
                self.cpu.set_reg16(reg, off);
                self.cpu.es = new_seg;
            }
            // LDS r16, m16:16
            0xC5 => {
                let (reg, ea, _, _) = self.decode_modrm(true);
                let seg = self.get_seg(self.cpu.ds);
                let off = self.read_u16(seg, ea);
                let new_seg = self.read_u16(seg, ea.wrapping_add(2));
                self.cpu.set_reg16(reg, off);
                self.cpu.ds = new_seg;
            }

            // MOV r/m8, imm8
            0xC6 => {
                let (_, _, ea, rm, is_mem) = self.read_rm8();
                let imm = self.fetch_u8();
                self.write_rm8(ea, is_mem, rm, imm);
            }
            // MOV r/m16, imm16
            0xC7 => {
                let (_, _, ea, rm, is_mem) = self.read_rm16();
                let imm = self.fetch_u16();
                self.write_rm16(ea, is_mem, rm, imm);
            }

            // RETF imm16
            0xCA => {
                let bytes = self.fetch_u16();
                self.cpu.ip = self.pop16();
                self.cpu.cs = self.pop16();
                self.cpu.sp = self.cpu.sp.wrapping_add(bytes);
            }
            // RETF
            0xCB => {
                self.cpu.ip = self.pop16();
                self.cpu.cs = self.pop16();
                // Debug: log RETF destination
                serial_write_bytes(b"  RETF -> ");
                serial_write_hex16(self.cpu.cs);
                serial_write_bytes(b":");
                serial_write_hex16(self.cpu.ip);
                serial_write_bytes(b"\r\n");
            }

            // INT 3
            0xCC => self.handle_int(3),
            // INT imm8
            0xCD => {
                let int_no = self.fetch_u8();
                self.handle_int(int_no);
            }
            // INTO
            0xCE => {
                if self.cpu.get_flag(FLAG_OF) {
                    self.handle_int(4);
                }
            }
            // IRET
            0xCF => {
                self.cpu.ip = self.pop16();
                self.cpu.cs = self.pop16();
                self.cpu.flags = self.pop16();
            }

            // Group 2 r/m8, 1
            0xD0 => self.exec_group2_8(1),
            // Group 2 r/m16, 1
            0xD1 => self.exec_group2_16(1),
            // Group 2 r/m8, CL
            0xD2 => {
                let count = self.cpu.cx as u8;
                self.exec_group2_8(count);
            }
            // Group 2 r/m16, CL
            0xD3 => {
                let count = self.cpu.cx as u8;
                self.exec_group2_16(count);
            }

            // AAM
            0xD4 => {
                let base = self.fetch_u8();
                if base != 0 {
                    let al = self.cpu.ax as u8;
                    let ah = al / base;
                    let new_al = al % base;
                    self.cpu.ax = ((ah as u16) << 8) | (new_al as u16);
                    self.cpu.update_flags_logic8(new_al);
                }
            }
            // AAD
            0xD5 => {
                let base = self.fetch_u8();
                let al = self.cpu.ax as u8;
                let ah = (self.cpu.ax >> 8) as u8;
                let result = (al as u16).wrapping_add((ah as u16).wrapping_mul(base as u16));
                self.cpu.ax = result & 0xFF;
                self.cpu.update_flags_logic8(result as u8);
            }

            // XLAT
            0xD7 => {
                let seg = self.get_seg(self.cpu.ds);
                let addr = self.cpu.bx.wrapping_add(self.cpu.ax as u8 as u16);
                let val = self.read_u8(seg, addr);
                self.cpu.ax = (self.cpu.ax & 0xFF00) | val as u16;
            }

            // LOOPNE/LOOPNZ
            0xE0 => {
                let rel = self.fetch_i8();
                self.cpu.cx = self.cpu.cx.wrapping_sub(1);
                if self.cpu.cx != 0 && !self.cpu.get_flag(FLAG_ZF) {
                    self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16);
                }
            }
            // LOOPE/LOOPZ
            0xE1 => {
                let rel = self.fetch_i8();
                self.cpu.cx = self.cpu.cx.wrapping_sub(1);
                if self.cpu.cx != 0 && self.cpu.get_flag(FLAG_ZF) {
                    self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16);
                }
            }
            // LOOP
            0xE2 => {
                let rel = self.fetch_i8();
                self.cpu.cx = self.cpu.cx.wrapping_sub(1);
                if self.cpu.cx != 0 {
                    self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16);
                }
            }
            // JCXZ
            0xE3 => {
                let rel = self.fetch_i8();
                if self.cpu.cx == 0 {
                    self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16);
                }
            }

            // IN AL, imm8
            0xE4 => {
                let port = self.fetch_u8() as u16;
                self.cpu.ax = (self.cpu.ax & 0xFF00) | self.port_in8(port) as u16;
            }
            // IN AX, imm16
            0xE5 => {
                let port = self.fetch_u8() as u16;
                self.cpu.ax = self.port_in16(port);
            }
            // OUT imm8, AL
            0xE6 => {
                let port = self.fetch_u8() as u16;
                self.port_out8(port, self.cpu.ax as u8);
            }
            // OUT imm8, AX
            0xE7 => {
                let port = self.fetch_u8() as u16;
                self.port_out16(port, self.cpu.ax);
            }

            // CALL rel16
            0xE8 => {
                let rel = self.fetch_u16();
                self.push16(self.cpu.ip);
                self.cpu.ip = self.cpu.ip.wrapping_add(rel);
            }
            // JMP rel16
            0xE9 => {
                let ip_before = self.cpu.ip;
                let rel = self.fetch_u16();
                let new_ip = self.cpu.ip.wrapping_add(rel);
                serial_write_bytes(b"  JMP rel16: IP=");
                serial_write_hex16(ip_before);
                serial_write_bytes(b" rel=");
                serial_write_hex16(rel);
                serial_write_bytes(b" -> ");
                serial_write_hex16(new_ip);
                serial_write_bytes(b"\r\n");
                self.cpu.ip = new_ip;
            }
            // JMP far ptr
            0xEA => {
                let off = self.fetch_u16();
                let seg = self.fetch_u16();
                self.cpu.ip = off;
                self.cpu.cs = seg;
            }
            // JMP rel8
            0xEB => {
                let rel = self.fetch_i8();
                self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16);
            }

            // IN AL, DX
            0xEC => {
                self.cpu.ax = (self.cpu.ax & 0xFF00) | self.port_in8(self.cpu.dx) as u16;
            }
            // IN AX, DX
            0xED => {
                self.cpu.ax = self.port_in16(self.cpu.dx);
            }
            // OUT DX, AL
            0xEE => {
                self.port_out8(self.cpu.dx, self.cpu.ax as u8);
            }
            // OUT DX, AX
            0xEF => {
                self.port_out16(self.cpu.dx, self.cpu.ax);
            }

            // LOCK prefix - preserve segment override state
            0xF0 => { self.step_inner(has_seg_prefix); }

            // REPNE/REPNZ
            0xF2 => self.exec_rep(false),
            // REP/REPE/REPZ
            0xF3 => self.exec_rep(true),

            // HLT
            0xF4 => {
                // For now, just pause
            }

            // CMC
            0xF5 => {
                let cf = self.cpu.get_flag(FLAG_CF);
                self.cpu.set_flag(FLAG_CF, !cf);
            }

            // Group 3 r/m8
            0xF6 => self.exec_group3_8(),
            // Group 3 r/m16
            0xF7 => self.exec_group3_16(),

            // CLC
            0xF8 => self.cpu.set_flag(FLAG_CF, false),
            // STC
            0xF9 => self.cpu.set_flag(FLAG_CF, true),
            // CLI
            0xFA => self.cpu.set_flag(FLAG_IF, false),
            // STI
            0xFB => self.cpu.set_flag(FLAG_IF, true),
            // CLD
            0xFC => self.cpu.set_flag(FLAG_DF, false),
            // STD
            0xFD => self.cpu.set_flag(FLAG_DF, true),

            // Group 4 r/m8 (INC/DEC)
            0xFE => self.exec_group4(),
            // Group 5 r/m16
            0xFF => self.exec_group5(),

            // Unhandled opcode
            _ => {
                // Log unhandled opcode and terminate
                serial_write_bytes(b"\r\n!!! UNKNOWN OPCODE ");
                serial_write_hex8(opcode);
                serial_write_bytes(b"h at ");
                serial_write_hex16(trace_cs);
                serial_write_bytes(b":");
                serial_write_hex16(trace_ip);
                serial_write_bytes(b" - HALTING\r\n");
                serial_write_bytes(b"CPU: AX=");
                serial_write_hex16(self.cpu.ax);
                serial_write_bytes(b" BX=");
                serial_write_hex16(self.cpu.bx);
                serial_write_bytes(b" CX=");
                serial_write_hex16(self.cpu.cx);
                serial_write_bytes(b" DX=");
                serial_write_hex16(self.cpu.dx);
                serial_write_bytes(b" SP=");
                serial_write_hex16(self.cpu.sp);
                serial_write_bytes(b" BP=");
                serial_write_hex16(self.cpu.bp);
                serial_write_bytes(b" SI=");
                serial_write_hex16(self.cpu.si);
                serial_write_bytes(b" DI=");
                serial_write_hex16(self.cpu.di);
                serial_write_bytes(b"\r\n");
                serial_write_bytes(b"     CS=");
                serial_write_hex16(self.cpu.cs);
                serial_write_bytes(b" DS=");
                serial_write_hex16(self.cpu.ds);
                serial_write_bytes(b" ES=");
                serial_write_hex16(self.cpu.es);
                serial_write_bytes(b" SS=");
                serial_write_hex16(self.cpu.ss);
                serial_write_bytes(b" FLAGS=");
                serial_write_hex16(self.cpu.flags);
                serial_write_bytes(b"\r\n");
                self.state = TaskState::Terminated;
            }
        }
    }

    // Helper methods for instruction groups and complex operations
    fn exec_adc(&mut self, opcode: u8) {
        let carry = if self.cpu.get_flag(FLAG_CF) { 1 } else { 0 };
        match opcode {
            0x10 => {
                let (reg, val, ea, rm, is_mem) = self.read_rm8();
                let src = self.cpu.get_reg8(reg);
                let result = (val as u16).wrapping_add(src as u16).wrapping_add(carry);
                self.cpu.update_flags_add8(val, src.wrapping_add(carry as u8), result);
                self.write_rm8(ea, is_mem, rm, result as u8);
            }
            0x11 => {
                let (reg, val, ea, rm, is_mem) = self.read_rm16();
                let src = self.cpu.get_reg16(reg);
                let result = (val as u32).wrapping_add(src as u32).wrapping_add(carry as u32);
                self.cpu.update_flags_add16(val, src.wrapping_add(carry), result);
                self.write_rm16(ea, is_mem, rm, result as u16);
            }
            0x12 => {
                let (reg, val, _, _, _) = self.read_rm8();
                let dst = self.cpu.get_reg8(reg);
                let result = (dst as u16).wrapping_add(val as u16).wrapping_add(carry);
                self.cpu.update_flags_add8(dst, val.wrapping_add(carry as u8), result);
                self.cpu.set_reg8(reg, result as u8);
            }
            0x13 => {
                let (reg, val, _, _, _) = self.read_rm16();
                let dst = self.cpu.get_reg16(reg);
                let result = (dst as u32).wrapping_add(val as u32).wrapping_add(carry as u32);
                self.cpu.update_flags_add16(dst, val.wrapping_add(carry), result);
                self.cpu.set_reg16(reg, result as u16);
            }
            0x14 => {
                let imm = self.fetch_u8();
                let al = self.cpu.ax as u8;
                let result = (al as u16).wrapping_add(imm as u16).wrapping_add(carry);
                self.cpu.update_flags_add8(al, imm.wrapping_add(carry as u8), result);
                self.cpu.ax = (self.cpu.ax & 0xFF00) | (result as u16 & 0xFF);
            }
            0x15 => {
                let imm = self.fetch_u16();
                let ax = self.cpu.ax;
                let result = (ax as u32).wrapping_add(imm as u32).wrapping_add(carry as u32);
                self.cpu.update_flags_add16(ax, imm.wrapping_add(carry), result);
                self.cpu.ax = result as u16;
            }
            _ => {}
        }
    }

    fn exec_sbb(&mut self, opcode: u8) {
        let borrow = if self.cpu.get_flag(FLAG_CF) { 1 } else { 0 };
        match opcode {
            0x18 => {
                let (reg, val, ea, rm, is_mem) = self.read_rm8();
                let src = self.cpu.get_reg8(reg).wrapping_add(borrow as u8);
                let result = (val as u16).wrapping_sub(src as u16);
                self.cpu.update_flags_sub8(val, src, result);
                self.write_rm8(ea, is_mem, rm, result as u8);
            }
            0x19 => {
                let (reg, val, ea, rm, is_mem) = self.read_rm16();
                let src = self.cpu.get_reg16(reg).wrapping_add(borrow);
                let result = (val as u32).wrapping_sub(src as u32);
                self.cpu.update_flags_sub16(val, src, result);
                self.write_rm16(ea, is_mem, rm, result as u16);
            }
            0x1A => {
                let (reg, val, _, _, _) = self.read_rm8();
                let dst = self.cpu.get_reg8(reg);
                let src = val.wrapping_add(borrow as u8);
                let result = (dst as u16).wrapping_sub(src as u16);
                self.cpu.update_flags_sub8(dst, src, result);
                self.cpu.set_reg8(reg, result as u8);
            }
            0x1B => {
                let (reg, val, _, _, _) = self.read_rm16();
                let dst = self.cpu.get_reg16(reg);
                let src = val.wrapping_add(borrow);
                let result = (dst as u32).wrapping_sub(src as u32);
                self.cpu.update_flags_sub16(dst, src, result);
                self.cpu.set_reg16(reg, result as u16);
            }
            0x1C => {
                let imm = self.fetch_u8().wrapping_add(borrow as u8);
                let al = self.cpu.ax as u8;
                let result = (al as u16).wrapping_sub(imm as u16);
                self.cpu.update_flags_sub8(al, imm, result);
                self.cpu.ax = (self.cpu.ax & 0xFF00) | (result as u16 & 0xFF);
            }
            0x1D => {
                let imm = self.fetch_u16().wrapping_add(borrow);
                let ax = self.cpu.ax;
                let result = (ax as u32).wrapping_sub(imm as u32);
                self.cpu.update_flags_sub16(ax, imm, result);
                self.cpu.ax = result as u16;
            }
            _ => {}
        }
    }

    fn exec_daa(&mut self) {
        let al = self.cpu.ax as u8;
        let mut new_al = al;
        let mut cf = self.cpu.get_flag(FLAG_CF);

        if (al & 0x0F) > 9 || self.cpu.get_flag(FLAG_AF) {
            new_al = new_al.wrapping_add(6);
            self.cpu.set_flag(FLAG_AF, true);
        } else {
            self.cpu.set_flag(FLAG_AF, false);
        }

        if al > 0x99 || cf {
            new_al = new_al.wrapping_add(0x60);
            cf = true;
        }

        self.cpu.ax = (self.cpu.ax & 0xFF00) | new_al as u16;
        self.cpu.set_flag(FLAG_CF, cf);
        self.cpu.update_flags_logic8(new_al);
    }

    fn exec_das(&mut self) {
        let al = self.cpu.ax as u8;
        let mut new_al = al;
        let mut cf = self.cpu.get_flag(FLAG_CF);

        if (al & 0x0F) > 9 || self.cpu.get_flag(FLAG_AF) {
            new_al = new_al.wrapping_sub(6);
            self.cpu.set_flag(FLAG_AF, true);
        } else {
            self.cpu.set_flag(FLAG_AF, false);
        }

        if al > 0x99 || cf {
            new_al = new_al.wrapping_sub(0x60);
            cf = true;
        }

        self.cpu.ax = (self.cpu.ax & 0xFF00) | new_al as u16;
        self.cpu.set_flag(FLAG_CF, cf);
        self.cpu.update_flags_logic8(new_al);
    }

    fn exec_aaa(&mut self) {
        let al = self.cpu.ax as u8;
        let ah = (self.cpu.ax >> 8) as u8;

        if (al & 0x0F) > 9 || self.cpu.get_flag(FLAG_AF) {
            let new_al = al.wrapping_add(6) & 0x0F;
            let new_ah = ah.wrapping_add(1);
            self.cpu.ax = ((new_ah as u16) << 8) | new_al as u16;
            self.cpu.set_flag(FLAG_AF, true);
            self.cpu.set_flag(FLAG_CF, true);
        } else {
            self.cpu.ax = (self.cpu.ax & 0xFF0F);
            self.cpu.set_flag(FLAG_AF, false);
            self.cpu.set_flag(FLAG_CF, false);
        }
    }

    fn exec_aas(&mut self) {
        let al = self.cpu.ax as u8;
        let ah = (self.cpu.ax >> 8) as u8;

        if (al & 0x0F) > 9 || self.cpu.get_flag(FLAG_AF) {
            let new_al = al.wrapping_sub(6) & 0x0F;
            let new_ah = ah.wrapping_sub(1);
            self.cpu.ax = ((new_ah as u16) << 8) | new_al as u16;
            self.cpu.set_flag(FLAG_AF, true);
            self.cpu.set_flag(FLAG_CF, true);
        } else {
            self.cpu.ax = (self.cpu.ax & 0xFF0F);
            self.cpu.set_flag(FLAG_AF, false);
            self.cpu.set_flag(FLAG_CF, false);
        }
    }

    fn exec_group1_8(&mut self, _sign_extend: bool) {
        let (op, val, ea, rm, is_mem) = self.read_rm8();
        let imm = self.fetch_u8();

        let result = match op {
            0 => { // ADD
                let r = (val as u16).wrapping_add(imm as u16);
                self.cpu.update_flags_add8(val, imm, r);
                r as u8
            }
            1 => { // OR
                let r = val | imm;
                self.cpu.update_flags_logic8(r);
                r
            }
            2 => { // ADC
                let carry = if self.cpu.get_flag(FLAG_CF) { 1u8 } else { 0 };
                let r = (val as u16).wrapping_add(imm as u16).wrapping_add(carry as u16);
                self.cpu.update_flags_add8(val, imm.wrapping_add(carry), r);
                r as u8
            }
            3 => { // SBB
                let borrow = if self.cpu.get_flag(FLAG_CF) { 1u8 } else { 0 };
                let src = imm.wrapping_add(borrow);
                let r = (val as u16).wrapping_sub(src as u16);
                self.cpu.update_flags_sub8(val, src, r);
                r as u8
            }
            4 => { // AND
                let r = val & imm;
                self.cpu.update_flags_logic8(r);
                r
            }
            5 => { // SUB
                let r = (val as u16).wrapping_sub(imm as u16);
                self.cpu.update_flags_sub8(val, imm, r);
                r as u8
            }
            6 => { // XOR
                let r = val ^ imm;
                self.cpu.update_flags_logic8(r);
                r
            }
            7 => { // CMP
                let r = (val as u16).wrapping_sub(imm as u16);
                self.cpu.update_flags_sub8(val, imm, r);
                return; // Don't write back
            }
            _ => val,
        };

        self.write_rm8(ea, is_mem, rm, result);
    }

    fn exec_group1_16(&mut self, sign_extend: bool) {
        let (op, val, ea, rm, is_mem) = self.read_rm16();
        let imm = if sign_extend {
            self.fetch_i8() as i16 as u16
        } else {
            self.fetch_u16()
        };

        let result = match op {
            0 => { // ADD
                let r = (val as u32).wrapping_add(imm as u32);
                self.cpu.update_flags_add16(val, imm, r);
                r as u16
            }
            1 => { // OR
                let r = val | imm;
                self.cpu.update_flags_logic16(r);
                r
            }
            2 => { // ADC
                let carry = if self.cpu.get_flag(FLAG_CF) { 1u16 } else { 0 };
                let r = (val as u32).wrapping_add(imm as u32).wrapping_add(carry as u32);
                self.cpu.update_flags_add16(val, imm.wrapping_add(carry), r);
                r as u16
            }
            3 => { // SBB
                let borrow = if self.cpu.get_flag(FLAG_CF) { 1u16 } else { 0 };
                let src = imm.wrapping_add(borrow);
                let r = (val as u32).wrapping_sub(src as u32);
                self.cpu.update_flags_sub16(val, src, r);
                r as u16
            }
            4 => { // AND
                let r = val & imm;
                self.cpu.update_flags_logic16(r);
                r
            }
            5 => { // SUB
                let r = (val as u32).wrapping_sub(imm as u32);
                self.cpu.update_flags_sub16(val, imm, r);
                r as u16
            }
            6 => { // XOR
                let r = val ^ imm;
                self.cpu.update_flags_logic16(r);
                r
            }
            7 => { // CMP
                let r = (val as u32).wrapping_sub(imm as u32);
                self.cpu.update_flags_sub16(val, imm, r);
                return; // Don't write back
            }
            _ => val,
        };

        self.write_rm16(ea, is_mem, rm, result);
    }

    fn exec_group2_8(&mut self, count: u8) {
        let (op, val, ea, rm, is_mem) = self.read_rm8();
        let count = count & 0x1F;
        if count == 0 { return; }

        let result = match op {
            0 => { // ROL
                let r = val.rotate_left(count as u32);
                self.cpu.set_flag(FLAG_CF, (r & 1) != 0);
                r
            }
            1 => { // ROR
                let r = val.rotate_right(count as u32);
                self.cpu.set_flag(FLAG_CF, (r & 0x80) != 0);
                r
            }
            2 => { // RCL
                let mut r = val;
                for _ in 0..count {
                    let cf = if self.cpu.get_flag(FLAG_CF) { 1 } else { 0 };
                    self.cpu.set_flag(FLAG_CF, (r & 0x80) != 0);
                    r = (r << 1) | cf;
                }
                r
            }
            3 => { // RCR
                let mut r = val;
                for _ in 0..count {
                    let cf = if self.cpu.get_flag(FLAG_CF) { 0x80 } else { 0 };
                    self.cpu.set_flag(FLAG_CF, (r & 1) != 0);
                    r = (r >> 1) | cf;
                }
                r
            }
            4 | 6 => { // SHL/SAL
                let r = val << count;
                self.cpu.set_flag(FLAG_CF, (val >> (8 - count)) & 1 != 0);
                self.cpu.update_flags_logic8(r);
                r
            }
            5 => { // SHR
                let r = val >> count;
                self.cpu.set_flag(FLAG_CF, (val >> (count - 1)) & 1 != 0);
                self.cpu.update_flags_logic8(r);
                r
            }
            7 => { // SAR
                let r = ((val as i8) >> count) as u8;
                self.cpu.set_flag(FLAG_CF, ((val as i8) >> (count - 1)) & 1 != 0);
                self.cpu.update_flags_logic8(r);
                r
            }
            _ => val,
        };

        self.write_rm8(ea, is_mem, rm, result);
    }

    fn exec_group2_16(&mut self, count: u8) {
        let (op, val, ea, rm, is_mem) = self.read_rm16();
        let count = count & 0x1F;
        if count == 0 { return; }

        let result = match op {
            0 => { // ROL
                let r = val.rotate_left(count as u32);
                self.cpu.set_flag(FLAG_CF, (r & 1) != 0);
                r
            }
            1 => { // ROR
                let r = val.rotate_right(count as u32);
                self.cpu.set_flag(FLAG_CF, (r & 0x8000) != 0);
                r
            }
            2 => { // RCL
                let mut r = val;
                for _ in 0..count {
                    let cf = if self.cpu.get_flag(FLAG_CF) { 1 } else { 0 };
                    self.cpu.set_flag(FLAG_CF, (r & 0x8000) != 0);
                    r = (r << 1) | cf;
                }
                r
            }
            3 => { // RCR
                let mut r = val;
                for _ in 0..count {
                    let cf = if self.cpu.get_flag(FLAG_CF) { 0x8000 } else { 0 };
                    self.cpu.set_flag(FLAG_CF, (r & 1) != 0);
                    r = (r >> 1) | cf;
                }
                r
            }
            4 | 6 => { // SHL/SAL
                let r = val << count;
                self.cpu.set_flag(FLAG_CF, (val >> (16 - count)) & 1 != 0);
                self.cpu.update_flags_logic16(r);
                r
            }
            5 => { // SHR
                let r = val >> count;
                self.cpu.set_flag(FLAG_CF, (val >> (count - 1)) & 1 != 0);
                self.cpu.update_flags_logic16(r);
                r
            }
            7 => { // SAR
                let r = ((val as i16) >> count) as u16;
                self.cpu.set_flag(FLAG_CF, ((val as i16) >> (count - 1)) & 1 != 0);
                self.cpu.update_flags_logic16(r);
                r
            }
            _ => val,
        };

        self.write_rm16(ea, is_mem, rm, result);
    }

    fn exec_group3_8(&mut self) {
        let (op, val, ea, rm, is_mem) = self.read_rm8();

        match op {
            0 | 1 => { // TEST r/m8, imm8
                let imm = self.fetch_u8();
                let result = val & imm;
                self.cpu.update_flags_logic8(result);
            }
            2 => { // NOT
                self.write_rm8(ea, is_mem, rm, !val);
            }
            3 => { // NEG
                let result = (0u16).wrapping_sub(val as u16);
                self.cpu.update_flags_sub8(0, val, result);
                self.write_rm8(ea, is_mem, rm, result as u8);
            }
            4 => { // MUL
                let result = (self.cpu.ax as u8 as u16) * (val as u16);
                self.cpu.ax = result;
                let of = (result & 0xFF00) != 0;
                self.cpu.set_flag(FLAG_CF, of);
                self.cpu.set_flag(FLAG_OF, of);
            }
            5 => { // IMUL
                let result = (self.cpu.ax as u8 as i8 as i16) * (val as i8 as i16);
                self.cpu.ax = result as u16;
                let of = result < -128 || result > 127;
                self.cpu.set_flag(FLAG_CF, of);
                self.cpu.set_flag(FLAG_OF, of);
            }
            6 => { // DIV
                if val == 0 {
                    self.handle_int(0); // Division by zero
                } else {
                    let dividend = self.cpu.ax;
                    let quotient = dividend / (val as u16);
                    let remainder = dividend % (val as u16);
                    if quotient > 0xFF {
                        self.handle_int(0);
                    } else {
                        self.cpu.ax = ((remainder as u16) << 8) | (quotient as u16 & 0xFF);
                    }
                }
            }
            7 => { // IDIV
                if val == 0 {
                    self.handle_int(0);
                } else {
                    let dividend = self.cpu.ax as i16;
                    let divisor = val as i8 as i16;
                    let quotient = dividend / divisor;
                    let remainder = dividend % divisor;
                    if quotient < -128 || quotient > 127 {
                        self.handle_int(0);
                    } else {
                        self.cpu.ax = ((remainder as u8 as u16) << 8) | (quotient as u8 as u16);
                    }
                }
            }
            _ => {}
        }
    }

    fn exec_group3_16(&mut self) {
        let (op, val, ea, rm, is_mem) = self.read_rm16();

        match op {
            0 | 1 => { // TEST r/m16, imm16
                let imm = self.fetch_u16();
                let result = val & imm;
                self.cpu.update_flags_logic16(result);
            }
            2 => { // NOT
                self.write_rm16(ea, is_mem, rm, !val);
            }
            3 => { // NEG
                let result = (0u32).wrapping_sub(val as u32);
                self.cpu.update_flags_sub16(0, val, result);
                self.write_rm16(ea, is_mem, rm, result as u16);
            }
            4 => { // MUL
                let result = (self.cpu.ax as u32) * (val as u32);
                self.cpu.ax = result as u16;
                self.cpu.dx = (result >> 16) as u16;
                let of = self.cpu.dx != 0;
                self.cpu.set_flag(FLAG_CF, of);
                self.cpu.set_flag(FLAG_OF, of);
            }
            5 => { // IMUL
                let result = (self.cpu.ax as i16 as i32) * (val as i16 as i32);
                self.cpu.ax = result as u16;
                self.cpu.dx = (result >> 16) as u16;
                let of = result < -32768 || result > 32767;
                self.cpu.set_flag(FLAG_CF, of);
                self.cpu.set_flag(FLAG_OF, of);
            }
            6 => { // DIV
                if val == 0 {
                    self.handle_int(0);
                } else {
                    let dividend = ((self.cpu.dx as u32) << 16) | (self.cpu.ax as u32);
                    let quotient = dividend / (val as u32);
                    let remainder = dividend % (val as u32);
                    if quotient > 0xFFFF {
                        self.handle_int(0);
                    } else {
                        self.cpu.ax = quotient as u16;
                        self.cpu.dx = remainder as u16;
                    }
                }
            }
            7 => { // IDIV
                if val == 0 {
                    self.handle_int(0);
                } else {
                    let dividend = (((self.cpu.dx as u32) << 16) | (self.cpu.ax as u32)) as i32;
                    let divisor = val as i16 as i32;
                    let quotient = dividend / divisor;
                    let remainder = dividend % divisor;
                    if quotient < -32768 || quotient > 32767 {
                        self.handle_int(0);
                    } else {
                        self.cpu.ax = quotient as u16;
                        self.cpu.dx = remainder as u16;
                    }
                }
            }
            _ => {}
        }
    }

    fn exec_group4(&mut self) {
        let (op, val, ea, rm, is_mem) = self.read_rm8();
        let cf = self.cpu.get_flag(FLAG_CF);

        match op {
            0 => { // INC
                let result = (val as u16).wrapping_add(1);
                self.cpu.update_flags_add8(val, 1, result);
                self.cpu.set_flag(FLAG_CF, cf);
                self.write_rm8(ea, is_mem, rm, result as u8);
            }
            1 => { // DEC
                let result = (val as u16).wrapping_sub(1);
                self.cpu.update_flags_sub8(val, 1, result);
                self.cpu.set_flag(FLAG_CF, cf);
                self.write_rm8(ea, is_mem, rm, result as u8);
            }
            _ => {}
        }
    }

    fn exec_group5(&mut self) {
        let (op, val, ea, rm, is_mem) = self.read_rm16();

        match op {
            0 => { // INC
                let cf = self.cpu.get_flag(FLAG_CF);
                let result = (val as u32).wrapping_add(1);
                self.cpu.update_flags_add16(val, 1, result);
                self.cpu.set_flag(FLAG_CF, cf);
                self.write_rm16(ea, is_mem, rm, result as u16);
            }
            1 => { // DEC
                let cf = self.cpu.get_flag(FLAG_CF);
                let result = (val as u32).wrapping_sub(1);
                self.cpu.update_flags_sub16(val, 1, result);
                self.cpu.set_flag(FLAG_CF, cf);
                self.write_rm16(ea, is_mem, rm, result as u16);
            }
            2 => { // CALL near indirect
                self.push16(self.cpu.ip);
                self.cpu.ip = val;
            }
            3 => { // CALL far indirect
                let seg = self.get_seg(self.cpu.ds);
                let new_ip = self.read_u16(seg, ea);
                let new_cs = self.read_u16(seg, ea.wrapping_add(2));
                self.push16(self.cpu.cs);
                self.push16(self.cpu.ip);
                self.cpu.cs = new_cs;
                self.cpu.ip = new_ip;
            }
            4 => { // JMP near indirect
                self.cpu.ip = val;
            }
            5 => { // JMP far indirect
                let seg = self.get_seg(self.cpu.ds);
                self.cpu.ip = self.read_u16(seg, ea);
                self.cpu.cs = self.read_u16(seg, ea.wrapping_add(2));
            }
            6 => { // PUSH
                self.push16(val);
            }
            _ => {}
        }
    }

    // String operations
    fn exec_movsb(&mut self) {
        let src_seg = self.get_seg(self.cpu.ds);
        let val = self.read_u8(src_seg, self.cpu.si);
        self.write_u8(self.cpu.es, self.cpu.di, val);
        if self.cpu.get_flag(FLAG_DF) {
            self.cpu.si = self.cpu.si.wrapping_sub(1);
            self.cpu.di = self.cpu.di.wrapping_sub(1);
        } else {
            self.cpu.si = self.cpu.si.wrapping_add(1);
            self.cpu.di = self.cpu.di.wrapping_add(1);
        }
    }

    fn exec_movsw(&mut self) {
        let src_seg = self.get_seg(self.cpu.ds);
        let val = self.read_u16(src_seg, self.cpu.si);
        self.write_u16(self.cpu.es, self.cpu.di, val);
        if self.cpu.get_flag(FLAG_DF) {
            self.cpu.si = self.cpu.si.wrapping_sub(2);
            self.cpu.di = self.cpu.di.wrapping_sub(2);
        } else {
            self.cpu.si = self.cpu.si.wrapping_add(2);
            self.cpu.di = self.cpu.di.wrapping_add(2);
        }
    }

    fn exec_stosb(&mut self) {
        self.write_u8(self.cpu.es, self.cpu.di, self.cpu.ax as u8);
        if self.cpu.get_flag(FLAG_DF) {
            self.cpu.di = self.cpu.di.wrapping_sub(1);
        } else {
            self.cpu.di = self.cpu.di.wrapping_add(1);
        }
    }

    fn exec_stosw(&mut self) {
        self.write_u16(self.cpu.es, self.cpu.di, self.cpu.ax);
        if self.cpu.get_flag(FLAG_DF) {
            self.cpu.di = self.cpu.di.wrapping_sub(2);
        } else {
            self.cpu.di = self.cpu.di.wrapping_add(2);
        }
    }

    fn exec_lodsb(&mut self) {
        let src_seg = self.get_seg(self.cpu.ds);
        let val = self.read_u8(src_seg, self.cpu.si);
        self.cpu.ax = (self.cpu.ax & 0xFF00) | val as u16;
        if self.cpu.get_flag(FLAG_DF) {
            self.cpu.si = self.cpu.si.wrapping_sub(1);
        } else {
            self.cpu.si = self.cpu.si.wrapping_add(1);
        }
    }

    fn exec_lodsw(&mut self) {
        let src_seg = self.get_seg(self.cpu.ds);
        self.cpu.ax = self.read_u16(src_seg, self.cpu.si);
        if self.cpu.get_flag(FLAG_DF) {
            self.cpu.si = self.cpu.si.wrapping_sub(2);
        } else {
            self.cpu.si = self.cpu.si.wrapping_add(2);
        }
    }

    fn exec_cmpsb(&mut self) {
        let src_seg = self.get_seg(self.cpu.ds);
        let a = self.read_u8(src_seg, self.cpu.si);
        let b = self.read_u8(self.cpu.es, self.cpu.di);
        let result = (a as u16).wrapping_sub(b as u16);
        self.cpu.update_flags_sub8(a, b, result);
        if self.cpu.get_flag(FLAG_DF) {
            self.cpu.si = self.cpu.si.wrapping_sub(1);
            self.cpu.di = self.cpu.di.wrapping_sub(1);
        } else {
            self.cpu.si = self.cpu.si.wrapping_add(1);
            self.cpu.di = self.cpu.di.wrapping_add(1);
        }
    }

    fn exec_cmpsw(&mut self) {
        let src_seg = self.get_seg(self.cpu.ds);
        let a = self.read_u16(src_seg, self.cpu.si);
        let b = self.read_u16(self.cpu.es, self.cpu.di);
        let result = (a as u32).wrapping_sub(b as u32);
        self.cpu.update_flags_sub16(a, b, result);
        if self.cpu.get_flag(FLAG_DF) {
            self.cpu.si = self.cpu.si.wrapping_sub(2);
            self.cpu.di = self.cpu.di.wrapping_sub(2);
        } else {
            self.cpu.si = self.cpu.si.wrapping_add(2);
            self.cpu.di = self.cpu.di.wrapping_add(2);
        }
    }

    fn exec_scasb(&mut self) {
        let b = self.read_u8(self.cpu.es, self.cpu.di);
        let a = self.cpu.ax as u8;
        let result = (a as u16).wrapping_sub(b as u16);
        self.cpu.update_flags_sub8(a, b, result);
        if self.cpu.get_flag(FLAG_DF) {
            self.cpu.di = self.cpu.di.wrapping_sub(1);
        } else {
            self.cpu.di = self.cpu.di.wrapping_add(1);
        }
    }

    fn exec_scasw(&mut self) {
        let b = self.read_u16(self.cpu.es, self.cpu.di);
        let a = self.cpu.ax;
        let result = (a as u32).wrapping_sub(b as u32);
        self.cpu.update_flags_sub16(a, b, result);
        if self.cpu.get_flag(FLAG_DF) {
            self.cpu.di = self.cpu.di.wrapping_sub(2);
        } else {
            self.cpu.di = self.cpu.di.wrapping_add(2);
        }
    }

    fn exec_rep(&mut self, repe: bool) {
        let opcode = self.fetch_u8();

        // Debug: log REP operation start
        serial_write_bytes(b"  REP op=");
        serial_write_hex8(opcode);
        serial_write_bytes(b" CX=");
        serial_write_hex16(self.cpu.cx);
        serial_write_bytes(b" DS:SI=");
        serial_write_hex16(self.cpu.ds);
        serial_write_bytes(b":");
        serial_write_hex16(self.cpu.si);
        serial_write_bytes(b" ES:DI=");
        serial_write_hex16(self.cpu.es);
        serial_write_bytes(b":");
        serial_write_hex16(self.cpu.di);
        serial_write_bytes(b" DF=");
        serial_write_hex8(if self.cpu.get_flag(FLAG_DF) { 1 } else { 0 });
        serial_write_bytes(b"\r\n");

        // Dump source at offset 0000 BEFORE the copy (for MOVSW/MOVSB)
        if matches!(opcode, 0xA4 | 0xA5) {
            let src_seg = self.get_seg(self.cpu.ds);
            serial_write_bytes(b"  BEFORE Src@0000 (");
            serial_write_hex16(src_seg);
            serial_write_bytes(b":0000): ");
            for i in 0..8 {
                let b = self.read_u8(src_seg, i);
                serial_write_hex8(b);
                serial_write_bytes(b" ");
            }
            serial_write_bytes(b"\r\n");
        }

        loop {
            if self.cpu.cx == 0 { break; }

            match opcode {
                0xA4 => self.exec_movsb(),
                0xA5 => self.exec_movsw(),
                0xA6 => self.exec_cmpsb(),
                0xA7 => self.exec_cmpsw(),
                0xAA => self.exec_stosb(),
                0xAB => self.exec_stosw(),
                0xAC => self.exec_lodsb(),
                0xAD => self.exec_lodsw(),
                0xAE => self.exec_scasb(),
                0xAF => self.exec_scasw(),
                _ => break,
            }

            self.cpu.cx = self.cpu.cx.wrapping_sub(1);

            // For CMPS/SCAS, check termination condition
            if matches!(opcode, 0xA6 | 0xA7 | 0xAE | 0xAF) {
                let zf = self.cpu.get_flag(FLAG_ZF);
                if repe && !zf { break; }  // REPE/REPZ: stop if not equal
                if !repe && zf { break; }  // REPNE/REPNZ: stop if equal
            }
        }

        // Debug: log REP operation end
        serial_write_bytes(b"  REP done SI=");
        serial_write_hex16(self.cpu.si);

        // Dump source and destination for MOVSW/MOVSB operations
        if matches!(opcode, 0xA4 | 0xA5) {
            let src_seg = self.get_seg(self.cpu.ds);
            serial_write_bytes(b"\r\n  Src @0000 (");
            serial_write_hex16(src_seg);
            serial_write_bytes(b":0000): ");
            for i in 0..8 {
                let b = self.read_u8(src_seg, i);
                serial_write_hex8(b);
                serial_write_bytes(b" ");
            }
            serial_write_bytes(b"\r\n  Dst @0000 (");
            serial_write_hex16(self.cpu.es);
            serial_write_bytes(b":0000): ");
            for i in 0..8 {
                let b = self.read_u8(self.cpu.es, i);
                serial_write_hex8(b);
                serial_write_bytes(b" ");
            }
            serial_write_bytes(b"\r\n");
        }
        serial_write_bytes(b" DI=");
        serial_write_hex16(self.cpu.di);
        serial_write_bytes(b"\r\n");
    }

    // I/O port access (stubbed - can be extended)
    fn port_in8(&self, _port: u16) -> u8 { 0 }
    fn port_in16(&self, _port: u16) -> u16 { 0 }
    fn port_out8(&self, _port: u16, _val: u8) {}
    fn port_out16(&self, _port: u16, _val: u16) {}

    // Log to serial (shows in QEMU serial log)
    fn log_serial(&mut self, msg: &str) {
        serial_write_bytes(msg.as_bytes());
        serial_write_bytes(b"\r\n");
    }

    // Log unhandled interrupt/function
    fn log_unhandled(&mut self, msg: &str) {
        serial_write_bytes(b"!!! ");
        serial_write_bytes(msg.as_bytes());
        serial_write_bytes(b"\r\n");
    }

    // Interrupt handling
    fn handle_int(&mut self, int_no: u8) {
        // Log all interrupts
        serial_write_bytes(b"INT ");
        serial_write_hex8(int_no);
        serial_write_bytes(b"h AX=");
        serial_write_hex16(self.cpu.ax);
        serial_write_bytes(b"\r\n");

        match int_no {
            0x00 => {
                // Division by zero - terminate
                serial_write_bytes(b"  -> DIV BY ZERO, terminating\r\n");
                self.state = TaskState::Terminated;
            }
            0x10 => self.handle_int10(),
            0x11 => {
                // Equipment list - return basic config
                self.cpu.ax = 0x0021; // Math coprocessor, 80x25 color
            }
            0x12 => {
                // Memory size - return 640KB conventional
                self.cpu.ax = 640;
            }
            0x16 => self.handle_int16(),
            0x1A => self.handle_int1a(),
            0x20 => {
                // DOS terminate
                serial_write_bytes(b"  -> TERMINATE\r\n");
                self.state = TaskState::Terminated;
            }
            0x21 => self.handle_int21(),
            0x2F => {
                // Multiplex interrupt - just return
                self.cpu.ax = 0;
            }
            0x33 => {
                // Mouse - not present
                self.cpu.ax = 0;
            }
            _ => {
                // Log unhandled interrupt
                self.log_unhandled(&format!("[INT {:02X}h unhandled]", int_no));
            }
        }
    }

    // INT 10h - Video BIOS
    fn handle_int10(&mut self) {
        let ah = (self.cpu.ax >> 8) as u8;
        match ah {
            0x00 => {
                // Set video mode - pretend success
                self.cpu.ax = (self.cpu.ax & 0xFF00) | 0x03; // 80x25 color
            }
            0x01 => {
                // Set cursor shape - ignore
            }
            0x02 => {
                // Set cursor position
                let row = (self.cpu.dx >> 8) as u8;
                let col = self.cpu.dx as u8;
                if let Some(con) = console::manager().get(self.console_id) as Option<&mut console::Console> {
                    con.set_cursor(col, row);
                }
            }
            0x03 => {
                // Get cursor position
                if let Some(con) = console::manager().get(self.console_id) as Option<&mut console::Console> {
                    self.cpu.dx = ((con.cursor_y as u16) << 8) | (con.cursor_x as u16);
                } else {
                    self.cpu.dx = 0;
                }
                self.cpu.cx = 0x0607; // Default cursor shape
            }
            0x05 => {
                // Set active display page - ignore (single page)
            }
            0x06 => {
                // Scroll up window
                let lines = self.cpu.ax as u8;
                let attr = (self.cpu.bx >> 8) as u8;
                let top = (self.cpu.cx >> 8) as u8;
                let left = self.cpu.cx as u8;
                let bottom = (self.cpu.dx >> 8) as u8;
                let right = self.cpu.dx as u8;
                if let Some(con) = console::manager().get(self.console_id) as Option<&mut console::Console> {
                    con.scroll_up(lines, attr, top, left, bottom, right);
                }
            }
            0x07 => {
                // Scroll down window - similar to scroll up
                let lines = self.cpu.ax as u8;
                let attr = (self.cpu.bx >> 8) as u8;
                let top = (self.cpu.cx >> 8) as u8;
                let left = self.cpu.cx as u8;
                let bottom = (self.cpu.dx >> 8) as u8;
                let right = self.cpu.dx as u8;
                if let Some(con) = console::manager().get(self.console_id) as Option<&mut console::Console> {
                    con.scroll_down(lines, attr, top, left, bottom, right);
                }
            }
            0x08 => {
                // Read character and attribute at cursor
                self.cpu.ax = 0x0720; // Space with white on black
            }
            0x09 => {
                // Write character and attribute at cursor
                let ch = self.cpu.ax as u8;
                let count = self.cpu.cx;
                for _ in 0..count {
                    self.console_putchar(ch);
                }
            }
            0x0A => {
                // Write character only at cursor
                let ch = self.cpu.ax as u8;
                let count = self.cpu.cx;
                for _ in 0..count {
                    self.console_putchar(ch);
                }
            }
            0x0E => {
                // Teletype output
                let ch = self.cpu.ax as u8;
                self.console_putchar(ch);
            }
            0x0F => {
                // Get video mode
                self.cpu.ax = 0x5003; // 80 columns, mode 3
                self.cpu.bx = 0; // Page 0
            }
            0x10 => {
                // Set/get palette - ignore
            }
            0x11 => {
                // Character generator - ignore
            }
            0x12 => {
                // Video subsystem config - return EGA/VGA info
                self.cpu.bx = 0x0003; // 256KB, color
            }
            0x1A => {
                // Get/set display combination
                self.cpu.ax = 0x001A; // Function supported
                self.cpu.bx = 0x0008; // VGA with color
            }
            _ => {
                self.log_unhandled(&alloc::format!("[INT 10h AH={:02X}h]", ah));
            }
        }
    }

    // INT 1Ah - Time/Date services
    fn handle_int1a(&mut self) {
        let ah = (self.cpu.ax >> 8) as u8;
        match ah {
            0x00 => {
                // Get system time (ticks since midnight)
                // Return a fake time - about 12:00 noon
                self.cpu.cx = 0x000F; // High word
                self.cpu.dx = 0x4240; // Low word (~12:00)
                self.cpu.ax = 0; // Midnight flag
            }
            0x02 => {
                // Get RTC time - return 12:00:00
                self.cpu.cx = 0x1200; // 12:00 in BCD
                self.cpu.dx = 0x0000; // 00 seconds
                self.cpu.set_flag(FLAG_CF, false);
            }
            0x04 => {
                // Get RTC date - return 2025-12-25
                self.cpu.cx = 0x2025; // Year in BCD
                self.cpu.dx = 0x1225; // Month/Day in BCD
                self.cpu.set_flag(FLAG_CF, false);
            }
            _ => {
                self.log_unhandled(&alloc::format!("[INT 1Ah AH={:02X}h]", ah));
            }
        }
    }

    // INT 16h - Keyboard BIOS
    fn handle_int16(&mut self) {
        let ah = (self.cpu.ax >> 8) as u8;
        match ah {
            0x00 | 0x10 => {
                // Get key - return 0 for now (no key)
                // TODO: Hook into actual keyboard input
                self.cpu.ax = 0;
            }
            0x01 | 0x11 => {
                // Check key status
                self.cpu.set_flag(FLAG_ZF, true); // No key available
                self.cpu.ax = 0;
            }
            0x02 | 0x12 => {
                // Get shift flags
                self.cpu.ax = (self.cpu.ax & 0xFF00) | 0; // No shift keys
            }
            0x03 => {
                // Set typematic rate - ignore
            }
            0x05 => {
                // Store key in buffer - ignore
            }
            _ => {
                self.log_unhandled(&alloc::format!("[INT 16h AH={:02X}h]", ah));
            }
        }
    }

    // INT 21h - DOS Services
    fn handle_int21(&mut self) {
        let ah = (self.cpu.ax >> 8) as u8;
        match ah {
            0x01 => {
                // Read character with echo
                self.cpu.ax = (self.cpu.ax & 0xFF00) | 0; // Return 0 for now
            }
            0x02 => {
                // Display character
                let ch = self.cpu.dx as u8;
                self.console_putchar(ch);
            }
            0x06 => {
                // Direct console I/O
                let dl = self.cpu.dx as u8;
                if dl == 0xFF {
                    // Input - no char available
                    self.cpu.set_flag(FLAG_ZF, true);
                    self.cpu.ax = (self.cpu.ax & 0xFF00) | 0;
                } else {
                    // Output
                    self.console_putchar(dl);
                }
            }
            0x09 => {
                // Display string (DS:DX, terminated by '$')
                let ds = self.cpu.ds;
                let mut off = self.cpu.dx;
                loop {
                    let ch = self.read_u8(ds, off);
                    if ch == b'$' { break; }
                    self.console_putchar(ch);
                    off = off.wrapping_add(1);
                }
            }
            0x25 => {
                // Set interrupt vector (ignore)
            }
            0x30 => {
                // Get DOS version
                self.cpu.ax = 0x0005; // DOS 5.0
                self.cpu.bx = 0;
                self.cpu.cx = 0;
            }
            0x35 => {
                // Get interrupt vector
                self.cpu.es = 0;
                self.cpu.bx = 0;
            }
            0x3C => {
                // Create file: DS:DX = filename, CX = attributes
                // Returns: AX = handle, or CF=1 and AX = error
                let filename = self.read_asciiz_string(self.cpu.ds, self.cpu.dx);
                if let Some(handle) = self.dos_create_file(&filename) {
                    self.cpu.ax = handle;
                    self.cpu.set_flag(FLAG_CF, false);
                } else {
                    self.cpu.set_flag(FLAG_CF, true);
                    self.cpu.ax = 0x05; // Access denied
                }
            }
            0x3D => {
                // Open file: DS:DX = filename, AL = mode (0=read, 1=write, 2=r/w)
                // Returns: AX = handle, or CF=1 and AX = error
                let filename = self.read_asciiz_string(self.cpu.ds, self.cpu.dx);
                let mode = self.cpu.ax as u8;
                if let Some(handle) = self.dos_open_file(&filename, mode) {
                    self.cpu.ax = handle;
                    self.cpu.set_flag(FLAG_CF, false);
                } else {
                    self.cpu.set_flag(FLAG_CF, true);
                    self.cpu.ax = 0x02; // File not found
                }
            }
            0x3E => {
                // Close file: BX = handle
                let handle = self.cpu.bx;
                self.dos_close_file(handle);
                self.cpu.set_flag(FLAG_CF, false);
            }
            0x3F => {
                // Read file: BX = handle, CX = bytes, DS:DX = buffer
                // Returns: AX = bytes read, or CF=1 and AX = error
                let handle = self.cpu.bx;
                let count = self.cpu.cx as usize;
                let ds = self.cpu.ds;
                let dx = self.cpu.dx;

                if handle == 0 {
                    // stdin - return 0 for now
                    self.cpu.ax = 0;
                    self.cpu.set_flag(FLAG_CF, false);
                } else if let Some(bytes_read) = self.dos_read_file(handle, count) {
                    // Copy data to buffer
                    for (i, &b) in bytes_read.iter().enumerate() {
                        self.write_u8(ds, dx.wrapping_add(i as u16), b);
                    }
                    self.cpu.ax = bytes_read.len() as u16;
                    self.cpu.set_flag(FLAG_CF, false);
                } else {
                    self.cpu.set_flag(FLAG_CF, true);
                    self.cpu.ax = 0x06; // Invalid handle
                }
            }
            0x40 => {
                // Write file: BX = handle, CX = bytes, DS:DX = buffer
                let handle = self.cpu.bx;
                let count = self.cpu.cx as usize;
                let ds = self.cpu.ds;
                let dx = self.cpu.dx;

                if handle == 1 || handle == 2 {
                    // stdout or stderr - write to console
                    for i in 0..count {
                        let ch = self.read_u8(ds, dx.wrapping_add(i as u16));
                        self.console_putchar(ch);
                    }
                    self.cpu.ax = count as u16;
                    self.cpu.set_flag(FLAG_CF, false);
                } else if let Some(bytes_written) = self.dos_write_file(handle, ds, dx, count) {
                    self.cpu.ax = bytes_written;
                    self.cpu.set_flag(FLAG_CF, false);
                } else {
                    self.cpu.set_flag(FLAG_CF, true);
                    self.cpu.ax = 0x06; // Invalid handle
                }
            }
            0x42 => {
                // Seek (LSEEK): BX = handle, CX:DX = offset, AL = origin
                // Returns: DX:AX = new position, or CF=1 and AX = error
                let handle = self.cpu.bx;
                let offset = ((self.cpu.cx as u32) << 16) | (self.cpu.dx as u32);
                let origin = self.cpu.ax as u8;

                if let Some(new_pos) = self.dos_seek_file(handle, offset, origin) {
                    self.cpu.ax = new_pos as u16;
                    self.cpu.dx = (new_pos >> 16) as u16;
                    self.cpu.set_flag(FLAG_CF, false);
                } else {
                    self.cpu.set_flag(FLAG_CF, true);
                    self.cpu.ax = 0x06; // Invalid handle
                }
            }
            0x44 => {
                // IOCTL - device control
                let al = self.cpu.ax as u8;
                match al {
                    0x00 => {
                        // Get device info - return char device
                        self.cpu.dx = 0x80D3; // Char device, not EOF
                        self.cpu.set_flag(FLAG_CF, false);
                    }
                    _ => {
                        self.cpu.set_flag(FLAG_CF, false);
                        self.cpu.ax = 0;
                    }
                }
            }
            0x48 => {
                // Allocate memory: BX = paragraphs requested
                // Returns: AX = segment of allocated block, or CF=1 and BX = largest available
                let paras = self.cpu.bx;
                serial_write_bytes(b"INT 21h/48h: Allocate ");
                serial_write_hex16(paras);
                serial_write_bytes(b" paragraphs\r\n");

                if let Some(seg) = self.alloc_memory(paras) {
                    self.cpu.ax = seg;
                    self.cpu.set_flag(FLAG_CF, false);
                } else {
                    // Find largest available block for error return
                    let mut mcb_seg = FIRST_MCB_SEG;
                    let mut largest: u16 = 0;
                    loop {
                        let (mcb_type, owner, size) = self.read_mcb(mcb_seg);
                        if owner == MCB_OWNER_FREE && size > largest {
                            largest = size;
                        }
                        if mcb_type == MCB_TYPE_LAST { break; }
                        mcb_seg = mcb_seg + size + 1;
                    }
                    self.cpu.ax = 0x08;  // Not enough memory
                    self.cpu.bx = largest;
                    self.cpu.set_flag(FLAG_CF, true);
                }
            }
            0x49 => {
                // Free memory: ES = segment of block to free
                // Returns: CF=1 on error
                let seg = self.cpu.es;
                serial_write_bytes(b"INT 21h/49h: Free segment ");
                serial_write_hex16(seg);
                serial_write_bytes(b"\r\n");

                if self.free_memory(seg) {
                    self.cpu.set_flag(FLAG_CF, false);
                } else {
                    self.cpu.ax = 0x09;  // Invalid memory block
                    self.cpu.set_flag(FLAG_CF, true);
                }
            }
            0x4A => {
                // Resize memory block: ES = segment, BX = new size in paragraphs
                // Returns: CF=1 on error, BX = largest available if can't grow
                let seg = self.cpu.es;
                let new_size = self.cpu.bx;
                serial_write_bytes(b"INT 21h/4Ah: Resize segment ");
                serial_write_hex16(seg);
                serial_write_bytes(b" to ");
                serial_write_hex16(new_size);
                serial_write_bytes(b" paragraphs\r\n");

                match self.resize_memory(seg, new_size) {
                    Ok(()) => {
                        self.cpu.set_flag(FLAG_CF, false);
                    }
                    Err(max_available) => {
                        self.cpu.ax = 0x08;  // Not enough memory
                        self.cpu.bx = max_available;
                        self.cpu.set_flag(FLAG_CF, true);
                    }
                }
            }
            0x4B => {
                // EXEC - load and execute program
                // This is complex - log and fail for now
                let filename = self.read_asciiz_string(self.cpu.ds, self.cpu.dx);
                self.log_unhandled(&alloc::format!("[EXEC: {}]", filename));
                self.cpu.set_flag(FLAG_CF, true);
                self.cpu.ax = 0x02; // File not found
            }
            0x4C => {
                // Terminate with return code
                self.state = TaskState::Terminated;
                // Remove console and switch to parent
                console::manager().remove_console(self.console_id);
            }
            0x4D => {
                // Get return code of child
                self.cpu.ax = 0; // No child, return 0
            }
            0x4E => {
                // Find first file (FCB) - not implemented
                self.cpu.set_flag(FLAG_CF, true);
                self.cpu.ax = 0x12; // No more files
            }
            0x4F => {
                // Find next file (FCB) - not implemented
                self.cpu.set_flag(FLAG_CF, true);
                self.cpu.ax = 0x12; // No more files
            }
            0x50 => {
                // Set PSP - ignore
            }
            0x51 | 0x62 => {
                // Get PSP address
                self.cpu.bx = self.psp_seg;
            }
            0x52 => {
                // Get DOS internal pointer - return dummy
                self.cpu.es = 0;
                self.cpu.bx = 0;
            }
            0x57 => {
                // Get/Set file date/time
                let al = self.cpu.ax as u8;
                if al == 0 {
                    // Get - return current date/time
                    self.cpu.cx = 0x6000; // 12:00:00
                    self.cpu.dx = 0x5B39; // 2025-12-25
                    self.cpu.set_flag(FLAG_CF, false);
                } else {
                    // Set - just succeed
                    self.cpu.set_flag(FLAG_CF, false);
                }
            }
            0x58 => {
                // Get/Set memory allocation strategy
                self.cpu.ax = 0;
                self.cpu.set_flag(FLAG_CF, false);
            }
            0x59 => {
                // Get extended error info
                self.cpu.ax = 0; // No error
                self.cpu.bx = 0;
                self.cpu.cx = 0;
            }
            0x5A => {
                // Create temp file - create in current dir
                let handle = self.alloc_handle();
                if let Some(h) = handle {
                    self.file_handles[h as usize] = Some(FileHandle {
                        filename: String::from("TEMP.$$$"),
                        position: 0,
                        size: 0,
                        data: Vec::new(),
                        writable: true,
                    });
                    self.cpu.ax = h;
                    self.cpu.set_flag(FLAG_CF, false);
                } else {
                    self.cpu.set_flag(FLAG_CF, true);
                    self.cpu.ax = 0x04; // Too many files
                }
            }
            0x5B => {
                // Create new file - fail if exists
                let filename = self.read_asciiz_string(self.cpu.ds, self.cpu.dx);
                if let Some(handle) = self.dos_create_file(&filename) {
                    self.cpu.ax = handle;
                    self.cpu.set_flag(FLAG_CF, false);
                } else {
                    self.cpu.set_flag(FLAG_CF, true);
                    self.cpu.ax = 0x05;
                }
            }
            _ => {
                // Log unhandled DOS function
                self.log_unhandled(&alloc::format!("[INT 21h AH={:02X}h]", ah));
                self.cpu.set_flag(FLAG_CF, true);
                self.cpu.ax = 0x01; // Invalid function
            }
        }
    }

    // Read null-terminated string from memory
    fn read_asciiz_string(&self, seg: u16, off: u16) -> String {
        let mut result = String::new();
        let mut o = off;
        loop {
            let ch = self.read_u8(seg, o);
            if ch == 0 { break; }
            result.push(ch as char);
            o = o.wrapping_add(1);
            if result.len() > 128 { break; } // Safety limit
        }
        result
    }

    // Allocate a file handle (handles 5-19 for user files, 0-4 reserved)
    fn alloc_handle(&mut self) -> Option<u16> {
        for i in 5..20 {
            if self.file_handles[i].is_none() {
                return Some(i as u16);
            }
        }
        None
    }

    // DOS Create File (INT 21h/3Ch)
    fn dos_create_file(&mut self, filename: &str) -> Option<u16> {
        let handle = self.alloc_handle()?;
        self.file_handles[handle as usize] = Some(FileHandle {
            filename: String::from(filename),
            position: 0,
            size: 0,
            data: Vec::new(),
            writable: true,
        });
        Some(handle)
    }

    // DOS Open File (INT 21h/3Dh) - uses VFS for any filesystem
    fn dos_open_file(&mut self, filename: &str, mode: u8) -> Option<u16> {
        // Use VFS abstraction - works with WFS, FAT, or any filesystem
        let current_name = disk::drive_manager().current_drive_name().to_string();
        let mut vfs = disk::drive_manager().get_vfs(&current_name)?;

        // Read file data using VFS
        let data = vfs.read_file(filename).ok()?;
        let size = data.len() as u32;

        // Allocate handle
        let handle = self.alloc_handle()?;
        self.file_handles[handle as usize] = Some(FileHandle {
            filename: String::from(filename),
            position: 0,
            size,
            data,
            writable: (mode & 1) != 0 || (mode & 2) != 0,
        });

        Some(handle)
    }

    // DOS Close File (INT 21h/3Eh)
    fn dos_close_file(&mut self, handle: u16) {
        if (handle as usize) < 20 {
            // If writable and modified, write back to disk (TODO)
            self.file_handles[handle as usize] = None;
        }
    }

    // DOS Read File (INT 21h/3Fh)
    fn dos_read_file(&mut self, handle: u16, count: usize) -> Option<Vec<u8>> {
        if handle as usize >= 20 { return None; }
        let fh = self.file_handles[handle as usize].as_mut()?;

        let pos = fh.position as usize;
        let available = if pos < fh.data.len() { fh.data.len() - pos } else { 0 };
        let to_read = core::cmp::min(count, available);

        let result = fh.data[pos..pos + to_read].to_vec();
        fh.position += to_read as u32;

        Some(result)
    }

    // DOS Write File (INT 21h/40h)
    fn dos_write_file(&mut self, handle: u16, seg: u16, off: u16, count: usize) -> Option<u16> {
        if handle as usize >= 20 { return None; }

        // Check writable first
        if let Some(fh) = &self.file_handles[handle as usize] {
            if !fh.writable { return None; }
        } else {
            return None;
        }

        // Collect data from memory first (before borrowing file_handles mutably)
        let mut data = Vec::with_capacity(count);
        for i in 0..count {
            data.push(self.read_u8(seg, off.wrapping_add(i as u16)));
        }

        // Now get mutable reference to file handle
        let fh = self.file_handles[handle as usize].as_mut()?;
        let pos = fh.position as usize;

        // Extend file if needed
        if pos + count > fh.data.len() {
            fh.data.resize(pos + count, 0);
        }

        // Write data
        fh.data[pos..pos + count].copy_from_slice(&data);
        fh.position += count as u32;
        fh.size = fh.data.len() as u32;

        Some(count as u16)
    }

    // DOS Seek File (INT 21h/42h)
    fn dos_seek_file(&mut self, handle: u16, offset: u32, origin: u8) -> Option<u32> {
        if handle as usize >= 20 { return None; }
        let fh = self.file_handles[handle as usize].as_mut()?;

        let new_pos = match origin {
            0 => offset, // SEEK_SET
            1 => fh.position.wrapping_add(offset), // SEEK_CUR
            2 => fh.size.wrapping_add(offset), // SEEK_END
            _ => return None,
        };

        fh.position = new_pos;
        Some(new_pos)
    }
}

const NONE_FILE: Option<FileHandle> = None;

fn outb(port: u16, value: u8) {
    unsafe { core::arch::asm!("out dx, al", in("dx") port, in("al") value, options(nostack, preserves_flags)); }
}

fn serial_write_bytes(s: &[u8]) {
    for &b in s { outb(0x3F8, b); }
}

fn serial_write_u16(n: u16) {
    let mut buf = [b'0'; 5];
    let mut val = n;
    let mut i = 4isize;
    while i >= 0 {
        buf[i as usize] = b'0' + (val % 10) as u8;
        val /= 10;
        i -= 1;
    }
    // Skip leading zeros
    let mut start = 0;
    while start < 4 && buf[start] == b'0' { start += 1; }
    serial_write_bytes(&buf[start..]);
}

fn serial_write_hex8(n: u8) {
    const HEX: &[u8] = b"0123456789ABCDEF";
    outb(0x3F8, HEX[(n >> 4) as usize]);
    outb(0x3F8, HEX[(n & 0xF) as usize]);
}

fn serial_write_hex16(n: u16) {
    serial_write_hex8((n >> 8) as u8);
    serial_write_hex8(n as u8);
}

static mut TASK_LIST: Option<Vec<DosTask>> = None;

impl Runtime for Dos16Runtime {
    fn name(&self) -> &'static str { "Dos16Runtime" }

    fn can_run(&self, format: BinaryFormat) -> bool {
        matches!(format, BinaryFormat::DosCom | BinaryFormat::DosExe)
    }

    fn run(&self, filename: &str, data: &[u8]) -> RunResult {
        unsafe {
            static mut NEXT_ID: u32 = 1;
            let id = NEXT_ID;
            NEXT_ID += 1;
            schedule_task_record(id, String::from(filename));
            if TASK_LIST.is_none() { TASK_LIST = Some(Vec::new()); }
            if let Some(list) = TASK_LIST.as_mut() {
                list.push(DosTask::new(id, String::from(filename), data));
            }
            RunResult::Scheduled(id)
        }
    }
}

pub fn poll_tasks() {
    unsafe {
        if TASK_LIST.is_none() { return; }
        if let Some(list) = TASK_LIST.as_mut() {
            let mut i = 0usize;
            while i < list.len() {
                if list[i].state == TaskState::Terminated {
                    list.remove(i);
                } else {
                    // Execute instruction budget
                    for _ in 0..256 {
                        list[i].step();
                        if list[i].state != TaskState::Running { break; }
                    }
                    i += 1;
                }
            }
        }
    }
}

pub fn has_running_tasks() -> bool {
    unsafe {
        if let Some(list) = TASK_LIST.as_ref() {
            !list.is_empty()
        } else {
            false
        }
    }
}
