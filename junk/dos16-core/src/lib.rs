//! DOS 16-bit x86 CPU Emulator Core
//!
//! This crate provides the CPU emulation logic in a form that can be unit tested.
//! Run tests with: `cargo test --package dos16-core --features std`

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
extern crate std;

#[cfg(not(feature = "std"))]
extern crate alloc;

#[cfg(feature = "std")]
use std::vec::Vec;
#[cfg(feature = "std")]
use std::vec;

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
#[cfg(not(feature = "std"))]
use alloc::vec;

// CPU Flags
pub const FLAG_CF: u16 = 0x0001;  // Carry
pub const FLAG_PF: u16 = 0x0004;  // Parity
pub const FLAG_AF: u16 = 0x0010;  // Auxiliary carry
pub const FLAG_ZF: u16 = 0x0040;  // Zero
pub const FLAG_SF: u16 = 0x0080;  // Sign
pub const FLAG_TF: u16 = 0x0100;  // Trap
pub const FLAG_IF: u16 = 0x0200;  // Interrupt enable
pub const FLAG_DF: u16 = 0x0400;  // Direction
pub const FLAG_OF: u16 = 0x0800;  // Overflow

/// 16-bit x86 CPU state
#[derive(Clone, Debug, PartialEq, Eq)]
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

impl Default for Cpu16 {
    fn default() -> Self {
        Self::new()
    }
}

impl Cpu16 {
    pub fn new() -> Self {
        Cpu16 {
            ax: 0, bx: 0, cx: 0, dx: 0,
            si: 0, di: 0, bp: 0, sp: 0xFFFE,
            ip: 0x100,
            cs: 0, ds: 0, es: 0, ss: 0,
            flags: 0x0002, // Bit 1 always set
        }
    }

    // Register accessors by index (for ModR/M)
    pub fn get_reg16(&self, idx: u8) -> u16 {
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

    pub fn set_reg16(&mut self, idx: u8, val: u16) {
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

    pub fn get_reg8(&self, idx: u8) -> u8 {
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

    pub fn set_reg8(&mut self, idx: u8, val: u8) {
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

    pub fn get_seg(&self, idx: u8) -> u16 {
        match idx & 3 {
            0 => self.es,
            1 => self.cs,
            2 => self.ss,
            3 => self.ds,
            _ => 0,
        }
    }

    pub fn set_seg(&mut self, idx: u8, val: u16) {
        match idx & 3 {
            0 => self.es = val,
            1 => self.cs = val,
            2 => self.ss = val,
            3 => self.ds = val,
            _ => {}
        }
    }

    // Flag helpers
    pub fn set_flag(&mut self, flag: u16, val: bool) {
        if val { self.flags |= flag; } else { self.flags &= !flag; }
    }

    pub fn get_flag(&self, flag: u16) -> bool {
        (self.flags & flag) != 0
    }

    pub fn update_flags_logic8(&mut self, result: u8) {
        self.set_flag(FLAG_ZF, result == 0);
        self.set_flag(FLAG_SF, (result & 0x80) != 0);
        self.set_flag(FLAG_PF, (result.count_ones() & 1) == 0);
        self.set_flag(FLAG_CF, false);
        self.set_flag(FLAG_OF, false);
    }

    pub fn update_flags_logic16(&mut self, result: u16) {
        self.set_flag(FLAG_ZF, result == 0);
        self.set_flag(FLAG_SF, (result & 0x8000) != 0);
        self.set_flag(FLAG_PF, ((result as u8).count_ones() & 1) == 0);
        self.set_flag(FLAG_CF, false);
        self.set_flag(FLAG_OF, false);
    }

    pub fn update_flags_add8(&mut self, a: u8, b: u8, result: u16) {
        let r = result as u8;
        self.set_flag(FLAG_ZF, r == 0);
        self.set_flag(FLAG_SF, (r & 0x80) != 0);
        self.set_flag(FLAG_PF, (r.count_ones() & 1) == 0);
        self.set_flag(FLAG_CF, result > 0xFF);
        self.set_flag(FLAG_OF, ((a ^ r) & (b ^ r) & 0x80) != 0);
        self.set_flag(FLAG_AF, ((a ^ b ^ r) & 0x10) != 0);
    }

    pub fn update_flags_add16(&mut self, a: u16, b: u16, result: u32) {
        let r = result as u16;
        self.set_flag(FLAG_ZF, r == 0);
        self.set_flag(FLAG_SF, (r & 0x8000) != 0);
        self.set_flag(FLAG_PF, ((r as u8).count_ones() & 1) == 0);
        self.set_flag(FLAG_CF, result > 0xFFFF);
        self.set_flag(FLAG_OF, ((a ^ r) & (b ^ r) & 0x8000) != 0);
        self.set_flag(FLAG_AF, ((a ^ b ^ r) & 0x10) != 0);
    }

    pub fn update_flags_sub8(&mut self, a: u8, b: u8, result: u16) {
        let r = result as u8;
        self.set_flag(FLAG_ZF, r == 0);
        self.set_flag(FLAG_SF, (r & 0x80) != 0);
        self.set_flag(FLAG_PF, (r.count_ones() & 1) == 0);
        self.set_flag(FLAG_CF, a < b);
        self.set_flag(FLAG_OF, ((a ^ b) & (a ^ r) & 0x80) != 0);
        self.set_flag(FLAG_AF, (a & 0x0F) < (b & 0x0F));
    }

    pub fn update_flags_sub16(&mut self, a: u16, b: u16, result: u32) {
        let r = result as u16;
        self.set_flag(FLAG_ZF, r == 0);
        self.set_flag(FLAG_SF, (r & 0x8000) != 0);
        self.set_flag(FLAG_PF, ((r as u8).count_ones() & 1) == 0);
        self.set_flag(FLAG_CF, a < b);
        self.set_flag(FLAG_OF, ((a ^ b) & (a ^ r) & 0x8000) != 0);
        self.set_flag(FLAG_AF, (a & 0x0F) < (b & 0x0F));
    }
}

/// Execution result for a single step
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepResult {
    Continue,
    Halt,
    Interrupt(u8),
    UnknownOpcode(u8),
}

/// Minimal CPU emulator for testing
/// Contains CPU state plus memory, no I/O or console dependencies
pub struct Emulator {
    pub cpu: Cpu16,
    pub memory: Vec<u8>,
    seg_override: Option<u16>,
}

impl Emulator {
    pub fn new() -> Self {
        Self::with_memory_size(1024 * 1024) // 1MB default
    }

    pub fn with_memory_size(size: usize) -> Self {
        Emulator {
            cpu: Cpu16::new(),
            memory: vec![0u8; size],
            seg_override: None,
        }
    }

    /// Load a COM file at offset 0x100
    pub fn load_com(&mut self, data: &[u8]) {
        let load_addr = 0x100usize;
        let copy_len = core::cmp::min(data.len(), self.memory.len() - load_addr);
        self.memory[load_addr..load_addr + copy_len].copy_from_slice(&data[..copy_len]);
        self.cpu.ip = 0x100;
        self.cpu.cs = 0;
        self.cpu.ds = 0;
        self.cpu.es = 0;
        self.cpu.ss = 0;
    }

    /// Load an MZ EXE file
    pub fn load_exe(&mut self, data: &[u8]) -> Result<(), &'static str> {
        if data.len() < 28 {
            return Err("EXE too small");
        }
        if data[0] != b'M' || data[1] != b'Z' {
            return Err("Not an MZ executable");
        }

        // MZ header parsing
        let last_page_size = u16::from_le_bytes([data[2], data[3]]);
        let page_count = u16::from_le_bytes([data[4], data[5]]);
        let reloc_count = u16::from_le_bytes([data[6], data[7]]) as usize;
        let header_paras = u16::from_le_bytes([data[8], data[9]]) as usize;
        let init_ss = u16::from_le_bytes([data[14], data[15]]);
        let init_sp = u16::from_le_bytes([data[16], data[17]]);
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
        let load_seg: u16 = 0x0010;
        let load_addr = (load_seg as usize) << 4;

        // Copy code
        if header_size < data.len() {
            let code_start = header_size;
            let code_end = core::cmp::min(code_start + code_size, data.len());
            let copy_len = core::cmp::min(code_end - code_start, self.memory.len() - load_addr);
            self.memory[load_addr..load_addr + copy_len]
                .copy_from_slice(&data[code_start..code_start + copy_len]);
        }

        // Apply relocations
        for i in 0..reloc_count {
            let rel_off = reloc_offset + i * 4;
            if rel_off + 4 <= data.len() {
                let off = u16::from_le_bytes([data[rel_off], data[rel_off + 1]]) as usize;
                let seg = u16::from_le_bytes([data[rel_off + 2], data[rel_off + 3]]) as usize;
                let addr = load_addr + (seg << 4) + off;
                if addr + 2 <= self.memory.len() {
                    let val = u16::from_le_bytes([self.memory[addr], self.memory[addr + 1]]);
                    let new_val = val.wrapping_add(load_seg);
                    self.memory[addr] = new_val as u8;
                    self.memory[addr + 1] = (new_val >> 8) as u8;
                }
            }
        }

        // Set up registers
        self.cpu.cs = load_seg.wrapping_add(init_cs);
        self.cpu.ip = init_ip;
        self.cpu.ss = load_seg.wrapping_add(init_ss);
        self.cpu.sp = init_sp;
        self.cpu.ds = load_seg;
        self.cpu.es = load_seg;

        Ok(())
    }

    /// Load raw code at a specific segment:offset
    pub fn load_code_at(&mut self, seg: u16, off: u16, code: &[u8]) {
        let addr = self.lin(seg, off);
        for (i, &byte) in code.iter().enumerate() {
            if addr + i < self.memory.len() {
                self.memory[addr + i] = byte;
            }
        }
    }

    // Linear address calculation
    pub fn lin(&self, seg: u16, off: u16) -> usize {
        ((seg as usize) << 4).wrapping_add(off as usize) & 0xFFFFF
    }

    // Memory access
    pub fn read_u8(&self, seg: u16, off: u16) -> u8 {
        let addr = self.lin(seg, off);
        if addr < self.memory.len() { self.memory[addr] } else { 0 }
    }

    pub fn read_u16(&self, seg: u16, off: u16) -> u16 {
        let lo = self.read_u8(seg, off) as u16;
        let hi = self.read_u8(seg, off.wrapping_add(1)) as u16;
        lo | (hi << 8)
    }

    pub fn write_u8(&mut self, seg: u16, off: u16, val: u8) {
        let addr = self.lin(seg, off);
        if addr < self.memory.len() { self.memory[addr] = val; }
    }

    pub fn write_u16(&mut self, seg: u16, off: u16, val: u16) {
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

    // ModR/M decoding - returns (reg field, effective address, rm field for register mode, is_memory)
    fn decode_modrm(&mut self, wide: bool) -> (u8, u16, u8, bool) {
        let modrm = self.fetch_u8();
        let mode = (modrm >> 6) & 3;
        let reg = (modrm >> 3) & 7;
        let rm = modrm & 7;

        if mode == 3 {
            // Register mode - ea holds register value, rm is register index
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

        (reg, ea, rm, true)
    }

    // Read operand from ModR/M
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

    /// Execute one instruction
    pub fn step(&mut self) -> StepResult {
        self.step_inner(false)
    }

    fn step_inner(&mut self, has_seg_override: bool) -> StepResult {
        // Only clear seg_override at the start of a new instruction, not when processing prefix
        if !has_seg_override {
            self.seg_override = None;
        }

        let opcode = self.fetch_u8();

        match opcode {
            // Segment override prefixes - set override and continue processing
            0x26 => { self.seg_override = Some(self.cpu.es); return self.step_inner(true); }
            0x2E => { self.seg_override = Some(self.cpu.cs); return self.step_inner(true); }
            0x36 => { self.seg_override = Some(self.cpu.ss); return self.step_inner(true); }
            0x3E => { self.seg_override = Some(self.cpu.ds); return self.step_inner(true); }

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

            // PUSH SS
            0x16 => self.push16(self.cpu.ss),
            // POP SS
            0x17 => self.cpu.ss = self.pop16(),

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

            // INC r16
            0x40..=0x47 => {
                let idx = opcode - 0x40;
                let val = self.cpu.get_reg16(idx);
                let cf = self.cpu.get_flag(FLAG_CF);
                let result = (val as u32).wrapping_add(1);
                self.cpu.update_flags_add16(val, 1, result);
                self.cpu.set_flag(FLAG_CF, cf); // INC doesn't affect CF
                self.cpu.set_reg16(idx, result as u16);
            }

            // DEC r16
            0x48..=0x4F => {
                let idx = opcode - 0x48;
                let val = self.cpu.get_reg16(idx);
                let cf = self.cpu.get_flag(FLAG_CF);
                let result = (val as u32).wrapping_sub(1);
                self.cpu.update_flags_sub16(val, 1, result);
                self.cpu.set_flag(FLAG_CF, cf); // DEC doesn't affect CF
                self.cpu.set_reg16(idx, result as u16);
            }

            // PUSH r16
            0x50..=0x57 => {
                let val = self.cpu.get_reg16(opcode - 0x50);
                self.push16(val);
            }

            // POP r16
            0x58..=0x5F => {
                let val = self.pop16();
                self.cpu.set_reg16(opcode - 0x58, val);
            }

            // Jcc rel8 (conditional jumps)
            0x70 => { let rel = self.fetch_i8(); if self.cpu.get_flag(FLAG_OF) { self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16); } } // JO
            0x71 => { let rel = self.fetch_i8(); if !self.cpu.get_flag(FLAG_OF) { self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16); } } // JNO
            0x72 => { let rel = self.fetch_i8(); if self.cpu.get_flag(FLAG_CF) { self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16); } } // JB/JC/JNAE
            0x73 => { let rel = self.fetch_i8(); if !self.cpu.get_flag(FLAG_CF) { self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16); } } // JNB/JNC/JAE
            0x74 => { let rel = self.fetch_i8(); if self.cpu.get_flag(FLAG_ZF) { self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16); } } // JE/JZ
            0x75 => { let rel = self.fetch_i8(); if !self.cpu.get_flag(FLAG_ZF) { self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16); } } // JNE/JNZ
            0x76 => { let rel = self.fetch_i8(); if self.cpu.get_flag(FLAG_CF) || self.cpu.get_flag(FLAG_ZF) { self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16); } } // JBE/JNA
            0x77 => { let rel = self.fetch_i8(); if !self.cpu.get_flag(FLAG_CF) && !self.cpu.get_flag(FLAG_ZF) { self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16); } } // JA/JNBE
            0x78 => { let rel = self.fetch_i8(); if self.cpu.get_flag(FLAG_SF) { self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16); } } // JS
            0x79 => { let rel = self.fetch_i8(); if !self.cpu.get_flag(FLAG_SF) { self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16); } } // JNS
            0x7A => { let rel = self.fetch_i8(); if self.cpu.get_flag(FLAG_PF) { self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16); } } // JP/JPE
            0x7B => { let rel = self.fetch_i8(); if !self.cpu.get_flag(FLAG_PF) { self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16); } } // JNP/JPO
            0x7C => { // JL/JNGE
                let rel = self.fetch_i8();
                let sf = self.cpu.get_flag(FLAG_SF);
                let of = self.cpu.get_flag(FLAG_OF);
                if sf != of { self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16); }
            }
            0x7D => { // JGE/JNL
                let rel = self.fetch_i8();
                let sf = self.cpu.get_flag(FLAG_SF);
                let of = self.cpu.get_flag(FLAG_OF);
                if sf == of { self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16); }
            }
            0x7E => { // JLE/JNG
                let rel = self.fetch_i8();
                let zf = self.cpu.get_flag(FLAG_ZF);
                let sf = self.cpu.get_flag(FLAG_SF);
                let of = self.cpu.get_flag(FLAG_OF);
                if zf || sf != of { self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16); }
            }
            0x7F => { // JG/JNLE
                let rel = self.fetch_i8();
                let zf = self.cpu.get_flag(FLAG_ZF);
                let sf = self.cpu.get_flag(FLAG_SF);
                let of = self.cpu.get_flag(FLAG_OF);
                if !zf && sf == of { self.cpu.ip = self.cpu.ip.wrapping_add(rel as i16 as u16); }
            }

            // MOV r8, imm8
            0xB0..=0xB7 => {
                let imm = self.fetch_u8();
                self.cpu.set_reg8(opcode - 0xB0, imm);
            }
            // MOV r16, imm16
            0xB8..=0xBF => {
                let imm = self.fetch_u16();
                self.cpu.set_reg16(opcode - 0xB8, imm);
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
                let (sreg, _, ea, rm, is_mem) = self.read_rm16();
                let val = self.cpu.get_seg(sreg);
                self.write_rm16(ea, is_mem, rm, val);
            }
            // LEA r16, m
            0x8D => {
                let (reg, ea, _, _, _) = self.read_rm16();
                self.cpu.set_reg16(reg, ea);
            }
            // MOV Sreg, r/m16
            0x8E => {
                let (sreg, val, _, _, _) = self.read_rm16();
                self.cpu.set_seg(sreg, val);
            }

            // NOP
            0x90 => {}

            // XCHG AX, r16
            0x91..=0x97 => {
                let idx = opcode - 0x90;
                let tmp = self.cpu.ax;
                self.cpu.ax = self.cpu.get_reg16(idx);
                self.cpu.set_reg16(idx, tmp);
            }

            // CBW
            0x98 => {
                let al = self.cpu.ax as u8;
                self.cpu.ax = (al as i8 as i16) as u16;
            }
            // CWD
            0x99 => {
                if (self.cpu.ax as i16) < 0 {
                    self.cpu.dx = 0xFFFF;
                } else {
                    self.cpu.dx = 0;
                }
            }

            // CALL far
            0x9A => {
                let off = self.fetch_u16();
                let seg = self.fetch_u16();
                self.push16(self.cpu.cs);
                self.push16(self.cpu.ip);
                self.cpu.cs = seg;
                self.cpu.ip = off;
            }

            // PUSHF
            0x9C => self.push16(self.cpu.flags),
            // POPF
            0x9D => self.cpu.flags = (self.pop16() & 0x0FD5) | 0x0002,

            // SAHF
            0x9E => {
                let ah = (self.cpu.ax >> 8) as u8;
                self.cpu.flags = (self.cpu.flags & 0xFF00) | (ah as u16);
            }
            // LAHF
            0x9F => {
                let flags_lo = self.cpu.flags as u8;
                self.cpu.ax = (self.cpu.ax & 0x00FF) | ((flags_lo as u16) << 8);
            }

            // MOV AL, [moffs8]
            0xA0 => {
                let off = self.fetch_u16();
                let seg = self.get_seg(self.cpu.ds);
                let val = self.read_u8(seg, off);
                self.cpu.ax = (self.cpu.ax & 0xFF00) | val as u16;
            }
            // MOV AX, [moffs16]
            0xA1 => {
                let off = self.fetch_u16();
                let seg = self.get_seg(self.cpu.ds);
                let val = self.read_u16(seg, off);
                self.cpu.ax = val;
            }
            // MOV [moffs8], AL
            0xA2 => {
                let off = self.fetch_u16();
                let seg = self.get_seg(self.cpu.ds);
                self.write_u8(seg, off, self.cpu.ax as u8);
            }
            // MOV [moffs16], AX
            0xA3 => {
                let off = self.fetch_u16();
                let seg = self.get_seg(self.cpu.ds);
                self.write_u16(seg, off, self.cpu.ax);
            }

            // MOVSB
            0xA4 => {
                let src_seg = self.get_seg(self.cpu.ds);
                let val = self.read_u8(src_seg, self.cpu.si);
                self.write_u8(self.cpu.es, self.cpu.di, val);
                let delta: u16 = if self.cpu.get_flag(FLAG_DF) { 0xFFFF } else { 1 };
                self.cpu.si = self.cpu.si.wrapping_add(delta);
                self.cpu.di = self.cpu.di.wrapping_add(delta);
            }
            // MOVSW
            0xA5 => {
                let src_seg = self.get_seg(self.cpu.ds);
                let val = self.read_u16(src_seg, self.cpu.si);
                self.write_u16(self.cpu.es, self.cpu.di, val);
                let delta: u16 = if self.cpu.get_flag(FLAG_DF) { 0xFFFE } else { 2 };
                self.cpu.si = self.cpu.si.wrapping_add(delta);
                self.cpu.di = self.cpu.di.wrapping_add(delta);
            }

            // STOSB
            0xAA => {
                self.write_u8(self.cpu.es, self.cpu.di, self.cpu.ax as u8);
                let delta: u16 = if self.cpu.get_flag(FLAG_DF) { 0xFFFF } else { 1 };
                self.cpu.di = self.cpu.di.wrapping_add(delta);
            }
            // STOSW
            0xAB => {
                self.write_u16(self.cpu.es, self.cpu.di, self.cpu.ax);
                let delta: u16 = if self.cpu.get_flag(FLAG_DF) { 0xFFFE } else { 2 };
                self.cpu.di = self.cpu.di.wrapping_add(delta);
            }

            // LODSB
            0xAC => {
                let src_seg = self.get_seg(self.cpu.ds);
                let val = self.read_u8(src_seg, self.cpu.si);
                self.cpu.ax = (self.cpu.ax & 0xFF00) | val as u16;
                let delta: u16 = if self.cpu.get_flag(FLAG_DF) { 0xFFFF } else { 1 };
                self.cpu.si = self.cpu.si.wrapping_add(delta);
            }
            // LODSW
            0xAD => {
                let src_seg = self.get_seg(self.cpu.ds);
                let val = self.read_u16(src_seg, self.cpu.si);
                self.cpu.ax = val;
                let delta: u16 = if self.cpu.get_flag(FLAG_DF) { 0xFFFE } else { 2 };
                self.cpu.si = self.cpu.si.wrapping_add(delta);
            }

            // RET near
            0xC3 => {
                self.cpu.ip = self.pop16();
            }

            // LES r16, m16:16
            0xC4 => {
                let (reg, ea, _, _, is_mem) = self.read_rm16();
                if is_mem {
                    let seg = self.get_seg(self.cpu.ds);
                    let off = self.read_u16(seg, ea);
                    let new_es = self.read_u16(seg, ea.wrapping_add(2));
                    self.cpu.set_reg16(reg, off);
                    self.cpu.es = new_es;
                }
            }
            // LDS r16, m16:16
            0xC5 => {
                let (reg, ea, _, _, is_mem) = self.read_rm16();
                if is_mem {
                    let seg = self.get_seg(self.cpu.ds);
                    let off = self.read_u16(seg, ea);
                    let new_ds = self.read_u16(seg, ea.wrapping_add(2));
                    self.cpu.set_reg16(reg, off);
                    self.cpu.ds = new_ds;
                }
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

            // RET near imm16
            0xC2 => {
                let imm = self.fetch_u16();
                self.cpu.ip = self.pop16();
                self.cpu.sp = self.cpu.sp.wrapping_add(imm);
            }

            // RETF
            0xCB => {
                self.cpu.ip = self.pop16();
                self.cpu.cs = self.pop16();
            }
            // RETF imm16
            0xCA => {
                let imm = self.fetch_u16();
                self.cpu.ip = self.pop16();
                self.cpu.cs = self.pop16();
                self.cpu.sp = self.cpu.sp.wrapping_add(imm);
            }

            // INT n
            0xCD => {
                let int_num = self.fetch_u8();
                return StepResult::Interrupt(int_num);
            }

            // IRET
            0xCF => {
                self.cpu.ip = self.pop16();
                self.cpu.cs = self.pop16();
                self.cpu.flags = self.pop16();
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

            // LOCK prefix
            0xF0 => { return self.step_inner(has_seg_override); }

            // REP/REPNE prefix
            0xF2 | 0xF3 => {
                let repz = opcode == 0xF3;
                self.exec_rep(repz);
            }

            // HLT
            0xF4 => return StepResult::Halt,

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

            // Group 5 (INC/DEC/CALL/JMP indirect)
            0xFF => {
                self.exec_group5();
            }

            _ => return StepResult::UnknownOpcode(opcode),
        }

        StepResult::Continue
    }

    fn exec_rep(&mut self, repz: bool) {
        if self.cpu.cx == 0 {
            return;
        }

        let op = self.fetch_u8();

        while self.cpu.cx > 0 {
            self.cpu.cx = self.cpu.cx.wrapping_sub(1);

            match op {
                // MOVSB
                0xA4 => {
                    let src_seg = self.get_seg(self.cpu.ds);
                    let val = self.read_u8(src_seg, self.cpu.si);
                    self.write_u8(self.cpu.es, self.cpu.di, val);
                    let delta: u16 = if self.cpu.get_flag(FLAG_DF) { 0xFFFF } else { 1 };
                    self.cpu.si = self.cpu.si.wrapping_add(delta);
                    self.cpu.di = self.cpu.di.wrapping_add(delta);
                }
                // MOVSW
                0xA5 => {
                    let src_seg = self.get_seg(self.cpu.ds);
                    let val = self.read_u16(src_seg, self.cpu.si);
                    self.write_u16(self.cpu.es, self.cpu.di, val);
                    let delta: u16 = if self.cpu.get_flag(FLAG_DF) { 0xFFFE } else { 2 };
                    self.cpu.si = self.cpu.si.wrapping_add(delta);
                    self.cpu.di = self.cpu.di.wrapping_add(delta);
                }
                // STOSB
                0xAA => {
                    self.write_u8(self.cpu.es, self.cpu.di, self.cpu.ax as u8);
                    let delta: u16 = if self.cpu.get_flag(FLAG_DF) { 0xFFFF } else { 1 };
                    self.cpu.di = self.cpu.di.wrapping_add(delta);
                }
                // STOSW
                0xAB => {
                    self.write_u16(self.cpu.es, self.cpu.di, self.cpu.ax);
                    let delta: u16 = if self.cpu.get_flag(FLAG_DF) { 0xFFFE } else { 2 };
                    self.cpu.di = self.cpu.di.wrapping_add(delta);
                }
                // CMPSB
                0xA6 => {
                    let src_seg = self.get_seg(self.cpu.ds);
                    let a = self.read_u8(src_seg, self.cpu.si);
                    let b = self.read_u8(self.cpu.es, self.cpu.di);
                    let result = (a as u16).wrapping_sub(b as u16);
                    self.cpu.update_flags_sub8(a, b, result);
                    let delta: u16 = if self.cpu.get_flag(FLAG_DF) { 0xFFFF } else { 1 };
                    self.cpu.si = self.cpu.si.wrapping_add(delta);
                    self.cpu.di = self.cpu.di.wrapping_add(delta);

                    let zf = self.cpu.get_flag(FLAG_ZF);
                    if (repz && !zf) || (!repz && zf) {
                        break;
                    }
                }
                // SCASB
                0xAE => {
                    let b = self.read_u8(self.cpu.es, self.cpu.di);
                    let a = self.cpu.ax as u8;
                    let result = (a as u16).wrapping_sub(b as u16);
                    self.cpu.update_flags_sub8(a, b, result);
                    let delta: u16 = if self.cpu.get_flag(FLAG_DF) { 0xFFFF } else { 1 };
                    self.cpu.di = self.cpu.di.wrapping_add(delta);

                    let zf = self.cpu.get_flag(FLAG_ZF);
                    if (repz && !zf) || (!repz && zf) {
                        break;
                    }
                }
                _ => break,
            }
        }
    }

    fn exec_group5(&mut self) {
        let (op, val, ea, rm, is_mem) = self.read_rm16();

        match op {
            // INC r/m16
            0 => {
                let cf = self.cpu.get_flag(FLAG_CF);
                let result = (val as u32).wrapping_add(1);
                self.cpu.update_flags_add16(val, 1, result);
                self.cpu.set_flag(FLAG_CF, cf);
                self.write_rm16(ea, is_mem, rm, result as u16);
            }
            // DEC r/m16
            1 => {
                let cf = self.cpu.get_flag(FLAG_CF);
                let result = (val as u32).wrapping_sub(1);
                self.cpu.update_flags_sub16(val, 1, result);
                self.cpu.set_flag(FLAG_CF, cf);
                self.write_rm16(ea, is_mem, rm, result as u16);
            }
            // CALL r/m16 (near indirect)
            2 => {
                self.push16(self.cpu.ip);
                self.cpu.ip = val;
            }
            // CALL m16:16 (far indirect)
            3 => {
                if is_mem {
                    let seg = self.get_seg(self.cpu.ds);
                    let new_ip = self.read_u16(seg, ea);
                    let new_cs = self.read_u16(seg, ea.wrapping_add(2));
                    self.push16(self.cpu.cs);
                    self.push16(self.cpu.ip);
                    self.cpu.cs = new_cs;
                    self.cpu.ip = new_ip;
                }
            }
            // JMP r/m16 (near indirect)
            4 => {
                self.cpu.ip = val;
            }
            // JMP m16:16 (far indirect)
            5 => {
                if is_mem {
                    let seg = self.get_seg(self.cpu.ds);
                    let new_ip = self.read_u16(seg, ea);
                    let new_cs = self.read_u16(seg, ea.wrapping_add(2));
                    self.cpu.cs = new_cs;
                    self.cpu.ip = new_ip;
                }
            }
            // PUSH r/m16
            6 => {
                self.push16(val);
            }
            _ => {}
        }
    }

    /// Run until halt, interrupt, or max_steps reached
    pub fn run(&mut self, max_steps: usize) -> (StepResult, usize) {
        let mut steps = 0;
        loop {
            if steps >= max_steps {
                return (StepResult::Continue, steps);
            }
            let result = self.step();
            steps += 1;
            match result {
                StepResult::Continue => continue,
                other => return (other, steps),
            }
        }
    }
}

impl Default for Emulator {
    fn default() -> Self {
        Self::new()
    }
}

// Tests
#[cfg(all(test, feature = "std"))]
mod tests;
