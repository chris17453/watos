//! WATOS Echo Application
//!
//! Echoes command line arguments.

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use watos_syscall::numbers as syscall;

/// Raw syscall wrapper functions
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

/// Write string to stdout
fn write_str(s: &str) {
    unsafe {
        syscall3(syscall::SYS_WRITE, 1, s.as_ptr() as u64, s.len() as u64);
    }
}

/// Write bytes to stdout
fn write_bytes(buf: &[u8]) {
    unsafe {
        syscall3(syscall::SYS_WRITE, 1, buf.as_ptr() as u64, buf.len() as u64);
    }
}

/// Get command line arguments into buffer
fn get_args(buf: &mut [u8]) -> usize {
    unsafe {
        syscall2(syscall::SYS_GETARGS, buf.as_mut_ptr() as u64, buf.len() as u64) as usize
    }
}

/// Exit with code
fn exit(code: i32) -> ! {
    unsafe {
        syscall1(syscall::SYS_EXIT, code as u64);
    }
    loop {}
}

/// Entry point for WATOS applications
#[no_mangle]
extern "C" fn _start() -> ! {
    // Debug: show we started
    write_str("[ECHO] Starting\r\n");

    // Buffer for command line args - use static to avoid stack issues
    static mut ARGS_BUF: [u8; 256] = [0u8; 256];

    write_str("[ECHO] Getting args\r\n");
    let args_len = unsafe { get_args(&mut ARGS_BUF) };

    write_str("[ECHO] Got args, len=");
    // Print length as single digit for simplicity
    if args_len < 10 {
        let digit = b'0' + args_len as u8;
        write_bytes(&[digit]);
    } else {
        write_str("10+");
    }
    write_str("\r\n");

    if args_len > 0 {
        // Find the arguments after the program name
        let args = unsafe { &ARGS_BUF[..args_len] };

        // Skip the program name (first word) and the space after it
        let mut i = 0;
        while i < args_len && args[i] != b' ' {
            i += 1;
        }
        // Skip the space
        if i < args_len && args[i] == b' ' {
            i += 1;
        }

        // Print everything after the program name
        if i < args_len {
            write_bytes(&args[i..]);
        }
    }

    // Print newline
    write_str("\r\n");

    write_str("[ECHO] Exiting\r\n");
    exit(0);
}

/// Panic handler
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    write_str("PANIC in echo\r\n");
    exit(1);
}
