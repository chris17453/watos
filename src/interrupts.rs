//! Interrupt handling for x86_64
//!
//! Sets up IDT, PIC, timer and keyboard interrupts

use core::arch::asm;

// PIC ports
const PIC1_COMMAND: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_COMMAND: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;

// PIC commands
const PIC_EOI: u8 = 0x20;
const ICW1_INIT: u8 = 0x11;
const ICW4_8086: u8 = 0x01;

// Remap IRQs to these vectors (avoid CPU exceptions 0-31)
const PIC1_OFFSET: u8 = 32;
const PIC2_OFFSET: u8 = 40;

// IRQ numbers (after remapping)
pub const IRQ_TIMER: u8 = PIC1_OFFSET;      // IRQ0 -> INT 32
pub const IRQ_KEYBOARD: u8 = PIC1_OFFSET + 1; // IRQ1 -> INT 33

// IDT entry for 64-bit mode
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct IdtEntry {
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
        Self {
            offset_low: 0,
            selector: 0,
            ist: 0,
            type_attr: 0,
            offset_mid: 0,
            offset_high: 0,
            reserved: 0,
        }
    }

    fn set_handler(&mut self, handler: u64, code_selector: u16) {
        self.offset_low = handler as u16;
        self.offset_mid = (handler >> 16) as u16;
        self.offset_high = (handler >> 32) as u32;
        self.selector = code_selector; // Use actual CS from CPU
        self.ist = 0;
        self.type_attr = 0x8E; // Present, Ring 0, Interrupt Gate
        self.reserved = 0;
    }
}

#[repr(C, packed)]
struct IdtDescriptor {
    limit: u16,
    base: u64,
}

// Static IDT - 256 entries
static mut IDT: [IdtEntry; 256] = [IdtEntry::empty(); 256];

// Interrupt state
pub static mut TIMER_TICKS: u64 = 0;
pub static mut KEY_BUFFER: [u8; 32] = [0; 32];
pub static mut KEY_READ_POS: usize = 0;
pub static mut KEY_WRITE_POS: usize = 0;

// Port I/O
#[inline(always)]
unsafe fn outb(port: u16, value: u8) {
    asm!("out dx, al", in("dx") port, in("al") value, options(nostack, preserves_flags));
}

#[inline(always)]
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    asm!("in al, dx", out("al") value, in("dx") port, options(nostack, preserves_flags));
    value
}

/// Initialize the PIC (8259) - remap IRQs
unsafe fn init_pic() {
    // ICW1: Start initialization, expect ICW4
    outb(PIC1_COMMAND, ICW1_INIT);
    outb(PIC2_COMMAND, ICW1_INIT);

    // ICW2: Set vector offsets
    outb(PIC1_DATA, PIC1_OFFSET);
    outb(PIC2_DATA, PIC2_OFFSET);

    // ICW3: Tell master about slave at IRQ2, tell slave its cascade identity
    outb(PIC1_DATA, 4);
    outb(PIC2_DATA, 2);

    // ICW4: 8086 mode
    outb(PIC1_DATA, ICW4_8086);
    outb(PIC2_DATA, ICW4_8086);

    // Mask all interrupts except keyboard (IRQ1)
    // Timer (IRQ0) is only enabled when needed for delays
    outb(PIC1_DATA, 0xFD); // 11111101 - only enable IRQ1 (keyboard)
    outb(PIC2_DATA, 0xFF); // Mask all on slave
}

/// Send End-Of-Interrupt to PIC
#[inline(always)]
unsafe fn pic_eoi(irq: u8) {
    if irq >= 8 {
        outb(PIC2_COMMAND, PIC_EOI);
    }
    outb(PIC1_COMMAND, PIC_EOI);
}

// Timer interrupt handler
#[unsafe(naked)]
unsafe extern "C" fn timer_handler_asm() {
    core::arch::naked_asm!(
        "push rax",
        "push rbx",

        // Increment tick counter using absolute address
        "lea rbx, [rip + {ticks}]",
        "mov rax, [rbx]",
        "inc rax",
        "mov [rbx], rax",

        // Send EOI to PIC1 (port 0x20, value 0x20)
        "mov al, 0x20",
        "out 0x20, al",

        "pop rbx",
        "pop rax",
        "iretq",
        ticks = sym TIMER_TICKS,
    );
}

// Keyboard interrupt handler
#[unsafe(naked)]
unsafe extern "C" fn keyboard_handler_asm() {
    core::arch::naked_asm!(
        "push rax",
        "push rbx",
        "push rcx",
        "push rdx",

        // Read scancode from keyboard port and save it
        "in al, 0x60",
        "mov dl, al",  // Save scancode in dl

        // Get write_pos into eax
        "lea rbx, [rip + {write_pos}]",
        "mov eax, [rbx]",

        // Calculate next_write = (write_pos + 1) & 31 into ecx
        "lea ecx, [eax + 1]",
        "and ecx, 31",

        // Check if buffer full (next_write == read_pos)
        "lea rbx, [rip + {read_pos}]",
        "cmp ecx, [rbx]",
        "je 2f",

        // Store scancode at KEY_BUFFER[write_pos]
        "lea rbx, [rip + {buffer}]",
        "mov [rbx + rax], dl",

        // Update write_pos = next_write
        "lea rbx, [rip + {write_pos}]",
        "mov [rbx], ecx",

        "2:",
        // Send EOI to PIC1
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
    );
}

/// Initialize interrupt system
pub fn init() {
    unsafe {
        // Disable interrupts during setup
        asm!("cli", options(nostack, preserves_flags));

        // Read actual code segment selector from CS register
        let cs: u16;
        asm!("mov {0:x}, cs", out(reg) cs, options(nostack, preserves_flags));

        // Initialize PIC with remapped IRQs
        init_pic();

        // Set up IDT entries for timer and keyboard using actual CS
        let timer_addr = timer_handler_asm as *const () as u64;
        let keyboard_addr = keyboard_handler_asm as *const () as u64;
        IDT[IRQ_TIMER as usize].set_handler(timer_addr, cs);
        IDT[IRQ_KEYBOARD as usize].set_handler(keyboard_addr, cs);

        // Load IDT
        let idt_desc = IdtDescriptor {
            limit: (core::mem::size_of::<[IdtEntry; 256]>() - 1) as u16,
            base: (&raw const IDT) as u64,
        };
        asm!("lidt [{}]", in(reg) &idt_desc, options(nostack, preserves_flags));

        // Enable interrupts
        asm!("sti", options(nostack, preserves_flags));
    }
}

/// Get a scancode from the keyboard buffer (interrupt-driven)
pub fn get_scancode() -> Option<u8> {
    unsafe {
        let read_pos = KEY_READ_POS;
        let write_pos = KEY_WRITE_POS;
        if read_pos != write_pos {
            let scancode = KEY_BUFFER[read_pos];
            KEY_READ_POS = (read_pos + 1) % 32; // Avoid calling .len() on static mut
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

/// Halt CPU until next interrupt (power-saving)
#[inline(always)]
pub fn halt() {
    unsafe {
        asm!("hlt", options(nostack, preserves_flags));
    }
}

/// Enable timer interrupt (for timing operations)
pub fn enable_timer() {
    unsafe {
        let mask = inb(PIC1_DATA);
        outb(PIC1_DATA, mask & 0xFE); // Clear bit 0 to enable IRQ0
    }
}

/// Disable timer interrupt (for idle power saving)
pub fn disable_timer() {
    unsafe {
        let mask = inb(PIC1_DATA);
        outb(PIC1_DATA, mask | 0x01); // Set bit 0 to disable IRQ0
    }
}

/// Wait for approximately N milliseconds
/// Note: Default PIT is ~18.2 Hz (~55ms per tick)
pub fn sleep_ms(ms: u32) {
    let ticks_needed = ((ms as u64) / 55).max(1);
    let start = get_ticks();
    while get_ticks().wrapping_sub(start) < ticks_needed {
        halt();
    }
}
