//! Global Descriptor Table (GDT) with Ring 0 and Ring 3 segments
//!
//! Defines code and data segments for kernel (Ring 0) and user (Ring 3) modes.

use core::arch::asm;
use core::mem::size_of;
use crate::tss;

/// GDT Entry (8 bytes)
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct GdtEntry {
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

    /// Kernel code segment (Ring 0, 64-bit)
    const fn kernel_code() -> Self {
        GdtEntry {
            limit_low: 0xFFFF,
            base_low: 0,
            base_mid: 0,
            access: 0x9A, // Present, Ring 0, Code, Executable, Readable
            granularity: 0xAF, // 64-bit, 4KB pages, limit high nibble
            base_high: 0,
        }
    }

    /// Kernel data segment (Ring 0)
    const fn kernel_data() -> Self {
        GdtEntry {
            limit_low: 0xFFFF,
            base_low: 0,
            base_mid: 0,
            access: 0x92, // Present, Ring 0, Data, Writable
            granularity: 0xCF, // 32-bit, 4KB pages
            base_high: 0,
        }
    }

    /// User code segment (Ring 3, 64-bit)
    const fn user_code() -> Self {
        GdtEntry {
            limit_low: 0xFFFF,
            base_low: 0,
            base_mid: 0,
            access: 0xFA, // Present, Ring 3, Code, Executable, Readable
            granularity: 0xAF, // 64-bit, 4KB pages
            base_high: 0,
        }
    }

    /// User data segment (Ring 3)
    const fn user_data() -> Self {
        GdtEntry {
            limit_low: 0xFFFF,
            base_low: 0,
            base_mid: 0,
            access: 0xF2, // Present, Ring 3, Data, Writable
            granularity: 0xCF, // 32-bit, 4KB pages
            base_high: 0,
        }
    }
}

/// Full GDT structure with TSS
#[repr(C, packed)]
struct Gdt {
    null: GdtEntry,        // 0x00 - Null descriptor
    kernel_code: GdtEntry, // 0x08 - Kernel code (Ring 0)
    kernel_data: GdtEntry, // 0x10 - Kernel data (Ring 0)
    user_code: GdtEntry,   // 0x18 - User code (Ring 3)
    user_data: GdtEntry,   // 0x20 - User data (Ring 3)
    tss_low: u64,          // 0x28 - TSS descriptor low
    tss_high: u64,         // 0x30 - TSS descriptor high
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
    pub const KERNEL_CODE: u16 = 0x08;
    pub const KERNEL_DATA: u16 = 0x10;
    pub const USER_CODE: u16 = 0x18 | 3; // RPL = 3
    pub const USER_DATA: u16 = 0x20 | 3; // RPL = 3
    pub const TSS: u16 = 0x28;
}

/// Initialize and load GDT with TSS
pub fn init() {
    unsafe {
        // Get TSS descriptor and embed in GDT
        let tss_desc = tss::descriptor();
        let (low, high) = tss_desc.as_u64_pair();
        GDT.tss_low = low;
        GDT.tss_high = high;

        // Create GDT pointer
        let gdt_ptr = GdtPointer {
            limit: (size_of::<Gdt>() - 1) as u16,
            base: &GDT as *const _ as u64,
        };

        // Load GDT
        asm!(
            "lgdt [{}]",
            in(reg) &gdt_ptr,
            options(nostack, preserves_flags)
        );

        // Reload segment registers
        asm!(
            // Far jump to reload CS
            "push {kernel_cs}",
            "lea rax, [rip + 2f]",
            "push rax",
            "retfq",
            "2:",
            // Load data segments
            "mov ax, {kernel_ds}",
            "mov ds, ax",
            "mov es, ax",
            "mov fs, ax",
            "mov gs, ax",
            "mov ss, ax",
            kernel_cs = const selectors::KERNEL_CODE as u64,
            kernel_ds = const selectors::KERNEL_DATA,
            out("rax") _,
        );

        // Load Task Register
        asm!(
            "ltr ax",
            in("ax") selectors::TSS,
            options(nostack)
        );

        crate::serial_write(b"[GDT] Loaded, CS=0x");
        crate::serial_hex_byte((get_cs() & 0xFF) as u8);
        crate::serial_write(b"\r\n");
    }
}

/// Get current CS selector
pub fn get_cs() -> u16 {
    let cs: u16;
    unsafe {
        asm!("mov {0:x}, cs", out(reg) cs, options(nostack, preserves_flags));
    }
    cs
}
