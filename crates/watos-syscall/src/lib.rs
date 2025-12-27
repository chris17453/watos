//! WATOS System Call Interface
//!
//! This crate provides the canonical system call definitions and wrapper functions
//! for WATOS. It's used by both the kernel and user applications to ensure
//! consistency in the syscall ABI.

#![cfg_attr(feature = "no-std", no_std)]

/// WATOS System Call Numbers - AUTHORITATIVE DEFINITION
pub mod numbers {
    // Console/IO
    pub const SYS_WRITE: u32 = 1;
    pub const SYS_READ: u32 = 2;
    pub const SYS_OPEN: u32 = 3;
    pub const SYS_CLOSE: u32 = 4;
    pub const SYS_GETKEY: u32 = 5;
    pub const SYS_EXIT: u32 = 6;

    // System
    pub const SYS_SLEEP: u32 = 11;
    pub const SYS_GETPID: u32 = 12;
    pub const SYS_TIME: u32 = 13;
    pub const SYS_MALLOC: u32 = 14;
    pub const SYS_FREE: u32 = 15;

    // Additional syscalls
    pub const SYS_PUTCHAR: u32 = 16;
    pub const SYS_CURSOR: u32 = 17;
    pub const SYS_CLEAR: u32 = 18;
    pub const SYS_COLOR: u32 = 19;
    
    // Console handle management
    pub const SYS_CONSOLE_IN: u32 = 20;    // Get stdin handle
    pub const SYS_CONSOLE_OUT: u32 = 21;   // Get stdout handle
    pub const SYS_CONSOLE_ERR: u32 = 22;   // Get stderr handle

    // VGA Graphics
    pub const SYS_VGA_SET_MODE: u32 = 30;
    pub const SYS_VGA_SET_PIXEL: u32 = 31;
    pub const SYS_VGA_GET_PIXEL: u32 = 32;
    pub const SYS_VGA_BLIT: u32 = 33;
    pub const SYS_VGA_CLEAR: u32 = 34;
    pub const SYS_VGA_FLIP: u32 = 35;
    pub const SYS_VGA_SET_PALETTE: u32 = 36;
    
    // Graphics extensions
    pub const SYS_GFX_PSET: u32 = 40;      // Set pixel
    pub const SYS_GFX_LINE: u32 = 41;      // Draw line
    pub const SYS_GFX_CIRCLE: u32 = 42;    // Draw circle
    pub const SYS_GFX_CLS: u32 = 43;       // Clear graphics screen
    pub const SYS_GFX_MODE: u32 = 44;      // Set graphics mode
    pub const SYS_GFX_DISPLAY: u32 = 45;   // Display graphics buffer
}

/// Raw syscall interface - performs INT 0x80
///
/// # Safety
/// This function performs a raw system call. The caller must ensure:
/// - The syscall number is valid
/// - Arguments match the expected types for the syscall
/// - Pointers (if any) are valid and point to accessible memory
#[inline(always)]
pub unsafe fn raw_syscall0(num: u32) -> u64 {
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
pub unsafe fn raw_syscall1(num: u32, arg1: u64) -> u64 {
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
pub unsafe fn raw_syscall2(num: u32, arg1: u64, arg2: u64) -> u64 {
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
pub unsafe fn raw_syscall3(num: u32, arg1: u64, arg2: u64, arg3: u64) -> u64 {
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

/// High-level syscall wrappers
pub mod syscalls {
    use super::{numbers::*, raw_syscall0, raw_syscall1, raw_syscall2, raw_syscall3};

    /// Exit the current process
    pub fn exit(code: i32) -> ! {
        unsafe {
            raw_syscall1(SYS_EXIT, code as u64);
        }
        loop {}
    }

    /// Write data to a file descriptor
    pub fn write(fd: i32, buf: &[u8]) -> usize {
        unsafe {
            raw_syscall3(SYS_WRITE, fd as u64, buf.as_ptr() as u64, buf.len() as u64) as usize
        }
    }

    /// Read data from a file descriptor
    pub fn read(fd: i32, buf: &mut [u8]) -> usize {
        unsafe {
            raw_syscall3(SYS_READ, fd as u64, buf.as_mut_ptr() as u64, buf.len() as u64) as usize
        }
    }

    /// Open a file
    pub fn open(path: &str, mode: u32) -> i32 {
        unsafe {
            raw_syscall3(SYS_OPEN, path.as_ptr() as u64, path.len() as u64, mode as u64) as i32
        }
    }

    /// Close a file
    pub fn close(fd: i32) -> i32 {
        unsafe {
            raw_syscall1(SYS_CLOSE, fd as u64) as i32
        }
    }

    /// Get a key without blocking
    pub fn getkey() -> u8 {
        unsafe {
            raw_syscall0(SYS_GETKEY) as u8
        }
    }

    /// Sleep for milliseconds
    pub fn sleep(ms: u32) {
        unsafe {
            raw_syscall1(SYS_SLEEP, ms as u64);
        }
    }

    /// Get current process ID
    pub fn getpid() -> u32 {
        unsafe {
            raw_syscall0(SYS_GETPID) as u32
        }
    }

    /// Get system time (ticks since boot)
    pub fn time() -> u64 {
        unsafe {
            raw_syscall0(SYS_TIME)
        }
    }

    /// Allocate memory
    pub fn malloc(size: usize) -> *mut u8 {
        unsafe {
            raw_syscall1(SYS_MALLOC, size as u64) as *mut u8
        }
    }

    /// Free memory
    pub fn free(ptr: *mut u8) {
        unsafe {
            raw_syscall1(SYS_FREE, ptr as u64);
        }
    }

    /// Output a single character
    pub fn putchar(ch: char) {
        unsafe {
            raw_syscall1(SYS_PUTCHAR, ch as u64);
        }
    }

    /// Set cursor position
    pub fn set_cursor(x: u32, y: u32) {
        unsafe {
            raw_syscall2(SYS_CURSOR, x as u64, y as u64);
        }
    }

    /// Clear screen
    pub fn clear_screen() {
        unsafe {
            raw_syscall0(SYS_CLEAR);
        }
    }

    /// Set text color
    pub fn set_color(color: u32) {
        unsafe {
            raw_syscall1(SYS_COLOR, color as u64);
        }
    }

    /// Set VGA graphics mode
    pub fn vga_set_mode(mode: u8) -> u64 {
        unsafe {
            raw_syscall1(SYS_VGA_SET_MODE, mode as u64)
        }
    }

    /// Set pixel in graphics mode
    pub fn vga_set_pixel(x: i32, y: i32, color: u8) {
        unsafe {
            raw_syscall3(SYS_VGA_SET_PIXEL, x as u64, y as u64, color as u64);
        }
    }

    /// Get pixel from graphics mode
    pub fn vga_get_pixel(x: i32, y: i32) -> u8 {
        unsafe {
            raw_syscall2(SYS_VGA_GET_PIXEL, x as u64, y as u64) as u8
        }
    }
}

/// Convenience functions for common operations
pub mod io {
    use super::syscalls;

    /// Print a string to stdout
    pub fn print(s: &str) {
        syscalls::write(1, s.as_bytes());
    }

    /// Print a string with newline
    pub fn println(s: &str) {
        print(s);
        print("\n");
    }

    /// Read a single character from keyboard
    pub fn getch() -> char {
        loop {
            let key = syscalls::getkey();
            if key != 0 {
                return key as char;
            }
            // Small delay to avoid busy waiting
            syscalls::sleep(1);
        }
    }
}