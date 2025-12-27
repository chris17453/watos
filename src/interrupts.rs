//! Interrupt handling for x86_64
//!
//! Sets up IDT, PIC, timer and keyboard interrupts

use core::arch::asm;
// External functions from main.rs
extern "C" {
    fn watos_set_cursor(x: u32, y: u32);
    fn watos_clear_screen();
    fn fb_clear_impl(r: u8, g: u8, b: u8);
}

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
        self.selector = code_selector; // Kernel code segment
        self.ist = 0;  // Don't use IST (Interrupt Stack Table)
        self.type_attr = 0x8E; // Present, Ring 0, Interrupt Gate (DPL=0)
        self.reserved = 0;
    }

    /// Set handler accessible from Ring 3 (for syscalls)
    fn set_user_handler(&mut self, handler: u64, code_selector: u16) {
        self.offset_low = handler as u16;
        self.offset_mid = (handler >> 16) as u16;
        self.offset_high = (handler >> 32) as u32;
        self.selector = code_selector; // Kernel code segment
        self.ist = 0;
        self.type_attr = 0xEF; // Present, Ring 3 callable (DPL=3), Trap Gate
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
unsafe fn _pic_eoi(irq: u8) {
    if irq >= 8 {
        outb(PIC2_COMMAND, PIC_EOI);
    }
    outb(PIC1_COMMAND, PIC_EOI);
}

// Timer interrupt handler
#[unsafe(naked)]
unsafe extern "C" fn timer_handler_asm() {
    core::arch::naked_asm!(
        // Save registers and maintain stack alignment
        "push rax",
        "push rbx", 
        "push rcx",
        "push rdx",

        // Increment tick counter using absolute address
        "lea rbx, [rip + {ticks}]",
        "mov rax, [rbx]",
        "inc rax",
        "mov [rbx], rax",

        // Send EOI to PIC1 (port 0x20, value 0x20)
        "mov al, 0x20",
        "out 0x20, al",

        // Restore registers in reverse order
        "pop rdx",
        "pop rcx",
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

        // Get write_pos into rax (using full 64-bit)
        "lea rbx, [rip + {write_pos}]",
        "mov rax, [rbx]",

        // Calculate next_write = (write_pos + 1) & 31 into rcx
        "lea rcx, [rax + 1]",
        "and rcx, 31",

        // Check if buffer full (next_write == read_pos)
        "lea rbx, [rip + {read_pos}]",
        "cmp rcx, [rbx]",
        "je 2f",

        // Store scancode at KEY_BUFFER[write_pos]
        "lea rbx, [rip + {buffer}]",
        "mov [rbx + rax], dl",

        // Update write_pos = next_write
        "lea rbx, [rip + {write_pos}]",
        "mov [rbx], rcx",

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

/// Initialize interrupt system with Ring 3 support
pub fn init() {
    unsafe {
        // Disable interrupts during setup
        asm!("cli", options(nostack, preserves_flags));

        // Use kernel code segment from GDT
        let kernel_cs = crate::gdt::selectors::KERNEL_CODE;

        // Initialize PIC with remapped IRQs
        init_pic();

        // Set up IDT entries for timer and keyboard using kernel CS
        let timer_addr = timer_handler_asm as *const () as u64;
        let keyboard_addr = keyboard_handler_asm as *const () as u64;
        IDT[IRQ_TIMER as usize].set_handler(timer_addr, kernel_cs);
        IDT[IRQ_KEYBOARD as usize].set_handler(keyboard_addr, kernel_cs);

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

/// WATOS Syscall numbers - import from shared crate
pub use watos_syscall::numbers as syscall;

/// Syscall context passed from assembly handler
/// Order matches the stack layout from syscall_handler_asm
#[repr(C)]
pub struct SyscallContext {
    pub rax: u64,  // syscall number / return value (pushed first, at top)
    pub rcx: u64,
    pub rdx: u64,  // arg3
    pub rsi: u64,  // arg2
    pub rdi: u64,  // arg1
    pub r8: u64,   // arg5
    pub r9: u64,   // arg6
    pub r10: u64,  // arg4
    pub r11: u64,
    pub rbp: u64,  // alignment padding (pushed last, at bottom)
}

/// Debug helper for syscalls
unsafe fn debug_syscall(msg: &[u8]) {
    const SERIAL_PORT: u16 = 0x3F8;
    for &byte in msg {
        // Simple busy-wait for serial
        for _ in 0..100 {
            core::arch::asm!("nop", options(nostack));
        }
        core::arch::asm!("out dx, al", in("dx") SERIAL_PORT, in("al") byte, options(nostack));
    }
}

unsafe fn debug_hex_byte(val: u8) {
    const SERIAL_PORT: u16 = 0x3F8;
    let hex = b"0123456789ABCDEF";
    for _ in 0..100 { core::arch::asm!("nop", options(nostack)); }
    core::arch::asm!("out dx, al", in("dx") SERIAL_PORT, in("al") hex[(val >> 4) as usize], options(nostack));
    for _ in 0..100 { core::arch::asm!("nop", options(nostack)); }
    core::arch::asm!("out dx, al", in("dx") SERIAL_PORT, in("al") hex[(val & 0xF) as usize], options(nostack));
}

/// Syscall handler - dispatches based on syscall number
/// Returns result in rax
#[no_mangle]
pub extern "C" fn syscall_handler(ctx: &mut SyscallContext) {
    // Debug: immediate output to serial at handler start
    unsafe {
        debug_syscall(b"[ENTER] ");
    }
    
    let syscall_num = ctx.rax as u32;
    
    // Debug: log syscall entry
    unsafe {
        debug_syscall(b"[SYSCALL] ");
        debug_hex_byte((syscall_num & 0xFF) as u8);
        debug_syscall(b" from PID=");
        if let Some(pid) = crate::process::current_pid() {
            debug_hex_byte(pid as u8);
        } else {
            debug_syscall(b"kernel");
        }
        debug_syscall(b"\r\n");
    }

    ctx.rax = match syscall_num {
        syscall::SYS_WRITE => {
            // Write to handle: rdi=handle, rsi=buf, rdx=len
            let handle = ctx.rdi as u32;
            let buf = ctx.rsi as *const u8;
            let len = ctx.rdx as usize;
            
            if !buf.is_null() && len > 0 {
                // Copy data from user memory
                let mut data = alloc::vec::Vec::with_capacity(len);
                unsafe {
                    for i in 0..len {
                        data.push(*buf.add(i));
                    }
                }
                
                // Get current process handle table
                if let Some(handle_table) = crate::process::current_handle_table() {
                    // Try writing to file handle first
                    if let Ok(bytes_written) = crate::io::HandleIO::write_file(handle_table, handle, &data) {
                        bytes_written as u64
                    }
                    // If not a file handle, try console
                    else if let Ok(bytes_written) = crate::io::HandleIO::write_console(handle_table, handle, &data) {
                        bytes_written as u64
                    }
                    // Handle not found or invalid
                    else {
                        crate::io::fs_error_to_errno(crate::disk::vfs::FsError::NotFound)
                    }
                } else {
                    // No process context - write to kernel console as fallback
                    crate::console::print(&data);
                    len as u64
                }
            } else {
                0
            }
        }

        syscall::SYS_READ => {
            // Read from handle: rdi=handle, rsi=buf, rdx=max_len
            let handle = ctx.rdi as u32;
            let buf = ctx.rsi as *mut u8;
            let max_len = ctx.rdx as usize;
            
            if !buf.is_null() && max_len > 0 {
                // Create buffer to read into
                let mut buffer = alloc::vec![0u8; max_len];
                
                if let Some(handle_table) = crate::process::current_handle_table() {
                    // Try reading from file handle first
                    let bytes_read = if let Ok(n) = crate::io::HandleIO::read_file(handle_table, handle, &mut buffer) {
                        n
                    }
                    // If not a file handle, try console
                    else if let Ok(n) = crate::io::HandleIO::read_console(handle_table, handle, &mut buffer) {
                        n
                    }
                    // Handle not found or invalid
                    else {
                        return ctx.rax = crate::io::fs_error_to_errno(crate::disk::vfs::FsError::NotFound);
                    };
                    
                    // Copy data back to user buffer
                    unsafe {
                        for i in 0..bytes_read {
                            *buf.add(i) = buffer[i];
                        }
                    }
                    bytes_read as u64
                } else {
                    // No process context - fall back to keyboard input
                    if let Some(scancode) = get_scancode() {
                        // Simple scancode to ASCII conversion
                        let ascii = match scancode {
                            0x1C => b'\n', // Enter
                            0x39 => b' ',  // Space  
                            0x1E..=0x26 => b'a' + (scancode - 0x1E), // a-l
                            0x10..=0x19 => b'q' + (scancode - 0x10), // q-p
                            0x2C..=0x32 => b'z' + (scancode - 0x2C), // z-m
                            0x02..=0x0B => b'1' + (scancode - 0x02), // 1-0
                            _ => b'?',
                        };
                        unsafe {
                            *buf = ascii;
                        }
                        1
                    } else {
                        0
                    }
                }
            } else {
                0
            }
        }

        syscall::SYS_OPEN => {
            // Open file: rdi=path_ptr, rsi=path_len, rdx=mode
            let path_ptr = ctx.rdi as *const u8;
            let path_len = ctx.rsi as usize;
            let mode = ctx.rdx;
            
            if !path_ptr.is_null() && path_len > 0 && path_len < 256 {
                // Copy path from user memory
                let mut path_bytes = alloc::vec![0u8; path_len];
                unsafe {
                    for i in 0..path_len {
                        path_bytes[i] = *path_ptr.add(i);
                    }
                }
                
                if let Ok(path_str) = alloc::string::String::from_utf8(path_bytes) {
                    if let Some(handle_table) = crate::process::current_handle_table() {
                        let open_mode = crate::io::OpenMode::from(mode);
                        // For now, use filesystem ID 1 (first mounted filesystem)
                        // TODO: Implement proper path resolution
                        match crate::io::HandleIO::open(handle_table, &path_str, open_mode, 1) {
                            Ok(handle) => handle as u64,
                            Err(error) => crate::io::fs_error_to_errno(error),
                        }
                    } else {
                        crate::io::fs_error_to_errno(crate::disk::vfs::FsError::NotSupported)
                    }
                } else {
                    crate::io::fs_error_to_errno(crate::disk::vfs::FsError::InvalidName)
                }
            } else {
                crate::io::fs_error_to_errno(crate::disk::vfs::FsError::InvalidName)
            }
        }

        syscall::SYS_CLOSE => {
            // Close handle: rdi=handle
            let handle = ctx.rdi as u32;
            
            if let Some(handle_table) = crate::process::current_handle_table() {
                match crate::io::HandleIO::close(handle_table, handle) {
                    Ok(_) => 0,
                    Err(error) => crate::io::fs_error_to_errno(error),
                }
            } else {
                crate::io::fs_error_to_errno(crate::disk::vfs::FsError::NotSupported)
            }
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
            // Call the exit handler to return to kernel
            crate::process::process_exit_to_kernel(code);
        }

        syscall::SYS_SLEEP => {
            // Sleep for rdi milliseconds
            let ms = ctx.rdi as u32;
            sleep_ms(ms);
            0
        }

        syscall::SYS_GETPID => {
            // Get current process ID
            if let Some(pid) = crate::process::current_pid() {
                pid as u64
            } else {
                0
            }
        }

        syscall::SYS_TIME => {
            // Get system time (ticks since boot)
            get_ticks()
        }

        syscall::SYS_MALLOC => {
            // Allocate memory: rdi=size
            let size = ctx.rdi as usize;
            // For now, just return a simple allocation from heap
            // TODO: Implement proper user heap management
            if size > 0 {
                use alloc::alloc::{alloc, Layout};
                unsafe {
                    let layout = Layout::from_size_align(size, 8).unwrap();
                    alloc(layout) as u64
                }
            } else {
                0
            }
        }

        syscall::SYS_FREE => {
            // Free memory: rdi=ptr
            let _ptr = ctx.rdi;
            // TODO: Implement proper deallocation
            // For now just return success
            0
        }

        syscall::SYS_PUTCHAR => {
            // Output single character: rdi=char
            let ch = ctx.rdi as u8;
            let buf = [ch];
            crate::console::print(&buf);
            1
        }

        syscall::SYS_CURSOR => {
            // Set cursor position: rdi=x, rsi=y
            let x = ctx.rdi as u32;
            let y = ctx.rsi as u32;
            unsafe { watos_set_cursor(x, y); }
            0
        }

        syscall::SYS_CLEAR => {
            // Clear screen
            unsafe { watos_clear_screen(); }
            0
        }

        syscall::SYS_COLOR => {
            // Set text color: rdi=color
            // For now, just return success (VGA text colors not implemented)
            0
        }

        syscall::SYS_CONSOLE_IN => {
            // Get stdin handle - creates console handle for current process
            if let Some(handle_table) = crate::process::current_handle_table() {
                let handle = handle_table.add_console_handle(crate::io::ConsoleKind::Stdin);
                handle as u64
            } else {
                crate::io::fs_error_to_errno(crate::disk::vfs::FsError::NotSupported)
            }
        }

        syscall::SYS_CONSOLE_OUT => {
            // Get stdout handle - creates console handle for current process
            if let Some(handle_table) = crate::process::current_handle_table() {
                let handle = handle_table.add_console_handle(crate::io::ConsoleKind::Stdout);
                handle as u64
            } else {
                crate::io::fs_error_to_errno(crate::disk::vfs::FsError::NotSupported)
            }
        }

        syscall::SYS_CONSOLE_ERR => {
            // Get stderr handle - creates console handle for current process
            if let Some(handle_table) = crate::process::current_handle_table() {
                let handle = handle_table.add_console_handle(crate::io::ConsoleKind::Stderr);
                handle as u64
            } else {
                crate::io::fs_error_to_errno(crate::disk::vfs::FsError::NotSupported)
            }
        }

        syscall::SYS_GFX_PSET => {
            // Set pixel: rdi=x, rsi=y, rdx=color
            let x = ctx.rdi as i32;
            let y = ctx.rsi as i32;  
            let color = ctx.rdx as u8;
            vga_set_pixel(x, y, color);
            0
        }

        syscall::SYS_GFX_LINE => {
            // Draw line (basic implementation)
            // For now just return success - TODO: implement line drawing
            0
        }

        syscall::SYS_GFX_CIRCLE => {
            // Draw circle (basic implementation) 
            // For now just return success - TODO: implement circle drawing
            0
        }

        syscall::SYS_GFX_CLS => {
            // Clear graphics screen
            unsafe {
                fb_clear_impl(0, 0, 0); // Clear to black
            }
            0
        }

        syscall::SYS_GFX_MODE => {
            // Set graphics mode: rdi=mode
            // For now just return success
            0
        }

        syscall::SYS_GFX_DISPLAY => {
            // Display/flip graphics buffer
            // For now just return success 
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

// Syscall handler assembly stub - must be pub for process module to reference
#[unsafe(naked)]
pub unsafe extern "C" fn syscall_handler_asm() {
    core::arch::naked_asm!(
        // Debug: write 'S' to serial port immediately 
        "push rax",
        "push rdx",
        "mov al, 83", // 'S'
        "mov dx, 0x3F8",
        "out dx, al",
        "pop rdx",
        "pop rax",

        // Save registers and align stack
        "push rax",
        "push rcx", 
        "push rdx",
        "push rsi",
        "push rdi",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        "push rbp",    // Extra push for 16-byte alignment (9+1=10, 10*8=80 bytes, 80%16=0)

        // Debug: write 'C' to serial before calling handler
        "push rax",
        "push rdx",
        "mov al, 67", // 'C' 
        "mov dx, 0x3F8",
        "out dx, al",
        "pop rdx",
        "pop rax",

        // Set up stack frame and call Rust handler
        "mov rbp, rsp",
        "mov rdi, rsp",
        "call {handler}",

        // Debug: write 'R' to serial after handler returns
        "push rax",
        "push rdx",
        "mov al, 82", // 'R'
        "mov dx, 0x3F8",
        "out dx, al", 
        "pop rdx",
        "pop rax",

        // Restore registers
        "pop rbp",
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


/// Install syscall handler at INT 0x80 (user-callable)
pub fn init_syscalls() {
    unsafe {
        // Use kernel code segment for handler
        let kernel_cs = crate::gdt::selectors::KERNEL_CODE;
        let handler_addr = syscall_handler_asm as *const () as u64;
        
        // Set INT 0x80 as user-callable (DPL=3)
        IDT[0x80].set_user_handler(handler_addr, kernel_cs);
        
        debug_syscall(b"[SYSCALL] INT 0x80 handler installed at 0x");
        unsafe {
            let hex = b"0123456789ABCDEF";
            for i in (0..16).rev() {
                let nibble = ((handler_addr >> (i * 4)) & 0xF) as usize;
                core::arch::asm!("out dx, al", in("dx") 0x3F8_u16, in("al") hex[nibble], options(nostack));
            }
        }
        debug_syscall(b"\r\n");
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
