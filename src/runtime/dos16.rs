//! DOS 16-bit Runtime - x86 Real Mode Interpreter
//!
//! Implements a 16-bit x86 CPU interpreter for running DOS COM/EXE programs.

extern crate alloc;
use alloc::{string::String, vec::Vec, vec};
use super::{Runtime, BinaryFormat, RunResult, schedule_task_record};
use crate::disk;

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
        let mut memory = vec![0u8; 1024 * 1024]; // 1 MiB

        // Check if MZ (EXE) or COM
        let is_exe = data.len() >= 2 && data[0] == b'M' && data[1] == b'Z';

        let mut cpu = Cpu16::new();

        serial_write_bytes(b"[DOS16] Loading ");
        serial_write_bytes(filename.as_bytes());
        serial_write_bytes(b" (");
        serial_write_u16(data.len() as u16);
        serial_write_bytes(b" bytes)\r\n");

        if is_exe {
            serial_write_bytes(b"[DOS16] Format: MZ EXE\r\n");
            // Parse MZ header and load EXE
            Self::load_exe(&mut memory, &mut cpu, data);
        } else {
            serial_write_bytes(b"[DOS16] Format: COM, loading at 0x100\r\n");
            // Load COM at offset 0x100
            let load_addr = 0x100usize;
            let copy_len = core::cmp::min(data.len(), memory.len() - load_addr);
            memory[load_addr..load_addr + copy_len].copy_from_slice(&data[..copy_len]);
            cpu.ip = 0x100;
            // Set up segment registers for COM file
            // All segments point to PSP segment (0x0000 for simplicity)
            cpu.cs = 0;
            cpu.ds = 0;
            cpu.es = 0;
            cpu.ss = 0;
        }

        // Set up minimal PSP at offset 0
        memory[0] = 0xCD; // INT 20h at PSP:0000
        memory[1] = 0x20;

        DosTask {
            id,
            filename,
            cpu,
            memory,
            state: TaskState::Running,
            seg_override: None,
            file_handles: [NONE_FILE; 20],
        }
    }

    fn load_exe(memory: &mut [u8], cpu: &mut Cpu16, data: &[u8]) {
        if data.len() < 28 { return; }

        // MZ header parsing
        let last_page_size = u16::from_le_bytes([data[2], data[3]]);
        let page_count = u16::from_le_bytes([data[4], data[5]]);
        let reloc_count = u16::from_le_bytes([data[6], data[7]]) as usize;
        let header_paras = u16::from_le_bytes([data[8], data[9]]) as usize;
        let _min_alloc = u16::from_le_bytes([data[10], data[11]]);
        let _max_alloc = u16::from_le_bytes([data[12], data[13]]);
        let init_ss = u16::from_le_bytes([data[14], data[15]]);
        let init_sp = u16::from_le_bytes([data[16], data[17]]);
        let _checksum = u16::from_le_bytes([data[18], data[19]]);
        let init_ip = u16::from_le_bytes([data[20], data[21]]);
        let init_cs = u16::from_le_bytes([data[22], data[23]]);
        let reloc_offset = u16::from_le_bytes([data[24], data[25]]) as usize;

        // Calculate code size
        let header_size = header_paras * 16;
        let code_size = if last_page_size == 0 {
            (page_count as usize) * 512 - header_size
        } else {
            ((page_count as usize) - 1) * 512 + (last_page_size as usize) - header_size
        };

        // Load segment (after PSP)
        let load_seg: u16 = 0x0010; // Load at segment 0x10 (linear 0x100)
        let load_addr = (load_seg as usize) << 4;

        // Copy code
        if header_size < data.len() {
            let code_start = header_size;
            let code_end = core::cmp::min(code_start + code_size, data.len());
            let copy_len = core::cmp::min(code_end - code_start, memory.len() - load_addr);
            memory[load_addr..load_addr + copy_len]
                .copy_from_slice(&data[code_start..code_start + copy_len]);
        }

        // Apply relocations
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

        // Set up registers
        cpu.cs = load_seg.wrapping_add(init_cs);
        cpu.ip = init_ip;
        cpu.ss = load_seg.wrapping_add(init_ss);
        cpu.sp = init_sp;
        cpu.ds = load_seg;
        cpu.es = load_seg;
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

    // ModR/M decoding - returns (reg field, effective address or register value, is_memory)
    fn decode_modrm(&mut self, wide: bool) -> (u8, u16, bool) {
        let modrm = self.fetch_u8();
        let mode = (modrm >> 6) & 3;
        let reg = (modrm >> 3) & 7;
        let rm = modrm & 7;

        if mode == 3 {
            // Register mode
            let val = if wide { self.cpu.get_reg16(rm) } else { self.cpu.get_reg8(rm) as u16 };
            return (reg, val, false);
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
        let default_seg = if rm == 2 || rm == 3 || (rm == 6 && mode != 0) {
            self.cpu.ss // BP-based addressing uses SS
        } else {
            self.cpu.ds
        };

        (reg, ea, true)
    }

    // Read operand from ModR/M
    fn read_rm8(&mut self) -> (u8, u8, u16, bool) {
        let (reg, ea, is_mem) = self.decode_modrm(false);
        let val = if is_mem {
            let seg = self.get_seg(self.cpu.ds);
            self.read_u8(seg, ea)
        } else {
            ea as u8
        };
        (reg, val, ea, is_mem)
    }

    fn read_rm16(&mut self) -> (u8, u16, u16, bool) {
        let (reg, ea, is_mem) = self.decode_modrm(true);
        let val = if is_mem {
            let seg = self.get_seg(self.cpu.ds);
            self.read_u16(seg, ea)
        } else {
            ea
        };
        (reg, val, ea, is_mem)
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
        let opcode = self.fetch_u8();

        match opcode {
            // Segment override prefixes
            0x26 => { self.seg_override = Some(self.cpu.es); self.step(); }
            0x2E => { self.seg_override = Some(self.cpu.cs); self.step(); }
            0x36 => { self.seg_override = Some(self.cpu.ss); self.step(); }
            0x3E => { self.seg_override = Some(self.cpu.ds); self.step(); }

            // ADD r/m8, r8
            0x00 => {
                let (reg, val, ea, is_mem) = self.read_rm8();
                let src = self.cpu.get_reg8(reg);
                let result = (val as u16).wrapping_add(src as u16);
                self.cpu.update_flags_add8(val, src, result);
                self.write_rm8(ea, is_mem, ea as u8, result as u8);
            }
            // ADD r/m16, r16
            0x01 => {
                let (reg, val, ea, is_mem) = self.read_rm16();
                let src = self.cpu.get_reg16(reg);
                let result = (val as u32).wrapping_add(src as u32);
                self.cpu.update_flags_add16(val, src, result);
                self.write_rm16(ea, is_mem, ea as u8, result as u16);
            }
            // ADD r8, r/m8
            0x02 => {
                let (reg, val, _, _) = self.read_rm8();
                let dst = self.cpu.get_reg8(reg);
                let result = (dst as u16).wrapping_add(val as u16);
                self.cpu.update_flags_add8(dst, val, result);
                self.cpu.set_reg8(reg, result as u8);
            }
            // ADD r16, r/m16
            0x03 => {
                let (reg, val, _, _) = self.read_rm16();
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
                self.cpu.update_flags_add16(ax, imm, result);
                self.cpu.ax = result as u16;
            }

            // PUSH ES
            0x06 => self.push16(self.cpu.es),
            // POP ES
            0x07 => self.cpu.es = self.pop16(),

            // OR r/m8, r8
            0x08 => {
                let (reg, val, ea, is_mem) = self.read_rm8();
                let src = self.cpu.get_reg8(reg);
                let result = val | src;
                self.cpu.update_flags_logic8(result);
                self.write_rm8(ea, is_mem, ea as u8, result);
            }
            // OR r/m16, r16
            0x09 => {
                let (reg, val, ea, is_mem) = self.read_rm16();
                let src = self.cpu.get_reg16(reg);
                let result = val | src;
                self.cpu.update_flags_logic16(result);
                self.write_rm16(ea, is_mem, ea as u8, result);
            }
            // OR r8, r/m8
            0x0A => {
                let (reg, val, _, _) = self.read_rm8();
                let dst = self.cpu.get_reg8(reg);
                let result = dst | val;
                self.cpu.update_flags_logic8(result);
                self.cpu.set_reg8(reg, result);
            }
            // OR r16, r/m16
            0x0B => {
                let (reg, val, _, _) = self.read_rm16();
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
                let (reg, val, ea, is_mem) = self.read_rm8();
                let src = self.cpu.get_reg8(reg);
                let result = val & src;
                self.cpu.update_flags_logic8(result);
                self.write_rm8(ea, is_mem, ea as u8, result);
            }
            // AND r/m16, r16
            0x21 => {
                let (reg, val, ea, is_mem) = self.read_rm16();
                let src = self.cpu.get_reg16(reg);
                let result = val & src;
                self.cpu.update_flags_logic16(result);
                self.write_rm16(ea, is_mem, ea as u8, result);
            }
            // AND r8, r/m8
            0x22 => {
                let (reg, val, _, _) = self.read_rm8();
                let dst = self.cpu.get_reg8(reg);
                let result = dst & val;
                self.cpu.update_flags_logic8(result);
                self.cpu.set_reg8(reg, result);
            }
            // AND r16, r/m16
            0x23 => {
                let (reg, val, _, _) = self.read_rm16();
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
                let (reg, val, ea, is_mem) = self.read_rm8();
                let src = self.cpu.get_reg8(reg);
                let result = (val as u16).wrapping_sub(src as u16);
                self.cpu.update_flags_sub8(val, src, result);
                self.write_rm8(ea, is_mem, ea as u8, result as u8);
            }
            // SUB r/m16, r16
            0x29 => {
                let (reg, val, ea, is_mem) = self.read_rm16();
                let src = self.cpu.get_reg16(reg);
                let result = (val as u32).wrapping_sub(src as u32);
                self.cpu.update_flags_sub16(val, src, result);
                self.write_rm16(ea, is_mem, ea as u8, result as u16);
            }
            // SUB r8, r/m8
            0x2A => {
                let (reg, val, _, _) = self.read_rm8();
                let dst = self.cpu.get_reg8(reg);
                let result = (dst as u16).wrapping_sub(val as u16);
                self.cpu.update_flags_sub8(dst, val, result);
                self.cpu.set_reg8(reg, result as u8);
            }
            // SUB r16, r/m16
            0x2B => {
                let (reg, val, _, _) = self.read_rm16();
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
                let (reg, val, ea, is_mem) = self.read_rm8();
                let src = self.cpu.get_reg8(reg);
                let result = val ^ src;
                self.cpu.update_flags_logic8(result);
                self.write_rm8(ea, is_mem, ea as u8, result);
            }
            // XOR r/m16, r16
            0x31 => {
                let (reg, val, ea, is_mem) = self.read_rm16();
                let src = self.cpu.get_reg16(reg);
                let result = val ^ src;
                self.cpu.update_flags_logic16(result);
                self.write_rm16(ea, is_mem, ea as u8, result);
            }
            // XOR r8, r/m8
            0x32 => {
                let (reg, val, _, _) = self.read_rm8();
                let dst = self.cpu.get_reg8(reg);
                let result = dst ^ val;
                self.cpu.update_flags_logic8(result);
                self.cpu.set_reg8(reg, result);
            }
            // XOR r16, r/m16
            0x33 => {
                let (reg, val, _, _) = self.read_rm16();
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
                let (reg, val, _, _) = self.read_rm8();
                let src = self.cpu.get_reg8(reg);
                let result = (val as u16).wrapping_sub(src as u16);
                self.cpu.update_flags_sub8(val, src, result);
            }
            // CMP r/m16, r16
            0x39 => {
                let (reg, val, _, _) = self.read_rm16();
                let src = self.cpu.get_reg16(reg);
                let result = (val as u32).wrapping_sub(src as u32);
                self.cpu.update_flags_sub16(val, src, result);
            }
            // CMP r8, r/m8
            0x3A => {
                let (reg, val, _, _) = self.read_rm8();
                let dst = self.cpu.get_reg8(reg);
                let result = (dst as u16).wrapping_sub(val as u16);
                self.cpu.update_flags_sub8(dst, val, result);
            }
            // CMP r16, r/m16
            0x3B => {
                let (reg, val, _, _) = self.read_rm16();
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
                let (reg, val, _, _) = self.read_rm8();
                let src = self.cpu.get_reg8(reg);
                let result = val & src;
                self.cpu.update_flags_logic8(result);
            }
            // TEST r/m16, r16
            0x85 => {
                let (reg, val, _, _) = self.read_rm16();
                let src = self.cpu.get_reg16(reg);
                let result = val & src;
                self.cpu.update_flags_logic16(result);
            }

            // XCHG r/m8, r8
            0x86 => {
                let (reg, val, ea, is_mem) = self.read_rm8();
                let src = self.cpu.get_reg8(reg);
                self.cpu.set_reg8(reg, val);
                self.write_rm8(ea, is_mem, ea as u8, src);
            }
            // XCHG r/m16, r16
            0x87 => {
                let (reg, val, ea, is_mem) = self.read_rm16();
                let src = self.cpu.get_reg16(reg);
                self.cpu.set_reg16(reg, val);
                self.write_rm16(ea, is_mem, ea as u8, src);
            }

            // MOV r/m8, r8
            0x88 => {
                let (reg, _, ea, is_mem) = self.read_rm8();
                let val = self.cpu.get_reg8(reg);
                self.write_rm8(ea, is_mem, ea as u8, val);
            }
            // MOV r/m16, r16
            0x89 => {
                let (reg, _, ea, is_mem) = self.read_rm16();
                let val = self.cpu.get_reg16(reg);
                self.write_rm16(ea, is_mem, ea as u8, val);
            }
            // MOV r8, r/m8
            0x8A => {
                let (reg, val, _, _) = self.read_rm8();
                self.cpu.set_reg8(reg, val);
            }
            // MOV r16, r/m16
            0x8B => {
                let (reg, val, _, _) = self.read_rm16();
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
                    let (_, _, ea, is_mem) = self.read_rm16();
                    self.write_rm16(ea, is_mem, ea as u8, val);
                }
            }
            // LEA r16, m
            0x8D => {
                let (reg, ea, _) = self.decode_modrm(true);
                self.cpu.set_reg16(reg, ea);
            }
            // MOV Sreg, r/m16
            0x8E => {
                let (reg, val, _, _) = self.read_rm16();
                self.cpu.set_seg(reg, val);
            }
            // POP r/m16
            0x8F => {
                let (_, _, ea, is_mem) = self.read_rm16();
                let val = self.pop16();
                self.write_rm16(ea, is_mem, ea as u8, val);
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
                let (reg, ea, _) = self.decode_modrm(true);
                let seg = self.get_seg(self.cpu.ds);
                let off = self.read_u16(seg, ea);
                let new_seg = self.read_u16(seg, ea.wrapping_add(2));
                self.cpu.set_reg16(reg, off);
                self.cpu.es = new_seg;
            }
            // LDS r16, m16:16
            0xC5 => {
                let (reg, ea, _) = self.decode_modrm(true);
                let seg = self.get_seg(self.cpu.ds);
                let off = self.read_u16(seg, ea);
                let new_seg = self.read_u16(seg, ea.wrapping_add(2));
                self.cpu.set_reg16(reg, off);
                self.cpu.ds = new_seg;
            }

            // MOV r/m8, imm8
            0xC6 => {
                let (_, _, ea, is_mem) = self.read_rm8();
                let imm = self.fetch_u8();
                self.write_rm8(ea, is_mem, ea as u8, imm);
            }
            // MOV r/m16, imm16
            0xC7 => {
                let (_, _, ea, is_mem) = self.read_rm16();
                let imm = self.fetch_u16();
                self.write_rm16(ea, is_mem, ea as u8, imm);
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
                let rel = self.fetch_u16();
                self.cpu.ip = self.cpu.ip.wrapping_add(rel);
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

            // LOCK prefix
            0xF0 => { self.step(); }

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
                // Skip unknown instruction
            }
        }
    }

    // Helper methods for instruction groups and complex operations
    fn exec_adc(&mut self, opcode: u8) {
        let carry = if self.cpu.get_flag(FLAG_CF) { 1 } else { 0 };
        match opcode {
            0x10 => {
                let (reg, val, ea, is_mem) = self.read_rm8();
                let src = self.cpu.get_reg8(reg);
                let result = (val as u16).wrapping_add(src as u16).wrapping_add(carry);
                self.cpu.update_flags_add8(val, src.wrapping_add(carry as u8), result);
                self.write_rm8(ea, is_mem, ea as u8, result as u8);
            }
            0x11 => {
                let (reg, val, ea, is_mem) = self.read_rm16();
                let src = self.cpu.get_reg16(reg);
                let result = (val as u32).wrapping_add(src as u32).wrapping_add(carry as u32);
                self.cpu.update_flags_add16(val, src.wrapping_add(carry), result);
                self.write_rm16(ea, is_mem, ea as u8, result as u16);
            }
            0x12 => {
                let (reg, val, _, _) = self.read_rm8();
                let dst = self.cpu.get_reg8(reg);
                let result = (dst as u16).wrapping_add(val as u16).wrapping_add(carry);
                self.cpu.update_flags_add8(dst, val.wrapping_add(carry as u8), result);
                self.cpu.set_reg8(reg, result as u8);
            }
            0x13 => {
                let (reg, val, _, _) = self.read_rm16();
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
                let (reg, val, ea, is_mem) = self.read_rm8();
                let src = self.cpu.get_reg8(reg).wrapping_add(borrow as u8);
                let result = (val as u16).wrapping_sub(src as u16);
                self.cpu.update_flags_sub8(val, src, result);
                self.write_rm8(ea, is_mem, ea as u8, result as u8);
            }
            0x19 => {
                let (reg, val, ea, is_mem) = self.read_rm16();
                let src = self.cpu.get_reg16(reg).wrapping_add(borrow);
                let result = (val as u32).wrapping_sub(src as u32);
                self.cpu.update_flags_sub16(val, src, result);
                self.write_rm16(ea, is_mem, ea as u8, result as u16);
            }
            0x1A => {
                let (reg, val, _, _) = self.read_rm8();
                let dst = self.cpu.get_reg8(reg);
                let src = val.wrapping_add(borrow as u8);
                let result = (dst as u16).wrapping_sub(src as u16);
                self.cpu.update_flags_sub8(dst, src, result);
                self.cpu.set_reg8(reg, result as u8);
            }
            0x1B => {
                let (reg, val, _, _) = self.read_rm16();
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
        let (op, val, ea, is_mem) = self.read_rm8();
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

        self.write_rm8(ea, is_mem, ea as u8, result);
    }

    fn exec_group1_16(&mut self, sign_extend: bool) {
        let (op, val, ea, is_mem) = self.read_rm16();
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

        self.write_rm16(ea, is_mem, ea as u8, result);
    }

    fn exec_group2_8(&mut self, count: u8) {
        let (op, val, ea, is_mem) = self.read_rm8();
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

        self.write_rm8(ea, is_mem, ea as u8, result);
    }

    fn exec_group2_16(&mut self, count: u8) {
        let (op, val, ea, is_mem) = self.read_rm16();
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

        self.write_rm16(ea, is_mem, ea as u8, result);
    }

    fn exec_group3_8(&mut self) {
        let (op, val, ea, is_mem) = self.read_rm8();

        match op {
            0 | 1 => { // TEST r/m8, imm8
                let imm = self.fetch_u8();
                let result = val & imm;
                self.cpu.update_flags_logic8(result);
            }
            2 => { // NOT
                self.write_rm8(ea, is_mem, ea as u8, !val);
            }
            3 => { // NEG
                let result = (0u16).wrapping_sub(val as u16);
                self.cpu.update_flags_sub8(0, val, result);
                self.write_rm8(ea, is_mem, ea as u8, result as u8);
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
        let (op, val, ea, is_mem) = self.read_rm16();

        match op {
            0 | 1 => { // TEST r/m16, imm16
                let imm = self.fetch_u16();
                let result = val & imm;
                self.cpu.update_flags_logic16(result);
            }
            2 => { // NOT
                self.write_rm16(ea, is_mem, ea as u8, !val);
            }
            3 => { // NEG
                let result = (0u32).wrapping_sub(val as u32);
                self.cpu.update_flags_sub16(0, val, result);
                self.write_rm16(ea, is_mem, ea as u8, result as u16);
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
        let (op, val, ea, is_mem) = self.read_rm8();
        let cf = self.cpu.get_flag(FLAG_CF);

        match op {
            0 => { // INC
                let result = (val as u16).wrapping_add(1);
                self.cpu.update_flags_add8(val, 1, result);
                self.cpu.set_flag(FLAG_CF, cf);
                self.write_rm8(ea, is_mem, ea as u8, result as u8);
            }
            1 => { // DEC
                let result = (val as u16).wrapping_sub(1);
                self.cpu.update_flags_sub8(val, 1, result);
                self.cpu.set_flag(FLAG_CF, cf);
                self.write_rm8(ea, is_mem, ea as u8, result as u8);
            }
            _ => {}
        }
    }

    fn exec_group5(&mut self) {
        let (op, val, ea, is_mem) = self.read_rm16();

        match op {
            0 => { // INC
                let cf = self.cpu.get_flag(FLAG_CF);
                let result = (val as u32).wrapping_add(1);
                self.cpu.update_flags_add16(val, 1, result);
                self.cpu.set_flag(FLAG_CF, cf);
                self.write_rm16(ea, is_mem, ea as u8, result as u16);
            }
            1 => { // DEC
                let cf = self.cpu.get_flag(FLAG_CF);
                let result = (val as u32).wrapping_sub(1);
                self.cpu.update_flags_sub16(val, 1, result);
                self.cpu.set_flag(FLAG_CF, cf);
                self.write_rm16(ea, is_mem, ea as u8, result as u16);
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
    }

    // I/O port access (stubbed - can be extended)
    fn port_in8(&self, _port: u16) -> u8 { 0 }
    fn port_in16(&self, _port: u16) -> u16 { 0 }
    fn port_out8(&self, _port: u16, _val: u8) {}
    fn port_out16(&self, _port: u16, _val: u16) {}

    // Interrupt handling
    fn handle_int(&mut self, int_no: u8) {
        match int_no {
            0x00 => {
                // Division by zero - terminate
                self.state = TaskState::Terminated;
            }
            0x10 => self.handle_int10(),
            0x16 => self.handle_int16(),
            0x20 => {
                // DOS terminate
                self.state = TaskState::Terminated;
            }
            0x21 => self.handle_int21(),
            _ => {
                // Unhandled interrupt - ignore for now
            }
        }
    }

    // INT 10h - Video BIOS
    fn handle_int10(&mut self) {
        let ah = (self.cpu.ax >> 8) as u8;
        match ah {
            0x0E => {
                // Teletype output
                let ch = self.cpu.ax as u8;
                serial_write_bytes(&[ch]);
            }
            0x00 => {
                // Set video mode - ignore
            }
            0x02 => {
                // Set cursor position - ignore for now
            }
            0x03 => {
                // Get cursor position
                self.cpu.dx = 0; // Row 0, col 0
                self.cpu.cx = 0x0607; // Default cursor shape
            }
            _ => {}
        }
    }

    // INT 16h - Keyboard BIOS
    fn handle_int16(&mut self) {
        let ah = (self.cpu.ax >> 8) as u8;
        match ah {
            0x00 => {
                // Get key - return 0 for now (no key)
                self.cpu.ax = 0;
            }
            0x01 => {
                // Check key status
                self.cpu.set_flag(FLAG_ZF, true); // No key available
                self.cpu.ax = 0;
            }
            0x02 => {
                // Get shift flags
                self.cpu.ax = (self.cpu.ax & 0xFF00) | 0; // No shift keys
            }
            _ => {}
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
                serial_write_bytes(&[ch]);
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
                    serial_write_bytes(&[dl]);
                }
            }
            0x09 => {
                // Display string (DS:DX, terminated by '$')
                let ds = self.cpu.ds;
                let mut off = self.cpu.dx;
                loop {
                    let ch = self.read_u8(ds, off);
                    if ch == b'$' { break; }
                    serial_write_bytes(&[ch]);
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
                    // stdout or stderr
                    for i in 0..count {
                        let ch = self.read_u8(ds, dx.wrapping_add(i as u16));
                        serial_write_bytes(&[ch]);
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
            0x4A => {
                // Resize memory block - just succeed
                self.cpu.set_flag(FLAG_CF, false);
            }
            0x4C => {
                // Terminate with return code
                self.state = TaskState::Terminated;
            }
            0x62 => {
                // Get PSP address
                self.cpu.bx = 0; // PSP at segment 0
            }
            _ => {
                // Unsupported function
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

    // DOS Open File (INT 21h/3Dh)
    fn dos_open_file(&mut self, filename: &str, mode: u8) -> Option<u16> {
        // Try to open from WFS
        let current = disk::drive_manager().current_drive();
        let drive = disk::drive_manager().get_drive(current)?;
        let ahci = disk::AhciController::new_port(drive.disk_port)?;
        let mut wfs = disk::Wfs::mount(ahci)?;

        // Find the file
        let entry = wfs.find_file(filename)?;

        // Read file data
        let mut data = vec![0u8; entry.size as usize];
        wfs.read_file(&entry, &mut data)?;

        // Allocate handle
        let handle = self.alloc_handle()?;
        self.file_handles[handle as usize] = Some(FileHandle {
            filename: String::from(filename),
            position: 0,
            size: entry.size as u32,
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
