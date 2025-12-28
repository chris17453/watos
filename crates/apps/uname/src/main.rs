//! WATOS uname command
//!
//! Print system information.

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use watos_syscall::numbers as syscall;

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

fn get_args() -> &'static str {
    unsafe {
        let ptr = syscall1(syscall::SYS_GETARGS, 0) as *const u8;
        if ptr.is_null() {
            return "";
        }
        // Find null terminator
        let mut len = 0;
        while *ptr.add(len) != 0 {
            len += 1;
            if len > 256 { break; }
        }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len))
    }
}

#[no_mangle]
extern "C" fn _start() -> ! {
    let args = get_args();

    // Parse options
    let show_all = args.contains("-a");
    let show_kernel = args.contains("-s") || args.is_empty();
    let show_nodename = args.contains("-n");
    let show_release = args.contains("-r");
    let show_version = args.contains("-v");
    let show_machine = args.contains("-m");
    let show_processor = args.contains("-p");
    let show_os = args.contains("-o");

    let mut first = true;

    if show_all || show_kernel {
        write_str("WATOS");
        first = false;
    }

    if show_all || show_nodename {
        if !first { write_str(" "); }
        write_str("watos");
        first = false;
    }

    if show_all || show_release {
        if !first { write_str(" "); }
        write_str("0.1.0");
        first = false;
    }

    if show_all || show_version {
        if !first { write_str(" "); }
        write_str("#1 SMP");
        first = false;
    }

    if show_all || show_machine {
        if !first { write_str(" "); }
        write_str("x86_64");
        first = false;
    }

    if show_all || show_processor {
        if !first { write_str(" "); }
        write_str("x86_64");
        first = false;
    }

    if show_all || show_os {
        if !first { write_str(" "); }
        write_str("WATOS");
    }

    write_str("\r\n");
    exit(0);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    exit(1);
}
