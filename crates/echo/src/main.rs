//! WATOS Echo Application
//! 
//! Demonstrates proper WATOS handle-based I/O.
//! In WATOS, processes must explicitly request console access.

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use watos_syscall::numbers as syscall;

/// Raw syscall wrapper functions
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

/// Write string to handle
unsafe fn write_to_handle(handle: u64, s: &str) {
    let bytes = s.as_bytes();
    syscall3(syscall::SYS_WRITE, handle, bytes.as_ptr() as u64, bytes.len() as u64);
}

/// Entry point for WATOS applications
#[no_mangle]
extern "C" fn _start() -> ! {
    unsafe {
        // Request stdout handle - explicit console access in WATOS
        let stdout_handle = syscall0(syscall::SYS_CONSOLE_OUT);
        
        // Check if we got a valid handle (errors are > 2^32)
        if stdout_handle < 0x100000000 {
            write_to_handle(stdout_handle, "Hello from WATOS echo!\n");
            write_to_handle(stdout_handle, "This demonstrates handle-based I/O.\n");
            write_to_handle(stdout_handle, "Process requested console access explicitly.\n");
        }
        
        // Exit with success
        syscall1(syscall::SYS_EXIT, 0);
    }
    
    loop {} // Should never reach here
}

/// Panic handler - also uses handle-based I/O
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    unsafe {
        // Try to get stderr handle for error output
        let stderr_handle = syscall0(syscall::SYS_CONSOLE_ERR);
        if stderr_handle < 0x100000000 {
            write_to_handle(stderr_handle, "PANIC in echo application!\n");
        }
        
        syscall1(syscall::SYS_EXIT, 1);
    }
    
    loop {}
}