//! WATOS Console Application
//!
//! User-space terminal emulator that provides:
//! - VT100/ANSI terminal emulation
//! - Keyboard input with modifiers (Shift, Ctrl, Alt)
//! - Framebuffer rendering via syscalls
//!
//! This runs as a user-space app, NOT in the kernel.

#![no_std]
#![no_main]

extern crate alloc;

use core::panic::PanicInfo;
use watos_syscall::numbers as syscall;
use watos_terminal::console::ConsoleManager;
use watos_terminal::framebuffer::{FramebufferInfo, PixelFormat, SimpleFramebuffer};
use watos_terminal::keyboard::KeyCode;

// ============================================================================
// Raw Syscall Wrappers
// ============================================================================

#[inline(always)]
unsafe fn syscall0(num: u32) -> u64 {
    let ret: u64;
    core::arch::asm!(
        "int 0x80",
        in("eax") num,
        lateout("rax") ret,
        options(nostack)
    );
    ret
}

#[inline(always)]
unsafe fn syscall1(num: u32, arg1: u64) -> u64 {
    let ret: u64;
    core::arch::asm!(
        "int 0x80",
        in("eax") num,
        in("rdi") arg1,
        lateout("rax") ret,
        options(nostack)
    );
    ret
}

#[inline(always)]
unsafe fn syscall2(num: u32, arg1: u64, arg2: u64) -> u64 {
    let ret: u64;
    core::arch::asm!(
        "int 0x80",
        in("eax") num,
        in("rdi") arg1,
        in("rsi") arg2,
        lateout("rax") ret,
        options(nostack)
    );
    ret
}

#[inline(always)]
unsafe fn syscall3(num: u32, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    let ret: u64;
    core::arch::asm!(
        "int 0x80",
        in("eax") num,
        in("rdi") arg1,
        in("rsi") arg2,
        in("rdx") arg3,
        lateout("rax") ret,
        options(nostack)
    );
    ret
}

// ============================================================================
// System Call Helpers
// ============================================================================

fn fb_addr() -> u64 {
    unsafe { syscall0(syscall::SYS_FB_ADDR) }
}

fn fb_dimensions() -> (u32, u32, u32) {
    unsafe {
        let packed = syscall0(syscall::SYS_FB_DIMENSIONS);
        let width = (packed >> 32) as u32;
        let height = ((packed >> 16) & 0xFFFF) as u32;
        let pitch = (packed & 0xFFFF) as u32 * 4;
        (width, height, pitch)
    }
}

fn read_scancode() -> u8 {
    unsafe { syscall0(syscall::SYS_READ_SCANCODE) as u8 }
}

fn serial_write(s: &str) {
    unsafe {
        syscall3(syscall::SYS_WRITE, 0, s.as_ptr() as u64, s.len() as u64);
    }
}

fn exit(code: i32) -> ! {
    unsafe {
        syscall1(syscall::SYS_EXIT, code as u64);
    }
    loop {}
}

/// Execute a program by name
/// Returns: 0 on success, 1 on exec error, 2 on not found
fn exec_program(name: &str) -> u64 {
    unsafe {
        syscall2(syscall::SYS_EXEC, name.as_ptr() as u64, name.len() as u64)
    }
}

// ============================================================================
// Global Allocator (via syscalls)
// ============================================================================

use core::alloc::{GlobalAlloc, Layout};

struct SyscallAllocator;

unsafe impl GlobalAlloc for SyscallAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // Request memory from kernel
        syscall1(syscall::SYS_MALLOC, layout.size() as u64) as *mut u8
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // Return memory to kernel
        let _ = syscall3(syscall::SYS_FREE as u32, ptr as u64, layout.size() as u64, 0);
    }
}

#[global_allocator]
static ALLOCATOR: SyscallAllocator = SyscallAllocator;

// ============================================================================
// Entry Point
// ============================================================================

#[no_mangle]
extern "C" fn _start() -> ! {
    serial_write("[CONSOLE] Starting console app\r\n");

    // Get framebuffer info from kernel
    let fb_address = fb_addr();
    if fb_address == 0 {
        serial_write("[CONSOLE] ERROR: No framebuffer\r\n");
        exit(1);
    }

    let (width, height, pitch) = fb_dimensions();

    // Create framebuffer
    let fb_info = FramebufferInfo {
        width,
        height,
        pitch,
        bpp: 32,
        format: PixelFormat::Bgr, // UEFI GOP typically uses BGR
    };

    let mut framebuffer = unsafe {
        SimpleFramebuffer::new(fb_address as *mut u32, fb_info)
    };

    // Calculate terminal size (8x16 font)
    let cols = (width / 8) as usize;
    let rows = (height / 16) as usize;

    // Create console manager with one terminal
    let mut console = ConsoleManager::new(cols, rows);
    console.init_consoles(1);

    // Display welcome message
    console.write_str("\x1b[2J\x1b[H"); // Clear screen, home cursor
    console.write_str("WATOS Console v0.1\r\n");
    console.write_str("===================\r\n\r\n");
    console.write_str("Type 'help' for commands.\r\n\r\n");
    console.write_str("> ");

    // Initial render
    console.render(&mut framebuffer);
    serial_write("[CONSOLE] Ready\r\n");

    // Command buffer
    let mut cmd_buffer = [0u8; 256];
    let mut cmd_len = 0usize;

    // Main loop
    let mut loop_count: u64 = 0;
    loop {
        loop_count += 1;
        // Read keyboard
        let scancode = read_scancode();
        if scancode != 0 {
            // Debug: minimal - just show we got something
            serial_write("[K]");
            if let Some(event) = console.process_scancode(scancode) {
                if event.pressed {
                    // Get character from keyboard
                    if let Some(ch) = console.keyboard().to_char(&event) {
                        match ch {
                            '\r' | '\n' => {
                                // Enter - process command
                                serial_write("[ENTER] cmd_len=");
                                // Print cmd_len as digit
                                if cmd_len < 10 {
                                    let digit = b'0' + cmd_len as u8;
                                    unsafe {
                                        core::arch::asm!(
                                            "int 0x80",
                                            in("eax") 1u32,  // SYS_WRITE
                                            in("rdi") 0u64,
                                            in("rsi") &digit as *const u8 as u64,
                                            in("rdx") 1u64,
                                            options(nostack)
                                        );
                                    }
                                }
                                serial_write("\r\n");
                                console.write_str("\r\n");

                                if cmd_len > 0 {
                                    let cmd = core::str::from_utf8(&cmd_buffer[..cmd_len]).unwrap_or("");
                                    serial_write("[CMD] ");
                                    serial_write(cmd);
                                    serial_write("\r\n");
                                    process_command(&mut console, cmd);
                                    cmd_len = 0;
                                }

                                console.write_str("> ");
                            }
                            '\x08' => {
                                // Backspace
                                if cmd_len > 0 {
                                    cmd_len -= 1;
                                    console.write_str("\x08 \x08"); // Move back, space, move back
                                }
                            }
                            '\x7f' => {
                                // Delete (treat like backspace)
                                if cmd_len > 0 {
                                    cmd_len -= 1;
                                    console.write_str("\x08 \x08");
                                }
                            }
                            _ => {
                                // Regular character
                                if cmd_len < cmd_buffer.len() - 1 {
                                    cmd_buffer[cmd_len] = ch as u8;
                                    cmd_len += 1;

                                    // Echo character
                                    let mut buf = [0u8; 4];
                                    let s = ch.encode_utf8(&mut buf);
                                    console.write_str(s);
                                }
                            }
                        }
                    } else {
                        // Handle special keys
                        match event.key {
                            KeyCode::Up => console.write_str("\x1b[A"),
                            KeyCode::Down => console.write_str("\x1b[B"),
                            KeyCode::Right => console.write_str("\x1b[C"),
                            KeyCode::Left => console.write_str("\x1b[D"),
                            KeyCode::Home => console.write_str("\x1b[H"),
                            KeyCode::End => console.write_str("\x1b[F"),
                            KeyCode::PageUp => console.write_str("\x1b[5~"),
                            KeyCode::PageDown => console.write_str("\x1b[6~"),
                            _ => {}
                        }
                    }
                }

                // Render after input
                console.render(&mut framebuffer);
            }
        }

        // Update cursor blink
        console.tick();
    }
}

/// Process a command
fn process_command(console: &mut ConsoleManager, cmd: &str) {
    let cmd = cmd.trim();

    // Parse command and arguments
    let (program, _args) = match cmd.find(' ') {
        Some(pos) => (&cmd[..pos], &cmd[pos + 1..]),
        None => (cmd, ""),
    };

    match program {
        "help" => {
            serial_write("[CONSOLE] Builtin: help\r\n");
            console.write_str("Built-in commands:\r\n");
            console.write_str("  help    - Show this help\r\n");
            console.write_str("  clear   - Clear screen\r\n");
            console.write_str("  colors  - Test colors\r\n");
            console.write_str("  ver     - Show version\r\n");
            console.write_str("\r\nExternal programs in /apps/system:\r\n");
            console.write_str("  date    - Show current date/time\r\n");
            console.write_str("  echo    - Echo arguments to output\r\n");
        }
        "clear" | "cls" => {
            console.write_str("\x1b[2J\x1b[H");
        }
        "colors" => {
            console.write_str("Color test:\r\n");
            // 16 standard colors
            for i in 0..16 {
                let code = if i < 8 { 30 + i } else { 90 + i - 8 };
                console.write_str("\x1b[");
                write_num(console, code);
                console.write_str("m*\x1b[0m");
            }
            console.write_str("\r\n");
            // Background colors
            for i in 0..16 {
                let code = if i < 8 { 40 + i } else { 100 + i - 8 };
                console.write_str("\x1b[");
                write_num(console, code);
                console.write_str("m \x1b[0m");
            }
            console.write_str("\r\n");
        }
        "ver" | "version" => {
            console.write_str("WATOS Console v0.1\r\n");
            console.write_str("Terminal: watos-terminal crate\r\n");
        }
        "" => {}
        _ => {
            // Try to execute as external program
            serial_write("[CONSOLE] Executing: ");
            serial_write(program);
            serial_write("\r\n");

            let result = exec_program(program);

            // Debug: confirm we returned from exec
            serial_write("[CONSOLE] exec returned: ");
            if result == 0 {
                serial_write("success\r\n");
            } else if result == 1 {
                serial_write("error\r\n");
            } else if result == 2 {
                serial_write("not found\r\n");
            } else {
                serial_write("unknown\r\n");
            }

            match result {
                0 => {
                    // Success - program ran
                }
                1 => {
                    console.write_str("Error: Failed to execute '");
                    console.write_str(program);
                    console.write_str("'\r\n");
                }
                2 => {
                    console.write_str("Command not found: ");
                    console.write_str(program);
                    console.write_str("\r\n");
                }
                _ => {
                    console.write_str("Unknown error executing: ");
                    console.write_str(program);
                    console.write_str("\r\n");
                }
            }
        }
    }
}

/// Write a number to console
fn write_num(console: &mut ConsoleManager, n: u32) {
    if n >= 10 {
        write_num(console, n / 10);
    }
    let digit = (n % 10) as u8 + b'0';
    console.write(&[digit]);
}

/// Write hex to serial
fn serial_hex(val: u64) {
    let hex = b"0123456789ABCDEF";
    let mut buf = [0u8; 16];
    let mut i = 15;
    let mut v = val;

    if v == 0 {
        serial_write("0");
        return;
    }

    while v > 0 {
        buf[i] = hex[(v & 0xF) as usize];
        v >>= 4;
        if i > 0 {
            i -= 1;
        }
    }

    if let Ok(s) = core::str::from_utf8(&buf[i + 1..]) {
        serial_write(s);
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    serial_write("\r\n!!! CONSOLE PANIC !!!\r\n");
    exit(1);
}
