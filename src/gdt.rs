//! Global Descriptor Table (GDT) with Ring 0 and Ring 3 segments
//! 
//! The GDT defines code and data segments for both kernel (Ring 0)
//! and user (Ring 3) modes. Required for privilege level transitions.

use core::mem::size_of;
use crate::tss::TssDescriptor;

/// GDT Entry (8 bytes)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct GdtEntry {
    limit_low: u16,
    base_low: u16,
    base_mid: u8,
    access: u8,
    granularity: u8,
    base_high: u8,
}

impl GdtEntry {
    const fn null() -> Self {
        GdtEntry {
            limit_low: 0,
            base_low: 0,
            base_mid: 0,
            access: 0,
            granularity: 0,
            base_high: 0,
        }
    }

    const fn new(base: u32, limit: u32, access: u8, granularity: u8) -> Self {
        GdtEntry {
            limit_low: (limit & 0xFFFF) as u16,
            base_low: (base & 0xFFFF) as u16,
            base_mid: ((base >> 16) & 0xFF) as u8,
            access,
            granularity: ((limit >> 16) & 0x0F) as u8 | (granularity & 0xF0),
            base_high: ((base >> 24) & 0xFF) as u8,
        }
    }

    /// Kernel code segment (Ring 0)
    const fn kernel_code() -> Self {
        // Base=0, Limit=0xFFFFF, Access=0x9A (Present, Ring 0, Code, Executable, Readable)
        // Granularity=0xA0 (64-bit, 4KB pages)
        Self::new(0, 0xFFFFF, 0x9A, 0xA0)
    }

    /// Kernel data segment (Ring 0)
    const fn kernel_data() -> Self {
        // Base=0, Limit=0xFFFFF, Access=0x92 (Present, Ring 0, Data, Writable)
        // Granularity=0xC0 (4KB pages)
        Self::new(0, 0xFFFFF, 0x92, 0xC0)
    }

    /// User code segment (Ring 3)
    const fn user_code() -> Self {
        // Base=0, Limit=0xFFFFF, Access=0xFA (Present, Ring 3, Code, Executable, Readable)
        // Granularity=0xA0 (64-bit, 4KB pages)
        Self::new(0, 0xFFFFF, 0xFA, 0xA0)
    }

    /// User data segment (Ring 3)
    const fn user_data() -> Self {
        // Base=0, Limit=0xFFFFF, Access=0xF2 (Present, Ring 3, Data, Writable)
        // Granularity=0xC0 (4KB pages)
        Self::new(0, 0xFFFFF, 0xF2, 0xC0)
    }
}

/// GDT with kernel and user segments
#[repr(C, packed)]
pub struct Gdt {
    null: GdtEntry,           // 0x00 - Null descriptor (required)
    kernel_code: GdtEntry,    // 0x08 - Kernel code (Ring 0)
    kernel_data: GdtEntry,    // 0x10 - Kernel data (Ring 0)
    user_code: GdtEntry,      // 0x18 - User code (Ring 3)
    user_data: GdtEntry,      // 0x20 - User data (Ring 3)
    tss_low: u64,             // 0x28 - TSS descriptor (16 bytes)
    tss_high: u64,
}

impl Gdt {
    const fn new() -> Self {
        Gdt {
            null: GdtEntry::null(),
            kernel_code: GdtEntry::kernel_code(),
            kernel_data: GdtEntry::kernel_data(),
            user_code: GdtEntry::user_code(),
            user_data: GdtEntry::user_data(),
            tss_low: 0,
            tss_high: 0,
        }
    }

    /// Set TSS descriptor in GDT
    unsafe fn set_tss(&mut self, tss_desc: TssDescriptor) {
        let tss_bytes = core::slice::from_raw_parts(
            &tss_desc as *const _ as *const u64,
            2
        );
        self.tss_low = tss_bytes[0];
        self.tss_high = tss_bytes[1];
    }
}

/// GDT Pointer for LGDT instruction
#[repr(C, packed)]
struct GdtPointer {
    limit: u16,
    base: u64,
}

/// Global GDT instance
static mut GDT: Gdt = Gdt::new();

/// Segment selectors
pub mod selectors {
    /// Kernel code selector (0x08, Ring 0)
    pub const KERNEL_CODE: u16 = 0x08;
    
    /// Kernel data selector (0x10, Ring 0)
    pub const KERNEL_DATA: u16 = 0x10;
    
    /// User code selector (0x18 | 3 = 0x1B, Ring 3)
    pub const USER_CODE: u16 = 0x18 | 3;
    
    /// User data selector (0x20 | 3 = 0x23, Ring 3)
    pub const USER_DATA: u16 = 0x20 | 3;
    
    /// TSS selector (0x28, Ring 0)
    pub const TSS: u16 = 0x28;
}

/// Initialize and load GDT with TSS
pub fn init(tss_desc: TssDescriptor) {
    unsafe {
        // Set TSS in GDT
        GDT.set_tss(tss_desc);

        // Create GDT pointer
        let gdt_ptr = GdtPointer {
            limit: (size_of::<Gdt>() - 1) as u16,
            base: &GDT as *const _ as u64,
        };

        // Load GDT
        core::arch::asm!(
            "lgdt [{}]",
            in(reg) &gdt_ptr,
            options(nostack, preserves_flags)
        );

        // Reload segment registers with kernel segments
        core::arch::asm!(
            // Load CS with kernel code segment (far jump)
            "push {kernel_code}",
            "lea rax, [rip + 2f]",
            "push rax",
            "retfq",
            "2:",
            
            // Load other segments with kernel data segment
            "mov {tmp:x}, {kernel_data}",
            "mov ds, {tmp:x}",
            "mov es, {tmp:x}",
            "mov fs, {tmp:x}",
            "mov gs, {tmp:x}",
            "mov ss, {tmp:x}",
            
            kernel_code = in(reg) selectors::KERNEL_CODE as u64,
            kernel_data = const selectors::KERNEL_DATA as u64,
            tmp = out(reg) _,
            out("rax") _,
        );

        // Load Task Register with TSS selector
        core::arch::asm!(
            "ltr ax",
            in("ax") selectors::TSS,
        );
    }
}

/// Get current CS selector (for debugging)
pub fn get_cs() -> u16 {
    let cs: u16;
    unsafe {
        core::arch::asm!(
            "mov {0:x}, cs",
            out(reg) cs,
            options(nostack, preserves_flags)
        );
    }
    cs
}
