//! WATOS mkfifo command - create named pipes (FIFOs)
//!
//! Usage: mkfifo NAME...
//!
//! Create named pipes (FIFOs) with the given NAMEs.

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use watos_syscall::numbers as syscall;

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

fn write_str(s: &str) {
    unsafe {
        syscall3(syscall::SYS_WRITE, 1, s.as_ptr() as u64, s.len() as u64);
    }
}

fn write_bytes(b: &[u8]) {
    unsafe {
        syscall3(syscall::SYS_WRITE, 1, b.as_ptr() as u64, b.len() as u64);
    }
}

fn exit(code: i32) -> ! {
    unsafe {
        let _: u64;
        core::arch::asm!(
            "int 0x80",
            in("eax") syscall::SYS_EXIT,
            in("rdi") code as u64,
            lateout("rax") _,
            options(nostack)
        );
    }
    loop {}
}

fn get_args(buf: &mut [u8]) -> usize {
    unsafe { syscall2(syscall::SYS_GETARGS, buf.as_mut_ptr() as u64, buf.len() as u64) as usize }
}

fn mkfifo(path: &[u8]) -> u64 {
    unsafe {
        syscall2(syscall::SYS_MKFIFO, path.as_ptr() as u64, path.len() as u64)
    }
}

#[no_mangle]
extern "C" fn _start() -> ! {
    static mut ARGS_BUF: [u8; 512] = [0u8; 512];

    let args_len = unsafe { get_args(&mut ARGS_BUF) };
    let args = unsafe { &ARGS_BUF[..args_len] };

    let mut i = 0;
    let mut exit_code = 0;
    let mut created_any = false;

    // Skip command name "mkfifo"
    while i < args.len() && args[i] != b' ' {
        i += 1;
    }
    while i < args.len() && args[i] == b' ' {
        i += 1;
    }

    // No arguments?
    if i >= args.len() {
        write_str("mkfifo: missing operand\r\n");
        write_str("Usage: mkfifo NAME...\r\n");
        exit(1);
    }

    // Process each name
    while i < args.len() {
        let name_start = i;
        while i < args.len() && args[i] != b' ' {
            i += 1;
        }
        let name = &args[name_start..i];

        if !name.is_empty() {
            let result = mkfifo(name);
            if result != 0 {
                write_str("mkfifo: cannot create fifo '");
                write_bytes(name);
                write_str("'\r\n");
                exit_code = 1;
            } else {
                created_any = true;
            }
        }

        // Skip spaces
        while i < args.len() && args[i] == b' ' {
            i += 1;
        }
    }

    if !created_any && exit_code == 0 {
        write_str("mkfifo: missing operand\r\n");
        exit(1);
    }

    exit(exit_code);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    exit(1);
}
