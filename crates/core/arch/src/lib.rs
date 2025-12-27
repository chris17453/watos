//! WATOS x86_64 Architecture Support
//!
//! This crate provides low-level CPU and architecture support:
//! - GDT (Global Descriptor Table) with Ring 0/3 segments
//! - TSS (Task State Segment) for privilege transitions
//! - IDT (Interrupt Descriptor Table) with exception handlers
//! - PIC (8259 Programmable Interrupt Controller)
//! - Port I/O primitives

#![no_std]

pub mod port;
pub mod gdt;
pub mod tss;
pub mod idt;
pub mod exceptions;
pub mod pic;
pub mod rtc;

/// Serial port for debug output (COM1)
pub const SERIAL_PORT: u16 = 0x3F8;

/// Debug output to serial port
#[inline]
pub unsafe fn serial_write(s: &[u8]) {
    for &byte in s {
        // Simple busy-wait
        for _ in 0..100 {
            core::arch::asm!("nop", options(nostack));
        }
        port::outb(SERIAL_PORT, byte);
    }
}

/// Debug output hex byte
#[inline]
pub unsafe fn serial_hex_byte(val: u8) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    port::outb(SERIAL_PORT, HEX[(val >> 4) as usize]);
    port::outb(SERIAL_PORT, HEX[(val & 0xF) as usize]);
}

/// Debug output hex u64
#[inline]
pub unsafe fn serial_hex(val: u64) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for i in (0..16).rev() {
        let nibble = ((val >> (i * 4)) & 0xF) as usize;
        port::outb(SERIAL_PORT, HEX[nibble]);
    }
}

/// Initialize all architecture components
///
/// Call order matters:
/// 1. TSS (needs stack address)
/// 2. GDT (needs TSS descriptor)
/// 3. IDT (needs GDT selectors)
/// 4. PIC (can be initialized anytime)
pub fn init(kernel_stack: u64) {
    unsafe {
        serial_write(b"[ARCH] Initializing x86_64 architecture...\r\n");

        // Initialize TSS with kernel stack
        serial_write(b"[ARCH] TSS init, stack=0x");
        serial_hex(kernel_stack);
        serial_write(b"\r\n");
        tss::init(kernel_stack);

        // Initialize GDT with TSS
        serial_write(b"[ARCH] GDT init...\r\n");
        gdt::init();

        // Initialize PIC
        serial_write(b"[ARCH] PIC init...\r\n");
        pic::init();

        // Initialize IDT with exception handlers
        serial_write(b"[ARCH] IDT init...\r\n");
        idt::init();

        serial_write(b"[ARCH] Architecture initialized\r\n");
    }
}

/// Enable interrupts
#[inline]
pub fn enable_interrupts() {
    unsafe {
        core::arch::asm!("sti", options(nostack, preserves_flags));
    }
}

/// Disable interrupts
#[inline]
pub fn disable_interrupts() {
    unsafe {
        core::arch::asm!("cli", options(nostack, preserves_flags));
    }
}

/// Halt CPU until next interrupt
/// Uses sti;hlt which is atomic - guarantees we don't miss an interrupt
#[inline]
pub fn halt() {
    unsafe {
        core::arch::asm!("sti; hlt", options(nostack, preserves_flags));
    }
}
