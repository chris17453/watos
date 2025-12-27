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

        // Initialize default VGA palette
        init_default_palette();

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

// =============================================================================
// Syscall Handler (INT 0x80)
// =============================================================================

/// Syscall numbers
pub mod syscall {
    // Console/IO
    pub const SYS_WRITE: u32 = 1;
    pub const SYS_READ: u32 = 2;
    pub const SYS_OPEN: u32 = 3;
    pub const SYS_CLOSE: u32 = 4;
    pub const SYS_GETKEY: u32 = 5;
    pub const SYS_EXIT: u32 = 6;

    // VGA Graphics
    pub const SYS_VGA_SET_MODE: u32 = 30;
    pub const SYS_VGA_SET_PIXEL: u32 = 31;
    pub const SYS_VGA_GET_PIXEL: u32 = 32;
    pub const SYS_VGA_BLIT: u32 = 33;
    pub const SYS_VGA_CLEAR: u32 = 34;
    pub const SYS_VGA_FLIP: u32 = 35;
    pub const SYS_VGA_SET_PALETTE: u32 = 36;
}

/// Syscall context passed from assembly handler
#[repr(C)]
pub struct SyscallContext {
    pub r11: u64,
    pub r10: u64,  // arg4
    pub r9: u64,   // arg6
    pub r8: u64,   // arg5
    pub rdi: u64,  // arg1
    pub rsi: u64,  // arg2
    pub rdx: u64,  // arg3
    pub rcx: u64,
    pub rax: u64,  // syscall number / return value
}

/// Syscall handler - dispatches based on syscall number
/// Returns result in rax
#[no_mangle]
pub extern "C" fn syscall_handler(ctx: &mut SyscallContext) {
    let syscall_num = ctx.rax as u32;

    // Debug: log syscall number
    unsafe {
        // Write to serial port directly
        let port: u16 = 0x3F8;
        let msg = b"SC:";
        for &b in msg {
            core::arch::asm!("out dx, al", in("dx") port, in("al") b, options(nostack));
        }
        let hex = b"0123456789ABCDEF";
        core::arch::asm!("out dx, al", in("dx") port, in("al") hex[((syscall_num >> 4) & 0xF) as usize], options(nostack));
        core::arch::asm!("out dx, al", in("dx") port, in("al") hex[(syscall_num & 0xF) as usize], options(nostack));
        core::arch::asm!("out dx, al", in("dx") port, in("al") b' ', options(nostack));
    }

    ctx.rax = match syscall_num {
        syscall::SYS_WRITE => {
            // Write to console: rdi=fd (ignored), rsi=buf, rdx=len
            let buf = ctx.rsi as *const u8;
            let len = ctx.rdx as usize;
            // Debug: print buf address and first byte
            unsafe {
                let port: u16 = 0x3F8;
                let hex = b"0123456789ABCDEF";
                let msg = b"WR:@";
                for &b in msg { core::arch::asm!("out dx, al", in("dx") port, in("al") b, options(nostack)); }
                for i in (0..16).rev() {
                    let n = ((ctx.rsi >> (i*4)) & 0xF) as usize;
                    core::arch::asm!("out dx, al", in("dx") port, in("al") hex[n], options(nostack));
                }
                let msg2 = b" L=";
                for &b in msg2 { core::arch::asm!("out dx, al", in("dx") port, in("al") b, options(nostack)); }
                for i in (0..8).rev() {
                    let n = ((len as u64 >> (i*4)) & 0xF) as usize;
                    core::arch::asm!("out dx, al", in("dx") port, in("al") hex[n], options(nostack));
                }
                if !buf.is_null() && len > 0 {
                    let msg3 = b" D=";
                    for &b in msg3 { core::arch::asm!("out dx, al", in("dx") port, in("al") b, options(nostack)); }
                    for i in 0..len.min(16) {
                        let byte = *buf.add(i);
                        core::arch::asm!("out dx, al", in("dx") port, in("al") hex[(byte >> 4) as usize], options(nostack));
                        core::arch::asm!("out dx, al", in("dx") port, in("al") hex[(byte & 0xF) as usize], options(nostack));
                    }
                }
                core::arch::asm!("out dx, al", in("dx") port, in("al") b'\r', options(nostack));
                core::arch::asm!("out dx, al", in("dx") port, in("al") b'\n', options(nostack));
            }
            if !buf.is_null() && len > 0 {
                unsafe {
                    let slice = core::slice::from_raw_parts(buf, len);
                    crate::console::print(slice);
                }
            }
            len as u64
        }

        syscall::SYS_READ => {
            // Read from console: rdi=fd (ignored), rsi=buf, rdx=max_len
            // Returns number of bytes read
            0 // Handled by watos_console_read for now
        }

        syscall::SYS_GETKEY => {
            // Get key without blocking
            if let Some(scancode) = get_scancode() {
                scancode as u64
            } else {
                0
            }
        }

        syscall::SYS_EXIT => {
            // Exit process: rdi=exit_code
            let code = ctx.rdi as i32;
            crate::process::exit_current(code);
            // Mark that we should return to kernel after syscall
            // The actual return happens when the process returns from its entry
            0
        }

        syscall::SYS_VGA_SET_MODE => {
            // Set VGA mode: rdi=mode_num
            let mode = ctx.rdi as u8;
            vga_set_mode(mode)
        }

        syscall::SYS_VGA_SET_PIXEL => {
            // Set pixel: rdi=x, rsi=y, rdx=color
            let x = ctx.rdi as i32;
            let y = ctx.rsi as i32;
            let color = ctx.rdx as u8;
            vga_set_pixel(x, y, color);
            0
        }

        syscall::SYS_VGA_GET_PIXEL => {
            // Get pixel: rdi=x, rsi=y
            let x = ctx.rdi as i32;
            let y = ctx.rsi as i32;
            vga_get_pixel(x, y) as u64
        }

        syscall::SYS_VGA_BLIT => {
            // Blit buffer: rdi=buf_ptr, rsi=width, rdx=height, r10=stride
            let buf = ctx.rdi as *const u8;
            let width = ctx.rsi as usize;
            let height = ctx.rdx as usize;
            let stride = ctx.r10 as usize;
            vga_blit(buf, width, height, stride);
            0
        }

        syscall::SYS_VGA_CLEAR => {
            // Clear screen: rdi=color
            let color = ctx.rdi as u8;
            vga_clear(color);
            0
        }

        syscall::SYS_VGA_FLIP => {
            // Flip/commit buffer (no-op for now, direct framebuffer)
            0
        }

        syscall::SYS_VGA_SET_PALETTE => {
            // Set palette: rdi=index, rsi=r, rdx=g, r10=b
            let index = (ctx.rdi & 0xFF) as usize;
            let r = (ctx.rsi & 0xFF) as u8;
            let g = (ctx.rdx & 0xFF) as u8;
            let b = (ctx.r10 & 0xFF) as u8;
            
            unsafe {
                if index < 256 {
                    VGA_PALETTE[index] = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
                }
            }
            0
        }

        _ => {
            // Unknown syscall
            u64::MAX
        }
    };
}

// Syscall handler assembly stub
#[unsafe(naked)]
unsafe extern "C" fn syscall_handler_asm() {
    core::arch::naked_asm!(
        // Save registers
        "push rax",
        "push rcx",
        "push rdx",
        "push rsi",
        "push rdi",
        "push r8",
        "push r9",
        "push r10",
        "push r11",

        // Call Rust handler with pointer to context
        "mov rdi, rsp",
        "call {handler}",

        // Restore registers
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop rcx",
        "pop rax",

        "iretq",
        handler = sym syscall_handler,
    );
}


/// Install syscall handler at INT 0x80
pub fn init_syscalls() {
    unsafe {
        let cs: u16;
        asm!("mov {0:x}, cs", out(reg) cs, options(nostack, preserves_flags));

        let handler_addr = syscall_handler_asm as *const () as u64;
        IDT[0x80].set_handler(handler_addr, cs);
        // Set type to 0x8E for interrupt gate, or 0xEE for user-callable
        IDT[0x80].type_attr = 0xEE; // Present, Ring 3 callable, Interrupt Gate
    }
}

// =============================================================================
// VGA Syscall Implementations
// =============================================================================

// External framebuffer functions from main.rs
extern "C" {
    fn fb_put_pixel(x: u32, y: u32, r: u8, g: u8, b: u8);
    fn fb_get_pixel(x: i32, y: i32) -> u8;
    fn fb_clear_screen(r: u8, g: u8, b: u8);
}

// VGA mode state
static mut VGA_MODE: u8 = 0;
static mut VGA_WIDTH: usize = 0;
static mut VGA_HEIGHT: usize = 0;

// VGA palette (256 entries, RGB format)
static mut VGA_PALETTE: [u32; 256] = [0; 256];

/// Initialize default VGA palette
fn init_default_palette() {
    unsafe {
        // Standard 16-color EGA/VGA palette
        const PALETTE_16: [u32; 16] = [
            0x000000, 0x0000AA, 0x00AA00, 0x00AAAA,
            0xAA0000, 0xAA00AA, 0xAA5500, 0xAAAAAA,
            0x555555, 0x5555FF, 0x55FF55, 0x55FFFF,
            0xFF5555, 0xFF55FF, 0xFFFF55, 0xFFFFFF,
        ];
        
        // Copy 16-color palette to first 16 entries
        for i in 0..16 {
            VGA_PALETTE[i] = PALETTE_16[i];
        }
        
        // Generate grayscale for remaining entries
        for i in 16..256 {
            let gray = ((i - 16) * 255 / 239) as u8;
            VGA_PALETTE[i] = ((gray as u32) << 16) | ((gray as u32) << 8) | (gray as u32);
        }
    }
}

/// Set VGA mode
fn vga_set_mode(mode: u8) -> u64 {
    unsafe {
        VGA_MODE = mode;
        match mode {
            0 => { VGA_WIDTH = 80; VGA_HEIGHT = 25; }  // Text mode
            1 => { VGA_WIDTH = 320; VGA_HEIGHT = 200; } // 320x200
            2 => { VGA_WIDTH = 640; VGA_HEIGHT = 200; } // 640x200
            3 => { VGA_WIDTH = 640; VGA_HEIGHT = 480; } // 640x480
            _ => return u64::MAX,
        }
        0
    }
}

/// Set pixel using kernel framebuffer
fn vga_set_pixel(x: i32, y: i32, color: u8) {
    if x < 0 || y < 0 {
        return;
    }

    // Use VGA_PALETTE for indexed color lookup
    let rgb = unsafe { VGA_PALETTE[color as usize] };
    let r = ((rgb >> 16) & 0xFF) as u8;
    let g = ((rgb >> 8) & 0xFF) as u8;
    let b = (rgb & 0xFF) as u8;

    // Call into framebuffer drawing
    unsafe {
        fb_put_pixel(x as u32, y as u32, r, g, b);
    }
}

/// Get pixel at position
fn vga_get_pixel(x: i32, y: i32) -> u8 {
    // Read from framebuffer - return grayscale value
    unsafe {
        fb_get_pixel(x, y)
    }
}

/// Blit indexed color buffer to framebuffer
fn vga_blit(buf: *const u8, width: usize, height: usize, stride: usize) {
    if buf.is_null() {
        return;
    }

    const PALETTE_16: [u32; 16] = [
        0x000000, 0x0000AA, 0x00AA00, 0x00AAAA,
        0xAA0000, 0xAA00AA, 0xAA5500, 0xAAAAAA,
        0x555555, 0x5555FF, 0x55FF55, 0x55FFFF,
        0xFF5555, 0xFF55FF, 0xFFFF55, 0xFFFFFF,
    ];

    unsafe {
        for y in 0..height {
            for x in 0..width {
                let color = *buf.add(y * stride + x);
                let rgb = PALETTE_16[(color & 0x0F) as usize];
                let r = ((rgb >> 16) & 0xFF) as u8;
                let g = ((rgb >> 8) & 0xFF) as u8;
                let b = (rgb & 0xFF) as u8;
                fb_put_pixel(x as u32, y as u32, r, g, b);
            }
        }
    }
}

/// Clear screen with color
fn vga_clear(color: u8) {
    const PALETTE_16: [u32; 16] = [
        0x000000, 0x0000AA, 0x00AA00, 0x00AAAA,
        0xAA0000, 0xAA00AA, 0xAA5500, 0xAAAAAA,
        0x555555, 0x5555FF, 0x55FF55, 0x55FFFF,
        0xFF5555, 0xFF55FF, 0xFFFF55, 0xFFFFFF,
    ];

    let rgb = PALETTE_16[(color & 0x0F) as usize];
    let r = ((rgb >> 16) & 0xFF) as u8;
    let g = ((rgb >> 8) & 0xFF) as u8;
    let b = (rgb & 0xFF) as u8;

    unsafe {
        fb_clear_screen(r, g, b);
    }
}
