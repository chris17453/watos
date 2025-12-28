//! WATOS ps command
//!
//! Display process information.
//! Currently shows only the current process (full process listing not yet implemented).

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use watos_syscall::numbers as syscall;

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

fn write_str(s: &str) {
    unsafe {
        syscall3(syscall::SYS_WRITE, 1, s.as_ptr() as u64, s.len() as u64);
    }
}

fn exit(code: i32) -> ! {
    unsafe {
        syscall1(syscall::SYS_EXIT, code as u64);
    }
    loop {}
}

fn get_pid() -> u32 {
    unsafe { syscall0(syscall::SYS_GETPID) as u32 }
}

fn write_num(n: u32) {
    if n >= 10 {
        write_num(n / 10);
    }
    let digit = (n % 10) as u8 + b'0';
    unsafe {
        syscall3(syscall::SYS_WRITE, 1, &digit as *const u8 as u64, 1);
    }
}

#[no_mangle]
extern "C" fn _start() -> ! {
    // Print header
    write_str("  PID CMD\r\n");

    // Show current process
    let pid = get_pid();
    write_str("    ");
    write_num(pid);
    write_str(" ps\r\n");

    exit(0);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    exit(1);
}
