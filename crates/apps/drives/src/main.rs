//! WATOS drives command
//!
//! List and manage mounted drives.
//! Usage:
//!   drives        - List all mounted drives
//!   drives mount NAME PATH  - Mount a drive
//!   drives unmount NAME     - Unmount a drive

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

fn list_drives(buf: &mut [u8]) -> usize {
    unsafe { syscall2(syscall::SYS_LISTDRIVES, buf.as_mut_ptr() as u64, buf.len() as u64) as usize }
}

fn mount_drive(name: &[u8], path: &[u8]) -> u64 {
    unsafe {
        syscall3(
            syscall::SYS_MOUNT,
            name.as_ptr() as u64,
            name.len() as u64,
            path.as_ptr() as u64,
        )
    }
}

fn unmount_drive(name: &[u8]) -> u64 {
    unsafe { syscall2(syscall::SYS_UNMOUNT, name.as_ptr() as u64, name.len() as u64) }
}

/// Parse arguments and find words
fn parse_args(args: &[u8]) -> (&[u8], &[u8], &[u8], &[u8]) {
    let mut words: [&[u8]; 4] = [&[], &[], &[], &[]];
    let mut word_idx = 0;
    let mut start = 0;
    let mut in_word = false;

    for (i, &c) in args.iter().enumerate() {
        if c == b' ' || c == b'\t' {
            if in_word {
                if word_idx < 4 {
                    words[word_idx] = &args[start..i];
                    word_idx += 1;
                }
                in_word = false;
            }
        } else {
            if !in_word {
                start = i;
                in_word = true;
            }
        }
    }
    if in_word && word_idx < 4 {
        words[word_idx] = &args[start..];
    }

    (words[0], words[1], words[2], words[3])
}

fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    for i in 0..a.len() {
        let a_lower = if a[i] >= b'A' && a[i] <= b'Z' { a[i] + 32 } else { a[i] };
        let b_lower = if b[i] >= b'A' && b[i] <= b'Z' { b[i] + 32 } else { b[i] };
        if a_lower != b_lower {
            return false;
        }
    }
    true
}

#[no_mangle]
extern "C" fn _start() -> ! {
    static mut ARGS_BUF: [u8; 256] = [0u8; 256];
    static mut LIST_BUF: [u8; 512] = [0u8; 512];

    let args_len = unsafe { get_args(&mut ARGS_BUF) };
    let args = unsafe { &ARGS_BUF[..args_len] };

    let (cmd, subcmd, arg1, arg2) = parse_args(args);

    // If just "drives" with no subcommand, list drives
    if subcmd.is_empty() {
        let len = unsafe { list_drives(&mut LIST_BUF) };
        if len == 0 {
            write_str("No drives mounted.\r\n");
        } else {
            write_str("Drive  Path           Type\r\n");
            write_str("-----  ----           ----\r\n");
            // Parse and format the list output
            let list = unsafe { &LIST_BUF[..len] };
            let mut line_start = 0;
            for i in 0..len {
                if list[i] == b'\n' {
                    let line = &list[line_start..i];
                    // Format: NAME:PATH:FSTYPE[*]
                    let mut parts: [&[u8]; 3] = [&[], &[], &[]];
                    let mut part_idx = 0;
                    let mut pstart = 0;
                    let mut is_current = false;
                    for (j, &c) in line.iter().enumerate() {
                        if c == b':' {
                            if part_idx < 3 {
                                parts[part_idx] = &line[pstart..j];
                                part_idx += 1;
                            }
                            pstart = j + 1;
                        } else if c == b'*' {
                            is_current = true;
                        }
                    }
                    if part_idx < 3 {
                        // Last part (fstype, may have *)
                        let end = if is_current && pstart < line.len() {
                            line.len() - 1
                        } else {
                            line.len()
                        };
                        parts[part_idx] = &line[pstart..end];
                    }

                    // Print formatted
                    if is_current {
                        write_str("*");
                    } else {
                        write_str(" ");
                    }
                    write_bytes(parts[0]); // Name
                    // Pad to column
                    for _ in parts[0].len()..5 {
                        write_str(" ");
                    }
                    write_str("  ");
                    write_bytes(parts[1]); // Path
                    for _ in parts[1].len()..13 {
                        write_str(" ");
                    }
                    write_str("  ");
                    write_bytes(parts[2]); // FS type
                    write_str("\r\n");

                    line_start = i + 1;
                }
            }
        }
    } else if bytes_eq(subcmd, b"mount") {
        // drives mount NAME PATH
        if arg1.is_empty() || arg2.is_empty() {
            write_str("Usage: drives mount NAME PATH\r\n");
            write_str("Example: drives mount D /mnt/data\r\n");
            exit(1);
        }

        // Create null-terminated path
        static mut PATH_BUF: [u8; 65] = [0u8; 65];
        let path_len = arg2.len().min(64);
        unsafe {
            PATH_BUF[..path_len].copy_from_slice(&arg2[..path_len]);
            PATH_BUF[path_len] = 0;
        }

        let result = unsafe { mount_drive(arg1, &PATH_BUF[..path_len + 1]) };
        match result {
            0 => {
                write_str("Mounted ");
                write_bytes(arg1);
                write_str(": -> ");
                write_bytes(arg2);
                write_str("\r\n");
            }
            1 => write_str("Error: Invalid drive name\r\n"),
            2 => write_str("Error: Invalid path\r\n"),
            3 => write_str("Error: Drive already mounted\r\n"),
            4 => write_str("Error: Too many drives mounted\r\n"),
            _ => write_str("Error: Mount failed\r\n"),
        }
    } else if bytes_eq(subcmd, b"unmount") || bytes_eq(subcmd, b"umount") {
        // drives unmount NAME
        if arg1.is_empty() {
            write_str("Usage: drives unmount NAME\r\n");
            exit(1);
        }

        let result = unmount_drive(arg1);
        match result {
            0 => {
                write_str("Unmounted ");
                write_bytes(arg1);
                write_str(":\r\n");
            }
            1 => write_str("Error: Invalid drive name\r\n"),
            2 => write_str("Error: Cannot unmount current drive\r\n"),
            3 => write_str("Error: Drive not found\r\n"),
            _ => write_str("Error: Unmount failed\r\n"),
        }
    } else {
        write_str("Usage: drives [mount NAME PATH | unmount NAME]\r\n");
        write_str("       drives         - List mounted drives\r\n");
        write_str("       drives mount   - Mount a drive\r\n");
        write_str("       drives unmount - Unmount a drive\r\n");
    }

    exit(0);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    exit(1);
}
