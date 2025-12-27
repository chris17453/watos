//! 16-bit x86 CPU emulator
//!
//! Implements the 8086/8088 instruction set.

/// CPU Flags
pub mod flags {
    pub const CF: u16 = 0x0001;  // Carry
    pub const PF: u16 = 0x0004;  // Parity
    pub const AF: u16 = 0x0010;  // Auxiliary carry
    pub const ZF: u16 = 0x0040;  // Zero
    pub const SF: u16 = 0x0080;  // Sign
    pub const TF: u16 = 0x0100;  // Trap
    pub const IF: u16 = 0x0200;  // Interrupt enable
    pub const DF: u16 = 0x0400;  // Direction
    pub const OF: u16 = 0x0800;  // Overflow
}

/// 16-bit x86 CPU state
#[derive(Clone, Debug)]
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

    // Interrupt flag for pending interrupt
    pub pending_int: Option<u8>,
}

impl Cpu16 {
    /// Create a new CPU with default state
    pub fn new() -> Self {
        Cpu16 {
            ax: 0, bx: 0, cx: 0, dx: 0,
            si: 0, di: 0, bp: 0, sp: 0xFFFE,
            ip: 0x100,  // COM files start at CS:0100
            cs: 0, ds: 0, es: 0, ss: 0,
            flags: 0x0002,  // Bit 1 always set
            pending_int: None,
        }
    }

    // 8-bit register accessors

    /// Get AL (low byte of AX)
    #[inline]
    pub fn al(&self) -> u8 { self.ax as u8 }

    /// Get AH (high byte of AX)
    #[inline]
    pub fn ah(&self) -> u8 { (self.ax >> 8) as u8 }

    /// Set AL
    #[inline]
    pub fn set_al(&mut self, val: u8) { self.ax = (self.ax & 0xFF00) | val as u16; }

    /// Set AH
    #[inline]
    pub fn set_ah(&mut self, val: u8) { self.ax = (self.ax & 0x00FF) | ((val as u16) << 8); }

    /// Get BL
    #[inline]
    pub fn bl(&self) -> u8 { self.bx as u8 }

    /// Get BH
    #[inline]
    pub fn bh(&self) -> u8 { (self.bx >> 8) as u8 }

    /// Set BL
    #[inline]
    pub fn set_bl(&mut self, val: u8) { self.bx = (self.bx & 0xFF00) | val as u16; }

    /// Set BH
    #[inline]
    pub fn set_bh(&mut self, val: u8) { self.bx = (self.bx & 0x00FF) | ((val as u16) << 8); }

    /// Get CL
    #[inline]
    pub fn cl(&self) -> u8 { self.cx as u8 }

    /// Get CH
    #[inline]
    pub fn ch(&self) -> u8 { (self.cx >> 8) as u8 }

    /// Set CL
    #[inline]
    pub fn set_cl(&mut self, val: u8) { self.cx = (self.cx & 0xFF00) | val as u16; }

    /// Set CH
    #[inline]
    pub fn set_ch(&mut self, val: u8) { self.cx = (self.cx & 0x00FF) | ((val as u16) << 8); }

    /// Get DL
    #[inline]
    pub fn dl(&self) -> u8 { self.dx as u8 }

    /// Get DH
    #[inline]
    pub fn dh(&self) -> u8 { (self.dx >> 8) as u8 }

    /// Set DL
    #[inline]
    pub fn set_dl(&mut self, val: u8) { self.dx = (self.dx & 0xFF00) | val as u16; }

    /// Set DH
    #[inline]
    pub fn set_dh(&mut self, val: u8) { self.dx = (self.dx & 0x00FF) | ((val as u16) << 8); }

    // Register by index (ModR/M encoding)

    /// Get 16-bit register by index
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

    /// Set 16-bit register by index
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

    /// Get 8-bit register by index
    pub fn get_reg8(&self, idx: u8) -> u8 {
        match idx & 7 {
            0 => self.al(),
            1 => self.cl(),
            2 => self.dl(),
            3 => self.bl(),
            4 => self.ah(),
            5 => self.ch(),
            6 => self.dh(),
            7 => self.bh(),
            _ => 0,
        }
    }

    /// Set 8-bit register by index
    pub fn set_reg8(&mut self, idx: u8, val: u8) {
        match idx & 7 {
            0 => self.set_al(val),
            1 => self.set_cl(val),
            2 => self.set_dl(val),
            3 => self.set_bl(val),
            4 => self.set_ah(val),
            5 => self.set_ch(val),
            6 => self.set_dh(val),
            7 => self.set_bh(val),
            _ => {}
        }
    }

    /// Get segment register by index
    pub fn get_seg(&self, idx: u8) -> u16 {
        match idx & 3 {
            0 => self.es,
            1 => self.cs,
            2 => self.ss,
            3 => self.ds,
            _ => 0,
        }
    }

    /// Set segment register by index
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

    /// Check if carry flag is set
    #[inline]
    pub fn cf(&self) -> bool { self.flags & flags::CF != 0 }

    /// Check if zero flag is set
    #[inline]
    pub fn zf(&self) -> bool { self.flags & flags::ZF != 0 }

    /// Check if sign flag is set
    #[inline]
    pub fn sf(&self) -> bool { self.flags & flags::SF != 0 }

    /// Check if overflow flag is set
    #[inline]
    pub fn of(&self) -> bool { self.flags & flags::OF != 0 }

    /// Check if direction flag is set
    #[inline]
    pub fn df(&self) -> bool { self.flags & flags::DF != 0 }

    /// Set carry flag
    #[inline]
    pub fn set_cf(&mut self, val: bool) {
        if val { self.flags |= flags::CF; } else { self.flags &= !flags::CF; }
    }

    /// Set zero flag
    #[inline]
    pub fn set_zf(&mut self, val: bool) {
        if val { self.flags |= flags::ZF; } else { self.flags &= !flags::ZF; }
    }

    /// Set sign flag
    #[inline]
    pub fn set_sf(&mut self, val: bool) {
        if val { self.flags |= flags::SF; } else { self.flags &= !flags::SF; }
    }

    /// Set overflow flag
    #[inline]
    pub fn set_of(&mut self, val: bool) {
        if val { self.flags |= flags::OF; } else { self.flags &= !flags::OF; }
    }

    /// Update flags for 8-bit result
    pub fn update_flags8(&mut self, result: u8) {
        self.set_zf(result == 0);
        self.set_sf(result & 0x80 != 0);
        // Parity: count 1 bits in low byte
        let ones = result.count_ones();
        if ones % 2 == 0 {
            self.flags |= flags::PF;
        } else {
            self.flags &= !flags::PF;
        }
    }

    /// Update flags for 16-bit result
    pub fn update_flags16(&mut self, result: u16) {
        self.set_zf(result == 0);
        self.set_sf(result & 0x8000 != 0);
        // Parity: only check low byte
        let ones = (result as u8).count_ones();
        if ones % 2 == 0 {
            self.flags |= flags::PF;
        } else {
            self.flags &= !flags::PF;
        }
    }

    /// Trigger a software interrupt
    pub fn trigger_interrupt(&mut self, vector: u8) {
        self.pending_int = Some(vector);
    }
}

impl Default for Cpu16 {
    fn default() -> Self {
        Self::new()
    }
}
