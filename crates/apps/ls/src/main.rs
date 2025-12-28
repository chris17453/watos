//! WATOS ls command - list directory contents

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

fn readdir(path: &[u8], buf: &mut [u8]) -> usize {
    unsafe {
        syscall3(
            syscall::SYS_READDIR,
            path.as_ptr() as u64,
            path.len() as u64,
            buf.as_mut_ptr() as u64,
        ) as usize
    }
}

#[no_mangle]
extern "C" fn _start() -> ! {
    static mut ARGS_BUF: [u8; 256] = [0u8; 256];
    static mut DIR_BUF: [u8; 2048] = [0u8; 2048];

    let args_len = unsafe { get_args(&mut ARGS_BUF) };
    let args = unsafe { &ARGS_BUF[..args_len] };

    // Parse path from args (skip "ls" command itself)
    let path = {
        let mut i = 0;
        // Skip command name
        while i < args_len && args[i] != b' ' {
            i += 1;
        }
        // Skip space
        if i < args_len && args[i] == b' ' {
            i += 1;
        }
        // Rest is path (or empty for current dir)
        if i < args_len {
            &args[i..]
        } else {
            &[]
        }
    };

    // Read directory
    let len = unsafe { readdir(path, &mut DIR_BUF) };

    if len == 0 {
        write_str("(empty directory)\r\n");
        exit(0);
    }

    let entries = unsafe { &DIR_BUF[..len] };

    // Parse and display entries
    // Format: "TYPE NAME SIZE\n" per line
    let mut line_start = 0;
    for i in 0..len {
        if entries[i] == b'\n' {
            let line = &entries[line_start..i];
            if line.len() >= 3 {
                let entry_type = line[0];
                // Find spaces
                let mut name_start = 2;
                let mut name_end = name_start;
                while name_end < line.len() && line[name_end] != b' ' {
                    name_end += 1;
                }
                let name = &line[name_start..name_end];
                let size_start = if name_end + 1 < line.len() { name_end + 1 } else { name_end };
                let size = &line[size_start..];

                // Display: <DIR> or size, then name
                if entry_type == b'D' {
                    write_str("<DIR>     ");
                } else {
                    // Right-align size in 10 chars
                    let padding = if size.len() < 10 { 10 - size.len() } else { 0 };
                    for _ in 0..padding {
                        write_str(" ");
                    }
                    write_bytes(size);
                }
                write_str(" ");
                write_bytes(name);
                write_str("\r\n");
            }
            line_start = i + 1;
        }
    }

    exit(0);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    exit(1);
}
