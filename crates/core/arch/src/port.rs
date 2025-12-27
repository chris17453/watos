//! x86_64 Port I/O operations

use core::arch::asm;

/// Write a byte to an I/O port
#[inline]
pub unsafe fn outb(port: u16, value: u8) {
    asm!("out dx, al", in("dx") port, in("al") value, options(nostack, preserves_flags));
}

/// Read a byte from an I/O port
#[inline]
pub unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    asm!("in al, dx", out("al") value, in("dx") port, options(nostack, preserves_flags));
    value
}

/// Write a word (16-bit) to an I/O port
#[inline]
pub unsafe fn outw(port: u16, value: u16) {
    asm!("out dx, ax", in("dx") port, in("ax") value, options(nostack, preserves_flags));
}

/// Read a word (16-bit) from an I/O port
#[inline]
pub unsafe fn inw(port: u16) -> u16 {
    let value: u16;
    asm!("in ax, dx", out("ax") value, in("dx") port, options(nostack, preserves_flags));
    value
}

/// Write a dword (32-bit) to an I/O port
#[inline]
pub unsafe fn outl(port: u16, value: u32) {
    asm!("out dx, eax", in("dx") port, in("eax") value, options(nostack, preserves_flags));
}

/// Read a dword (32-bit) from an I/O port
#[inline]
pub unsafe fn inl(port: u16) -> u32 {
    let value: u32;
    asm!("in eax, dx", out("eax") value, in("dx") port, options(nostack, preserves_flags));
    value
}

/// Small I/O delay (for older hardware that needs time between port accesses)
#[inline]
pub unsafe fn io_wait() {
    // Write to unused port 0x80 (POST code port)
    outb(0x80, 0);
}
