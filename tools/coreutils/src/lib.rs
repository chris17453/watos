//! WATOS System Call Interface for Core Utilities
//!
//! Provides syscall wrappers for native WATOS applications.

#![no_std]

use core::panic::PanicInfo;

/// WATOS Syscall numbers (matching native64.rs)
pub mod syscall {
    pub const SYS_EXIT: u64 = 0;
    pub const SYS_WRITE: u64 = 1;
    pub const SYS_READ: u64 = 2;
    pub const SYS_OPEN: u64 = 3;
    pub const SYS_CLOSE: u64 = 4;
}

/// Perform a syscall with up to 6 arguments
/// Uses INT 0x80 for syscalls (WATOS convention)
#[inline(always)]
unsafe fn syscall6(
    num: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64,
) -> u64 {
    let ret: u64;
    core::arch::asm!(
        "int 0x80",
        inlateout("rax") num => ret,
        in("rdi") arg1,
        in("rsi") arg2,
        in("rdx") arg3,
        in("r10") arg4,
        in("r8") arg5,
        in("r9") arg6,
        options(nostack)
    );
    ret
}

/// Write bytes to stdout
pub fn write(buf: &[u8]) -> usize {
    unsafe {
        syscall6(
            syscall::SYS_WRITE,
            1, // stdout
            buf.as_ptr() as u64,
            buf.len() as u64,
            0,
            0,
            0,
        ) as usize
    }
}

/// Write a string to stdout
pub fn print(s: &str) {
    write(s.as_bytes());
}

/// Write a string with newline
pub fn println(s: &str) {
    print(s);
    write(b"\n");
}

/// Exit the program
pub fn exit(code: i32) -> ! {
    unsafe {
        syscall6(syscall::SYS_EXIT, code as u64, 0, 0, 0, 0, 0);
        // Should never reach here
        loop {
            core::arch::asm!("hlt", options(nostack, nomem));
        }
    }
}

/// Panic handler for WATOS utilities
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    print("PANIC!\n");
    exit(1)
}
