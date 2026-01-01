//! loadkeys - Load keyboard layout from file
//!
//! Usage: loadkeys <layout>
//! Example: loadkeys us
//!          loadkeys de

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

fn open(path: &[u8], flags: u32) -> i64 {
    unsafe {
        syscall3(
            syscall::SYS_OPEN,
            path.as_ptr() as u64,
            path.len() as u64,
            flags as u64,
        ) as i64
    }
}

fn read(fd: u64, buf: &mut [u8]) -> i64 {
    unsafe {
        syscall3(
            syscall::SYS_READ,
            fd,
            buf.as_mut_ptr() as u64,
            buf.len() as u64,
        ) as i64
    }
}

fn close(fd: u64) {
    unsafe {
        syscall2(syscall::SYS_CLOSE, fd, 0);
    }
}

fn set_keymap(data: &[u8]) -> u64 {
    unsafe {
        syscall2(
            syscall::SYS_SET_KEYMAP,
            data.as_ptr() as u64,
            data.len() as u64,
        )
    }
}

/// Parse arguments from space-separated string
/// Returns iterator of byte slices
fn parse_args(args: &[u8]) -> impl Iterator<Item = &[u8]> {
    args.split(|&b| b == b' ').filter(|s| !s.is_empty())
}

#[no_mangle]
extern "C" fn _start() -> ! {
    static mut ARGS_BUF: [u8; 256] = [0u8; 256];
    static mut READ_BUF: [u8; 2048] = [0u8; 2048];

    let args_len = unsafe { get_args(&mut ARGS_BUF) };
    let args = unsafe { &ARGS_BUF[..args_len] };

    // Collect arguments
    let mut arg_iter = parse_args(args);
    let _cmd = arg_iter.next(); // Skip command name

    let layout = match arg_iter.next() {
        Some(l) => l,
        None => {
            write_str("Usage: loadkeys <layout>\r\n");
            write_str("Available layouts: us, uk, de, fr\r\n");
            exit(1);
        }
    };

    // Validate layout name
    if layout != b"us" && layout != b"uk" && layout != b"de" && layout != b"fr" {
        write_str("Error: Unknown layout '");
        write_bytes(layout);
        write_str("'\r\n");
        write_str("Available layouts: us, uk, de, fr\r\n");
        exit(1);
    }

    // Build path to keymap file: /system/keymaps/<layout>.kmap
    static mut PATH_BUF: [u8; 64] = [0u8; 64];
    let path = unsafe {
        let prefix = b"/system/keymaps/";
        let suffix = b".kmap";
        let mut i = 0;
        for &b in prefix {
            PATH_BUF[i] = b;
            i += 1;
        }
        for &b in layout {
            PATH_BUF[i] = b;
            i += 1;
        }
        for &b in suffix {
            PATH_BUF[i] = b;
            i += 1;
        }
        &PATH_BUF[..i]
    };

    // Open and read the keymap file
    let fd = open(path, 0); // O_RDONLY = 0
    if fd < 0 {
        write_str("Error: Could not open keymap file '");
        write_bytes(path);
        write_str("'\r\n");
        exit(1);
    }

    let bytes_read = unsafe { read(fd as u64, &mut READ_BUF) };
    close(fd as u64);

    if bytes_read <= 0 {
        write_str("Error: Failed to read keymap file\r\n");
        exit(1);
    }

    let data = unsafe { &READ_BUF[..bytes_read as usize] };

    // Verify magic header "KMAP"
    if bytes_read < 4 || &data[0..4] != b"KMAP" {
        write_str("Error: Invalid keymap file format\r\n");
        exit(1);
    }

    // Send keymap to kernel via syscall
    let result = set_keymap(data);

    if result != 0 {
        write_str("Error: Failed to load keymap\r\n");
        exit(1);
    }

    write_str("Keyboard layout set to: ");
    write_bytes(layout);
    write_str("\r\n");
    exit(0);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    write_str("PANIC in loadkeys\r\n");
    exit(1);
}
