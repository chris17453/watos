//! Interrupt handling for WATOS kernel
//!
//! Uses watos-arch for low-level IDT/PIC management.
//! Provides syscall handler (INT 0x80) and VGA syscalls.

use core::arch::asm;

// External functions from main.rs
extern "C" {
    fn watos_set_cursor(x: u32, y: u32);
    fn watos_clear_screen();
    fn fb_clear_impl(r: u8, g: u8, b: u8);
    fn fb_put_pixel(x: u32, y: u32, r: u8, g: u8, b: u8);
    fn fb_get_pixel(x: i32, y: i32) -> u8;
    fn fb_clear_screen(r: u8, g: u8, b: u8);
}

// Re-export from watos-arch for convenience
pub use watos_arch::idt::{get_scancode, get_ticks, sleep_ms};
pub use watos_arch::pic::{enable_timer, disable_timer};
pub use watos_arch::halt;

/// Initialize interrupt system
pub fn init() {
    // PIC and IDT are initialized by watos_arch
    // Just need to make sure they're set up
    watos_arch::pic::init();
    watos_arch::idt::init();

    // Initialize VGA palette
    init_default_palette();
}

/// Install syscall handler at INT 0x80 (user-callable)
pub fn init_syscalls() {
    watos_arch::idt::install_syscall_handler(syscall_handler_asm);

    unsafe {
        watos_arch::serial_write(b"[SYSCALL] INT 0x80 handler installed at 0x");
        watos_arch::serial_hex(syscall_handler_asm as u64);
        watos_arch::serial_write(b"\r\n");
    }
}

// =============================================================================
// Syscall Handler (INT 0x80)
// =============================================================================

/// WATOS Syscall numbers - import from shared crate
pub use watos_syscall::numbers as syscall;

/// Syscall context passed from assembly handler
#[repr(C)]
pub struct SyscallContext {
    pub rax: u64,  // syscall number / return value
    pub rcx: u64,
    pub rdx: u64,  // arg3
    pub rsi: u64,  // arg2
    pub rdi: u64,  // arg1
    pub r8: u64,   // arg5
    pub r9: u64,   // arg6
    pub r10: u64,  // arg4
    pub r11: u64,
    pub rbp: u64,
}

/// Debug helper for syscalls
unsafe fn debug_syscall(msg: &[u8]) {
    watos_arch::serial_write(msg);
}

/// Syscall handler - dispatches based on syscall number
#[no_mangle]
pub extern "C" fn syscall_handler(ctx: &mut SyscallContext) {
    unsafe {
        debug_syscall(b"[ENTER] ");
    }

    let syscall_num = ctx.rax as u32;

    unsafe {
        debug_syscall(b"[SYSCALL] ");
        watos_arch::serial_hex_byte((syscall_num & 0xFF) as u8);
        debug_syscall(b" from PID=");
        if let Some(pid) = crate::process::current_pid() {
            watos_arch::serial_hex_byte(pid as u8);
        } else {
            debug_syscall(b"kernel");
        }
        debug_syscall(b"\r\n");
    }

    ctx.rax = match syscall_num {
        syscall::SYS_WRITE => {
            let handle = ctx.rdi as u32;
            let buf = ctx.rsi as *const u8;
            let len = ctx.rdx as usize;

            if !buf.is_null() && len > 0 {
                let mut data = alloc::vec::Vec::with_capacity(len);
                unsafe {
                    for i in 0..len {
                        data.push(*buf.add(i));
                    }
                }

                if let Some(handle_table) = crate::process::current_handle_table() {
                    if let Ok(bytes_written) = crate::io::HandleIO::write_file(handle_table, handle, &data) {
                        bytes_written as u64
                    } else if let Ok(bytes_written) = crate::io::HandleIO::write_console(handle_table, handle, &data) {
                        bytes_written as u64
                    } else {
                        crate::io::fs_error_to_errno(crate::disk::FsError::NotFound)
                    }
                } else {
                    crate::console::print(&data);
                    len as u64
                }
            } else {
                0
            }
        }

        syscall::SYS_READ => {
            let handle = ctx.rdi as u32;
            let buf = ctx.rsi as *mut u8;
            let max_len = ctx.rdx as usize;

            if !buf.is_null() && max_len > 0 {
                let mut buffer = alloc::vec![0u8; max_len];

                if let Some(handle_table) = crate::process::current_handle_table() {
                    let bytes_read = if let Ok(n) = crate::io::HandleIO::read_file(handle_table, handle, &mut buffer) {
                        n
                    } else if let Ok(n) = crate::io::HandleIO::read_console(handle_table, handle, &mut buffer) {
                        n
                    } else {
                        return ctx.rax = crate::io::fs_error_to_errno(crate::disk::FsError::NotFound);
                    };

                    unsafe {
                        for i in 0..bytes_read {
                            *buf.add(i) = buffer[i];
                        }
                    }
                    bytes_read as u64
                } else {
                    if let Some(scancode) = get_scancode() {
                        let ascii = match scancode {
                            0x1C => b'\n',
                            0x39 => b' ',
                            0x1E..=0x26 => b'a' + (scancode - 0x1E),
                            0x10..=0x19 => b'q' + (scancode - 0x10),
                            0x2C..=0x32 => b'z' + (scancode - 0x2C),
                            0x02..=0x0B => b'1' + (scancode - 0x02),
                            _ => b'?',
                        };
                        unsafe { *buf = ascii; }
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
            let path_ptr = ctx.rdi as *const u8;
            let path_len = ctx.rsi as usize;
            let mode = ctx.rdx;

            if !path_ptr.is_null() && path_len > 0 && path_len < 256 {
                let mut path_bytes = alloc::vec![0u8; path_len];
                unsafe {
                    for i in 0..path_len {
                        path_bytes[i] = *path_ptr.add(i);
                    }
                }

                if let Ok(path_str) = alloc::string::String::from_utf8(path_bytes) {
                    if let Some(handle_table) = crate::process::current_handle_table() {
                        let open_mode = crate::io::mode_from_u64(mode);
                        match crate::io::HandleIO::open(handle_table, &path_str, open_mode, 1) {
                            Ok(handle) => handle as u64,
                            Err(error) => crate::io::fs_error_to_errno(error),
                        }
                    } else {
                        crate::io::fs_error_to_errno(crate::disk::FsError::NotSupported)
                    }
                } else {
                    crate::io::fs_error_to_errno(crate::disk::FsError::InvalidName)
                }
            } else {
                crate::io::fs_error_to_errno(crate::disk::FsError::InvalidName)
            }
        }

        syscall::SYS_CLOSE => {
            let handle = ctx.rdi as u32;
            if let Some(handle_table) = crate::process::current_handle_table() {
                match crate::io::HandleIO::close(handle_table, handle) {
                    Ok(_) => 0,
                    Err(error) => crate::io::fs_error_to_errno(error),
                }
            } else {
                crate::io::fs_error_to_errno(crate::disk::FsError::NotSupported)
            }
        }

        syscall::SYS_GETKEY => {
            if let Some(scancode) = get_scancode() {
                scancode as u64
            } else {
                0
            }
        }

        syscall::SYS_EXIT => {
            let code = ctx.rdi as i32;
            crate::process::exit_current(code);
            crate::process::process_exit_to_kernel(code);
        }

        syscall::SYS_SLEEP => {
            let ms = ctx.rdi as u32;
            sleep_ms(ms);
            0
        }

        syscall::SYS_GETPID => {
            if let Some(pid) = crate::process::current_pid() {
                pid as u64
            } else {
                0
            }
        }

        syscall::SYS_TIME => get_ticks(),

        syscall::SYS_MALLOC => {
            let size = ctx.rdi as usize;
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

        syscall::SYS_FREE => 0,

        syscall::SYS_PUTCHAR => {
            let ch = ctx.rdi as u8;
            crate::console::print(&[ch]);
            1
        }

        syscall::SYS_CURSOR => {
            let x = ctx.rdi as u32;
            let y = ctx.rsi as u32;
            unsafe { watos_set_cursor(x, y); }
            0
        }

        syscall::SYS_CLEAR => {
            unsafe { watos_clear_screen(); }
            0
        }

        syscall::SYS_COLOR => 0,

        syscall::SYS_CONSOLE_IN => {
            if let Some(handle_table) = crate::process::current_handle_table() {
                handle_table.add_console_handle(crate::io::ConsoleKind::Stdin) as u64
            } else {
                crate::io::fs_error_to_errno(crate::disk::FsError::NotSupported)
            }
        }

        syscall::SYS_CONSOLE_OUT => {
            if let Some(handle_table) = crate::process::current_handle_table() {
                handle_table.add_console_handle(crate::io::ConsoleKind::Stdout) as u64
            } else {
                crate::io::fs_error_to_errno(crate::disk::FsError::NotSupported)
            }
        }

        syscall::SYS_CONSOLE_ERR => {
            if let Some(handle_table) = crate::process::current_handle_table() {
                handle_table.add_console_handle(crate::io::ConsoleKind::Stderr) as u64
            } else {
                crate::io::fs_error_to_errno(crate::disk::FsError::NotSupported)
            }
        }

        syscall::SYS_GFX_PSET => {
            vga_set_pixel(ctx.rdi as i32, ctx.rsi as i32, ctx.rdx as u8);
            0
        }

        syscall::SYS_GFX_LINE => 0,
        syscall::SYS_GFX_CIRCLE => 0,

        syscall::SYS_GFX_CLS => {
            unsafe { fb_clear_impl(0, 0, 0); }
            0
        }

        syscall::SYS_GFX_MODE => 0,
        syscall::SYS_GFX_DISPLAY => 0,

        syscall::SYS_VGA_SET_MODE => vga_set_mode(ctx.rdi as u8),

        syscall::SYS_VGA_SET_PIXEL => {
            vga_set_pixel(ctx.rdi as i32, ctx.rsi as i32, ctx.rdx as u8);
            0
        }

        syscall::SYS_VGA_GET_PIXEL => vga_get_pixel(ctx.rdi as i32, ctx.rsi as i32) as u64,

        syscall::SYS_VGA_BLIT => {
            vga_blit(ctx.rdi as *const u8, ctx.rsi as usize, ctx.rdx as usize, ctx.r10 as usize);
            0
        }

        syscall::SYS_VGA_CLEAR => {
            vga_clear(ctx.rdi as u8);
            0
        }

        syscall::SYS_VGA_FLIP => 0,

        syscall::SYS_VGA_SET_PALETTE => {
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

        _ => u64::MAX,
    };
}

/// Syscall handler assembly stub
#[unsafe(naked)]
pub unsafe extern "C" fn syscall_handler_asm() {
    core::arch::naked_asm!(
        // Debug: write 'S' to serial
        "push rax",
        "push rdx",
        "mov al, 83",
        "mov dx, 0x3F8",
        "out dx, al",
        "pop rdx",
        "pop rax",

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
        "push rbp",

        // Debug: write 'C'
        "push rax",
        "push rdx",
        "mov al, 67",
        "mov dx, 0x3F8",
        "out dx, al",
        "pop rdx",
        "pop rax",

        // Call Rust handler
        "mov rbp, rsp",
        "mov rdi, rsp",
        "call {handler}",

        // Debug: write 'R'
        "push rax",
        "push rdx",
        "mov al, 82",
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

// =============================================================================
// VGA Syscall Implementations
// =============================================================================

static mut VGA_MODE: u8 = 0;
static mut VGA_WIDTH: usize = 0;
static mut VGA_HEIGHT: usize = 0;
static mut VGA_PALETTE: [u32; 256] = [0; 256];

fn init_default_palette() {
    unsafe {
        const PALETTE_16: [u32; 16] = [
            0x000000, 0x0000AA, 0x00AA00, 0x00AAAA,
            0xAA0000, 0xAA00AA, 0xAA5500, 0xAAAAAA,
            0x555555, 0x5555FF, 0x55FF55, 0x55FFFF,
            0xFF5555, 0xFF55FF, 0xFFFF55, 0xFFFFFF,
        ];

        for i in 0..16 {
            VGA_PALETTE[i] = PALETTE_16[i];
        }

        for i in 16..256 {
            let gray = ((i - 16) * 255 / 239) as u8;
            VGA_PALETTE[i] = ((gray as u32) << 16) | ((gray as u32) << 8) | (gray as u32);
        }
    }
}

fn vga_set_mode(mode: u8) -> u64 {
    unsafe {
        VGA_MODE = mode;
        match mode {
            0 => { VGA_WIDTH = 80; VGA_HEIGHT = 25; }
            1 => { VGA_WIDTH = 320; VGA_HEIGHT = 200; }
            2 => { VGA_WIDTH = 640; VGA_HEIGHT = 200; }
            3 => { VGA_WIDTH = 640; VGA_HEIGHT = 480; }
            _ => return u64::MAX,
        }
        0
    }
}

fn vga_set_pixel(x: i32, y: i32, color: u8) {
    if x < 0 || y < 0 { return; }
    let rgb = unsafe { VGA_PALETTE[color as usize] };
    let r = ((rgb >> 16) & 0xFF) as u8;
    let g = ((rgb >> 8) & 0xFF) as u8;
    let b = (rgb & 0xFF) as u8;
    unsafe { fb_put_pixel(x as u32, y as u32, r, g, b); }
}

fn vga_get_pixel(x: i32, y: i32) -> u8 {
    unsafe { fb_get_pixel(x, y) }
}

fn vga_blit(buf: *const u8, width: usize, height: usize, stride: usize) {
    if buf.is_null() { return; }

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
    unsafe { fb_clear_screen(r, g, b); }
}
