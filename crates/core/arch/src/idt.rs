//! Interrupt Descriptor Table (IDT)
//!
//! Manages the IDT with exception handlers, hardware interrupts, and syscalls.

use core::arch::naked_asm;
use crate::gdt::selectors;
use crate::exceptions;
use crate::pic;

/// IDT entry (16 bytes in 64-bit mode)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    type_attr: u8,
    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtEntry {
    const fn empty() -> Self {
        IdtEntry {
            offset_low: 0,
            selector: 0,
            ist: 0,
            type_attr: 0,
            offset_mid: 0,
            offset_high: 0,
            reserved: 0,
        }
    }

    /// Set handler for Ring 0 only (DPL=0)
    fn set_handler(&mut self, handler: u64, ist_index: u8) {
        self.offset_low = handler as u16;
        self.offset_mid = (handler >> 16) as u16;
        self.offset_high = (handler >> 32) as u32;
        self.selector = selectors::KERNEL_CODE;
        self.ist = ist_index & 0x7;
        self.type_attr = 0x8E; // Present, DPL=0, Interrupt Gate
        self.reserved = 0;
    }

    /// Set handler callable from Ring 3 (DPL=3) - for syscalls
    fn set_user_handler(&mut self, handler: u64) {
        self.offset_low = handler as u16;
        self.offset_mid = (handler >> 16) as u16;
        self.offset_high = (handler >> 32) as u32;
        self.selector = selectors::KERNEL_CODE;
        self.ist = 0;
        self.type_attr = 0xEE; // Present, DPL=3, Interrupt Gate
        self.reserved = 0;
    }

    /// Set trap gate (doesn't disable interrupts)
    fn set_trap(&mut self, handler: u64, ist_index: u8) {
        self.offset_low = handler as u16;
        self.offset_mid = (handler >> 16) as u16;
        self.offset_high = (handler >> 32) as u32;
        self.selector = selectors::KERNEL_CODE;
        self.ist = ist_index & 0x7;
        self.type_attr = 0x8F; // Present, DPL=0, Trap Gate
        self.reserved = 0;
    }
}

/// IDT Pointer for LIDT instruction
#[repr(C, packed)]
struct IdtPointer {
    limit: u16,
    base: u64,
}

/// Static IDT - 256 entries
static mut IDT: [IdtEntry; 256] = [IdtEntry::empty(); 256];

/// Timer tick counter
pub static mut TIMER_TICKS: u64 = 0;

/// Keyboard buffer
pub static mut KEY_BUFFER: [u8; 32] = [0; 32];
pub static mut KEY_READ_POS: usize = 0;
pub static mut KEY_WRITE_POS: usize = 0;

/// Initialize IDT with all handlers
pub fn init() {
    unsafe {
        // Disable interrupts during setup
        core::arch::asm!("cli", options(nostack, preserves_flags));

        // Install exception handlers (vectors 0-31)
        let handlers = exceptions::handlers();
        for (vector, (handler, _has_error, ist)) in handlers.iter().enumerate() {
            let handler_addr = *handler as u64;
            IDT[vector].set_handler(handler_addr, *ist);
        }

        // Install hardware interrupt handlers (vectors 32-47)
        IDT[pic::irq::TIMER as usize].set_handler(timer_handler as u64, 0);
        IDT[pic::irq::KEYBOARD as usize].set_handler(keyboard_handler as u64, 0);

        // Load IDT
        let idt_ptr = IdtPointer {
            limit: (core::mem::size_of::<[IdtEntry; 256]>() - 1) as u16,
            base: IDT.as_ptr() as u64,
        };
        core::arch::asm!("lidt [{}]", in(reg) &idt_ptr, options(nostack, preserves_flags));

        crate::serial_write(b"[IDT] Loaded with ");
        crate::serial_hex_byte(32);
        crate::serial_write(b" exception handlers\r\n");

        // Enable interrupts
        core::arch::asm!("sti", options(nostack, preserves_flags));
    }
}

/// Install syscall handler at INT 0x80 (callable from Ring 3)
pub fn install_syscall_handler(handler: unsafe extern "C" fn()) {
    unsafe {
        IDT[0x80].set_user_handler(handler as u64);
        crate::serial_write(b"[IDT] Syscall handler installed at INT 0x80\r\n");
    }
}

/// Install a custom interrupt handler
pub fn install_handler(vector: u8, handler: unsafe extern "C" fn(), ring3_callable: bool) {
    unsafe {
        if ring3_callable {
            IDT[vector as usize].set_user_handler(handler as u64);
        } else {
            IDT[vector as usize].set_handler(handler as u64, 0);
        }
    }
}

// ============================================================================
// Hardware Interrupt Handlers
// ============================================================================

/// Timer interrupt handler (IRQ0 -> INT 32)
#[unsafe(naked)]
unsafe extern "C" fn timer_handler() {
    naked_asm!(
        "push rax",
        "push rdx",

        // Increment tick counter
        "lea rax, [rip + {ticks}]",
        "lock inc qword ptr [rax]",

        // Send EOI
        "mov al, 0x20",
        "out 0x20, al",

        "pop rdx",
        "pop rax",
        "iretq",
        ticks = sym TIMER_TICKS,
        options()
    );
}

/// Keyboard interrupt handler (IRQ1 -> INT 33)
#[unsafe(naked)]
unsafe extern "C" fn keyboard_handler() {
    naked_asm!(
        "push rax",
        "push rbx",
        "push rcx",
        "push rdx",

        // Read scancode
        "in al, 0x60",
        "mov dl, al",

        // Get write position
        "lea rbx, [rip + {write_pos}]",
        "mov rax, [rbx]",

        // Calculate next position
        "lea rcx, [rax + 1]",
        "and rcx, 31",

        // Check if buffer full
        "lea rbx, [rip + {read_pos}]",
        "cmp rcx, [rbx]",
        "je 2f",

        // Store scancode
        "lea rbx, [rip + {buffer}]",
        "mov [rbx + rax], dl",

        // Update write position
        "lea rbx, [rip + {write_pos}]",
        "mov [rbx], rcx",

        "2:",
        // Send EOI
        "mov al, 0x20",
        "out 0x20, al",

        "pop rdx",
        "pop rcx",
        "pop rbx",
        "pop rax",
        "iretq",
        buffer = sym KEY_BUFFER,
        write_pos = sym KEY_WRITE_POS,
        read_pos = sym KEY_READ_POS,
        options()
    );
}

// ============================================================================
// Public API
// ============================================================================

/// Get a scancode from keyboard buffer
pub fn get_scancode() -> Option<u8> {
    unsafe {
        if KEY_READ_POS != KEY_WRITE_POS {
            let scancode = KEY_BUFFER[KEY_READ_POS];
            KEY_READ_POS = (KEY_READ_POS + 1) & 31;
            Some(scancode)
        } else {
            None
        }
    }
}

/// Get current timer tick count
pub fn get_ticks() -> u64 {
    unsafe { TIMER_TICKS }
}

/// Wait for approximately N milliseconds (18.2 Hz timer = ~55ms/tick)
pub fn sleep_ms(ms: u32) {
    let ticks_needed = ((ms as u64) / 55).max(1);
    let start = get_ticks();
    while get_ticks().wrapping_sub(start) < ticks_needed {
        crate::halt();
    }
}
