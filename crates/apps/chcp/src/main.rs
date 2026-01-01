//! chcp - Change code page
//!
//! Usage: chcp [<codepage>]
//! Example: chcp        (show current code page)
//!          chcp 437    (change to CP437)
//!          chcp 850    (change to CP850)

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

fn get_codepage() -> u64 {
    unsafe { syscall0(syscall::SYS_GET_CODEPAGE) }
}

fn set_codepage(data: &[u8]) -> u64 {
    unsafe {
        syscall2(
            syscall::SYS_SET_CODEPAGE,
            data.as_ptr() as u64,
            data.len() as u64,
        )
    }
}

/// Parse arguments from space-separated string
fn parse_args(args: &[u8]) -> impl Iterator<Item = &[u8]> {
    args.split(|&b| b == b' ').filter(|s| !s.is_empty())
}

/// Parse a byte slice as a u16 number
fn parse_u16(s: &[u8]) -> Option<u16> {
    if s.is_empty() || s.len() > 5 {
        return None;
    }
    let mut result: u32 = 0;
    for &c in s {
        if c < b'0' || c > b'9' {
            return None;
        }
        result = result * 10 + (c - b'0') as u32;
        if result > 65535 {
            return None;
        }
    }
    Some(result as u16)
}

/// Write a u16 number to output
fn write_u16(n: u16) {
    if n == 0 {
        write_str("0");
        return;
    }
    let mut buf = [0u8; 5];
    let mut i = 5;
    let mut n = n;
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    write_bytes(&buf[i..]);
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

    // No arguments - show current code page
    let cp_arg = match arg_iter.next() {
        Some(arg) => arg,
        None => {
            let current_cp = get_codepage();
            write_str("Active code page: ");
            write_u16(current_cp as u16);
            write_str("\r\n");
            exit(0);
        }
    };

    // Parse code page number
    let cp_num = match parse_u16(cp_arg) {
        Some(n) => n,
        None => {
            write_str("Error: Invalid code page number '");
            write_bytes(cp_arg);
            write_str("'\r\n");
            exit(1);
        }
    };

    // Validate code page number
    if cp_num != 437 && cp_num != 850 && cp_num != 1252 {
        write_str("Error: Unsupported code page ");
        write_u16(cp_num);
        write_str("\r\n");
        write_str("Available code pages: 437, 850, 1252\r\n");
        exit(1);
    }

    // Build path to code page file: /system/codepages/cp<num>.cpg
    static mut PATH_BUF: [u8; 64] = [0u8; 64];
    let path = unsafe {
        let prefix = b"/system/codepages/cp";
        let suffix = b".cpg";
        let mut i = 0;
        for &b in prefix {
            PATH_BUF[i] = b;
            i += 1;
        }
        // Write cp number
        let mut num_buf = [0u8; 5];
        let mut j = 5;
        let mut n = cp_num;
        if n == 0 {
            j -= 1;
            num_buf[j] = b'0';
        } else {
            while n > 0 {
                j -= 1;
                num_buf[j] = b'0' + (n % 10) as u8;
                n /= 10;
            }
        }
        for k in j..5 {
            PATH_BUF[i] = num_buf[k];
            i += 1;
        }
        for &b in suffix {
            PATH_BUF[i] = b;
            i += 1;
        }
        &PATH_BUF[..i]
    };

    // Open and read the code page file
    let fd = open(path, 0); // O_RDONLY = 0
    if fd < 0 {
        write_str("Error: Could not open code page file '");
        write_bytes(path);
        write_str("'\r\n");
        exit(1);
    }

    let bytes_read = unsafe { read(fd as u64, &mut READ_BUF) };
    close(fd as u64);

    if bytes_read <= 0 {
        write_str("Error: Failed to read code page file\r\n");
        exit(1);
    }

    let data = unsafe { &READ_BUF[..bytes_read as usize] };

    // Verify magic header "CPAG"
    if bytes_read < 4 || &data[0..4] != b"CPAG" {
        write_str("Error: Invalid code page file format\r\n");
        exit(1);
    }

    // Send code page to kernel via syscall
    let result = set_codepage(data);

    if result != 0 {
        write_str("Error: Failed to load code page\r\n");
        exit(1);
    }

    write_str("Code page set to: ");
    write_u16(cp_num);
    write_str("\r\n");
    exit(0);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    write_str("PANIC in chcp\r\n");
    exit(1);
}
