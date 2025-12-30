//! WATOS shutdown command - shutdown or halt the system
//!
//! Usage: shutdown [OPTIONS]
//!
//! Options:
//!   -h        Halt the system (power off)
//!   -r        Reboot the system (same as reboot command)
//!   -n        No sync before shutdown (immediate)
//!   --help    Show this help message
//!
//! Without options, performs a clean shutdown.

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

fn shutdown() -> ! {
    unsafe {
        syscall0(syscall::SYS_SHUTDOWN);
    }
    loop {}
}

fn reboot() -> ! {
    unsafe {
        syscall0(syscall::SYS_REBOOT);
    }
    loop {}
}

fn show_help() {
    write_str("Usage: shutdown [OPTIONS]\r\n");
    write_str("\r\n");
    write_str("Shutdown or halt the system.\r\n");
    write_str("\r\n");
    write_str("Options:\r\n");
    write_str("  -h        Halt the system (power off)\r\n");
    write_str("  -r        Reboot the system\r\n");
    write_str("  -n        No sync before shutdown (immediate)\r\n");
    write_str("  --help    Show this help message\r\n");
}

#[no_mangle]
extern "C" fn _start() -> ! {
    use core::ptr::addr_of_mut;
    static mut ARGS_BUF: [u8; 256] = [0u8; 256];

    let args_len = unsafe {
        let buf = &mut *addr_of_mut!(ARGS_BUF);
        get_args(buf)
    };
    let args = unsafe { &ARGS_BUF[..args_len] };

    let mut do_reboot = false;
    let mut show_help_flag = false;
    let mut i = 0;

    // Skip command name
    while i < args.len() && args[i] != b' ' {
        i += 1;
    }
    while i < args.len() && args[i] == b' ' {
        i += 1;
    }

    // Parse options
    while i < args.len() {
        if args[i] == b'-' {
            i += 1;
            if i < args.len() && args[i] == b'-' {
                // Long option
                i += 1;
                let opt_start = i;
                while i < args.len() && args[i] != b' ' {
                    i += 1;
                }
                let opt = &args[opt_start..i];
                if opt == b"help" {
                    show_help_flag = true;
                }
            } else {
                // Short options
                while i < args.len() && args[i] != b' ' {
                    match args[i] {
                        b'h' => {} // Halt (default behavior)
                        b'r' => do_reboot = true,
                        b'n' => {} // No sync (we don't sync anyway yet)
                        _ => {}
                    }
                    i += 1;
                }
            }
        }
        while i < args.len() && args[i] == b' ' {
            i += 1;
        }
        // Skip any non-option arguments
        while i < args.len() && args[i] != b' ' && args[i] != b'-' {
            i += 1;
        }
        while i < args.len() && args[i] == b' ' {
            i += 1;
        }
    }

    if show_help_flag {
        show_help();
        exit(0);
    }

    if do_reboot {
        write_str("Rebooting system...\r\n");
        reboot();
    } else {
        write_str("Shutting down system...\r\n");
        shutdown();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    exit(1);
}
