//! WATOS Login Application
//!
//! Provides user authentication and launches console sessions.
//! Runs as the initial application instead of going directly to shell.

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use watos_syscall::numbers as syscall;

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
// Helper Functions
// ============================================================================

fn write_str(s: &str) {
    unsafe {
        syscall3(syscall::SYS_WRITE, 1, s.as_ptr() as u64, s.len() as u64);
    }
}

fn read_line(buf: &mut [u8]) -> usize {
    let mut pos = 0;
    loop {
        // Wait for keypress
        let key = loop {
            let k = unsafe { syscall0(syscall::SYS_GETKEY) as u8 };
            if k != 0 {
                break k;
            }
            // Small delay to avoid busy-waiting
            for _ in 0..10000 { core::hint::spin_loop(); }
        };

        if key == b'\n' || key == b'\r' {
            write_str("\r\n");
            break;
        } else if key == 0x08 || key == 0x7F { // Backspace or DEL
            if pos > 0 {
                pos -= 1;
                write_str("\x08 \x08"); // Backspace, space, backspace
            }
        } else if key >= 0x20 && key < 0x7F && pos < buf.len() {
            buf[pos] = key;
            pos += 1;
            // Echo character (or asterisk for password)
            let echo = [key];
            unsafe {
                syscall3(syscall::SYS_WRITE, 1, echo.as_ptr() as u64, 1);
            }
        }
    }
    pos
}

fn read_password(buf: &mut [u8]) -> usize {
    let mut pos = 0;
    loop {
        // Wait for keypress
        let key = loop {
            let k = unsafe { syscall0(syscall::SYS_GETKEY) as u8 };
            if k != 0 {
                break k;
            }
            // Small delay to avoid busy-waiting
            for _ in 0..10000 { core::hint::spin_loop(); }
        };

        if key == b'\n' || key == b'\r' {
            write_str("\r\n");
            break;
        } else if key == 0x08 || key == 0x7F { // Backspace or DEL
            if pos > 0 {
                pos -= 1;
                write_str("\x08 \x08"); // Backspace, space, backspace
            }
        } else if key >= 0x20 && key < 0x7F && pos < buf.len() {
            buf[pos] = key;
            pos += 1;
            // Echo asterisk instead of actual character
            write_str("*");
        }
    }
    pos
}

fn exec_console() {
    // Execute the shell
    let cmd = b"shell";
    unsafe {
        syscall2(syscall::SYS_EXEC, cmd.as_ptr() as u64, cmd.len() as u64);
    }
}

fn exit(code: i32) -> ! {
    unsafe {
        syscall1(syscall::SYS_EXIT, code as u64);
    }
    loop {}
}

// ============================================================================
// Main Entry Point
// ============================================================================

#[no_mangle]
pub extern "C" fn _start() -> ! {
    write_str("\r\n");
    write_str("===============================================\r\n");
    write_str("     WATOS - Welcome to the Operating System  \r\n");
    write_str("===============================================\r\n");
    write_str("\r\n");

    let mut username_buf = [0u8; 32];
    let mut password_buf = [0u8; 64];
    let mut attempts = 0;

    loop {
        write_str("Login: ");
        let username_len = read_line(&mut username_buf);
        
        if username_len == 0 {
            write_str("Username cannot be empty\r\n");
            continue;
        }

        write_str("Password: ");
        let password_len = read_password(&mut password_buf);

        // Call authentication syscall
        let username = &username_buf[..username_len];
        let password = &password_buf[..password_len];
        
        let uid = unsafe {
            syscall3(
                syscall::SYS_AUTHENTICATE,
                username.as_ptr() as u64,
                username_len as u64,
                password.as_ptr() as u64,
            )
        };

        // Clear password from memory for security (volatile write to prevent optimization)
        for i in 0..password_buf.len() {
            unsafe {
                core::ptr::write_volatile(&mut password_buf[i], 0);
            }
        }

        if uid != u64::MAX {
            // Authentication successful
            write_str("\r\nLogin successful! Welcome, ");
            unsafe {
                syscall3(syscall::SYS_WRITE, 1, username.as_ptr() as u64, username_len as u64);
            }
            write_str("\r\n\r\n");
            
            // Set current user context
            unsafe {
                syscall1(syscall::SYS_SETUID, uid);
            }

            // Launch console/shell
            exec_console();
            
            // If exec fails, exit
            exit(0);
        } else {
            // Authentication failed
            attempts += 1;
            write_str("\r\nLogin incorrect\r\n");
            
            if attempts >= 3 {
                write_str("Too many failed attempts. Please try again later.\r\n");
                // Wait a bit before allowing retry
                for _ in 0..100_000_000 { core::hint::spin_loop(); }
                attempts = 0;
            }
            write_str("\r\n");
        }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
