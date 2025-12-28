//! WATOS cd command - change directory

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
        syscall1(syscall::SYS_EXIT, code as u64);
    }
    loop {}
}

fn get_args(buf: &mut [u8]) -> usize {
    unsafe { syscall2(syscall::SYS_GETARGS, buf.as_mut_ptr() as u64, buf.len() as u64) as usize }
}

fn chdir(path: &[u8]) -> u64 {
    unsafe { syscall2(syscall::SYS_CHDIR, path.as_ptr() as u64, path.len() as u64) }
}

fn getcwd(buf: &mut [u8]) -> usize {
    unsafe { syscall2(syscall::SYS_GETCWD, buf.as_mut_ptr() as u64, buf.len() as u64) as usize }
}

#[no_mangle]
extern "C" fn _start() -> ! {
    static mut ARGS_BUF: [u8; 256] = [0u8; 256];
    static mut CWD_BUF: [u8; 256] = [0u8; 256];

    let args_len = unsafe { get_args(&mut ARGS_BUF) };
    let args = unsafe { &ARGS_BUF[..args_len] };

    // Parse path from args (skip "cd" command itself)
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
        // Rest is path
        if i < args_len {
            &args[i..]
        } else {
            &[]
        }
    };

    if path.is_empty() {
        // No argument - print current directory
        let len = unsafe { getcwd(&mut CWD_BUF) };
        if len > 0 {
            write_bytes(unsafe { &CWD_BUF[..len] });
            write_str("\r\n");
        }
        exit(0);
    }

    // Change directory
    let result = chdir(path);

    if result != 0 {
        write_str("cd: ");
        write_bytes(path);
        match result {
            1 => write_str(": Invalid path\r\n"),
            2 => write_str(": Path too long\r\n"),
            _ => write_str(": No such directory\r\n"),
        }
        exit(1);
    }

    exit(0);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    exit(1);
}
