//! 8259 Programmable Interrupt Controller (PIC)
//!
//! Handles IRQ remapping and End-of-Interrupt signaling.

use crate::port::{outb, inb, io_wait};

// PIC ports
const PIC1_COMMAND: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_COMMAND: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;

// PIC commands
const ICW1_INIT: u8 = 0x11;
const ICW4_8086: u8 = 0x01;
const PIC_EOI: u8 = 0x20;

/// IRQ offset for master PIC (vectors 32-39)
pub const PIC1_OFFSET: u8 = 32;
/// IRQ offset for slave PIC (vectors 40-47)
pub const PIC2_OFFSET: u8 = 40;

/// Hardware interrupt vectors
pub mod irq {
    use super::*;
    pub const TIMER: u8 = PIC1_OFFSET;      // IRQ0 -> INT 32
    pub const KEYBOARD: u8 = PIC1_OFFSET + 1; // IRQ1 -> INT 33
    pub const CASCADE: u8 = PIC1_OFFSET + 2;  // IRQ2 (cascade)
    pub const COM2: u8 = PIC1_OFFSET + 3;     // IRQ3
    pub const COM1: u8 = PIC1_OFFSET + 4;     // IRQ4
    pub const LPT2: u8 = PIC1_OFFSET + 5;     // IRQ5
    pub const FLOPPY: u8 = PIC1_OFFSET + 6;   // IRQ6
    pub const LPT1: u8 = PIC1_OFFSET + 7;     // IRQ7
    pub const RTC: u8 = PIC2_OFFSET;          // IRQ8
    pub const FREE1: u8 = PIC2_OFFSET + 1;    // IRQ9
    pub const FREE2: u8 = PIC2_OFFSET + 2;    // IRQ10
    pub const FREE3: u8 = PIC2_OFFSET + 3;    // IRQ11
    pub const MOUSE: u8 = PIC2_OFFSET + 4;    // IRQ12
    pub const FPU: u8 = PIC2_OFFSET + 5;      // IRQ13
    pub const ATA1: u8 = PIC2_OFFSET + 6;     // IRQ14
    pub const ATA2: u8 = PIC2_OFFSET + 7;     // IRQ15
}

/// Initialize and remap the PIC
pub fn init() {
    unsafe {
        // Save masks
        let mask1 = inb(PIC1_DATA);
        let mask2 = inb(PIC2_DATA);

        // ICW1: Start initialization sequence
        outb(PIC1_COMMAND, ICW1_INIT);
        io_wait();
        outb(PIC2_COMMAND, ICW1_INIT);
        io_wait();

        // ICW2: Set vector offsets
        outb(PIC1_DATA, PIC1_OFFSET);
        io_wait();
        outb(PIC2_DATA, PIC2_OFFSET);
        io_wait();

        // ICW3: Configure cascading
        outb(PIC1_DATA, 4); // Slave on IRQ2
        io_wait();
        outb(PIC2_DATA, 2); // Cascade identity
        io_wait();

        // ICW4: 8086 mode
        outb(PIC1_DATA, ICW4_8086);
        io_wait();
        outb(PIC2_DATA, ICW4_8086);
        io_wait();

        // Restore masks (or set default)
        // Enable keyboard (IRQ1) only by default
        outb(PIC1_DATA, 0xFD); // 11111101 - only keyboard
        outb(PIC2_DATA, 0xFF); // All masked

        crate::serial_write(b"[PIC] Remapped to vectors 32-47\r\n");
    }
}

/// Send End-of-Interrupt signal
pub fn send_eoi(irq: u8) {
    unsafe {
        if irq >= 8 {
            outb(PIC2_COMMAND, PIC_EOI);
        }
        outb(PIC1_COMMAND, PIC_EOI);
    }
}

/// Enable a specific IRQ
pub fn enable_irq(irq: u8) {
    unsafe {
        if irq < 8 {
            let mask = inb(PIC1_DATA);
            outb(PIC1_DATA, mask & !(1 << irq));
        } else {
            let mask = inb(PIC2_DATA);
            outb(PIC2_DATA, mask & !(1 << (irq - 8)));
        }
    }
}

/// Disable a specific IRQ
pub fn disable_irq(irq: u8) {
    unsafe {
        if irq < 8 {
            let mask = inb(PIC1_DATA);
            outb(PIC1_DATA, mask | (1 << irq));
        } else {
            let mask = inb(PIC2_DATA);
            outb(PIC2_DATA, mask | (1 << (irq - 8)));
        }
    }
}

/// Enable timer interrupt (IRQ0)
pub fn enable_timer() {
    enable_irq(0);
}

/// Disable timer interrupt (IRQ0)
pub fn disable_timer() {
    disable_irq(0);
}
