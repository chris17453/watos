//! WATOS df command - report filesystem disk space usage
//!
//! Usage: df [OPTIONS] [FILE...]
//!
//! Options:
//!   -h    Human-readable sizes (powers of 1024)
//!   -H    Human-readable sizes (powers of 1000)
//!   -T    Show filesystem type
//!   -a    Include pseudo filesystems

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

fn list_drives(buf: &mut [u8]) -> usize {
    unsafe { syscall2(syscall::SYS_LISTDRIVES, buf.as_mut_ptr() as u64, buf.len() as u64) as usize }
}

fn statfs(path: &[u8], buf: &mut [u64; 6]) -> u64 {
    unsafe {
        syscall3(
            syscall::SYS_STATFS,
            path.as_ptr() as u64,
            path.len() as u64,
            buf.as_mut_ptr() as u64,
        )
    }
}

struct Options {
    human_readable: bool,
    si_units: bool,      // Use powers of 1000 instead of 1024
    show_type: bool,
    show_all: bool,
}

impl Options {
    fn new() -> Self {
        Options {
            human_readable: false,
            si_units: false,
            show_type: false,
            show_all: false,
        }
    }
}

fn format_u64(mut n: u64, buf: &mut [u8]) -> usize {
    if n == 0 {
        buf[0] = b'0';
        return 1;
    }

    let mut tmp = [0u8; 20];
    let mut i = 0;
    while n > 0 {
        tmp[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }

    for j in 0..i {
        buf[j] = tmp[i - 1 - j];
    }
    i
}

fn format_size_human(size: u64, si: bool, buf: &mut [u8]) -> usize {
    let units: &[u8] = if si { b"BKMGTPE" } else { b"BKMGTPE" };
    let divisor: u64 = if si { 1000 } else { 1024 };
    let mut s = size;
    let mut unit_idx = 0;

    while s >= divisor && unit_idx < units.len() - 1 {
        s /= divisor;
        unit_idx += 1;
    }

    let len = format_u64(s, buf);
    if unit_idx > 0 && len < buf.len() - 1 {
        buf[len] = units[unit_idx];
        if si {
            len + 1
        } else {
            buf[len + 1] = b'i';
            len + 2
        }
    } else {
        len
    }
}

fn write_padded_right(s: &[u8], width: usize) {
    let padding = if s.len() < width { width - s.len() } else { 0 };
    for _ in 0..padding {
        write_str(" ");
    }
    write_bytes(s);
}

fn write_padded_left(s: &[u8], width: usize) {
    write_bytes(s);
    let padding = if s.len() < width { width - s.len() } else { 0 };
    for _ in 0..padding {
        write_str(" ");
    }
}

fn print_header(opts: &Options) {
    write_padded_left(b"Filesystem", 14);
    if opts.show_type {
        write_padded_left(b"Type", 8);
    }
    write_padded_right(b"Size", 10);
    write_padded_right(b"Used", 10);
    write_padded_right(b"Avail", 10);
    write_padded_right(b"Use%", 6);
    write_str(" Mounted on\r\n");
}

fn print_drive_info(name: &[u8], path: &[u8], fstype: &[u8], opts: &Options) {
    // Get filesystem stats
    let mut stat_buf: [u64; 6] = [0; 6];
    let result = statfs(path, &mut stat_buf);

    // Even if statfs fails, show the drive
    let total_blocks = stat_buf[0];
    let free_blocks = stat_buf[1];
    let block_size = stat_buf[2];

    let total_bytes = total_blocks * block_size;
    let free_bytes = free_blocks * block_size;
    let used_bytes = total_bytes.saturating_sub(free_bytes);

    let use_percent = if total_bytes > 0 {
        ((used_bytes * 100) / total_bytes) as u32
    } else {
        0
    };

    // Print filesystem name
    write_padded_left(name, 14);

    // Print type if requested
    if opts.show_type {
        write_padded_left(fstype, 8);
    }

    // Format sizes
    let mut buf = [0u8; 16];

    // Size
    let len = if opts.human_readable || opts.si_units {
        format_size_human(total_bytes, opts.si_units, &mut buf)
    } else {
        format_u64(total_bytes / 1024, &mut buf)  // Default: 1K blocks
    };
    write_padded_right(&buf[..len], 10);

    // Used
    let len = if opts.human_readable || opts.si_units {
        format_size_human(used_bytes, opts.si_units, &mut buf)
    } else {
        format_u64(used_bytes / 1024, &mut buf)
    };
    write_padded_right(&buf[..len], 10);

    // Available
    let len = if opts.human_readable || opts.si_units {
        format_size_human(free_bytes, opts.si_units, &mut buf)
    } else {
        format_u64(free_bytes / 1024, &mut buf)
    };
    write_padded_right(&buf[..len], 10);

    // Use%
    let len = format_u64(use_percent as u64, &mut buf);
    buf[len] = b'%';
    write_padded_right(&buf[..len + 1], 6);

    // Mount point
    write_str(" ");
    write_bytes(path);
    write_str("\r\n");
}

#[no_mangle]
extern "C" fn _start() -> ! {
    static mut ARGS_BUF: [u8; 256] = [0u8; 256];
    static mut DRIVES_BUF: [u8; 1024] = [0u8; 1024];

    let args_len = unsafe { get_args(&mut ARGS_BUF) };
    let args = unsafe { &ARGS_BUF[..args_len] };

    let mut opts = Options::new();
    let mut i = 0;

    // Skip command name
    while i < args.len() && args[i] != b' ' {
        i += 1;
    }
    while i < args.len() && args[i] == b' ' {
        i += 1;
    }

    // Parse options
    while i < args.len() && args[i] == b'-' {
        i += 1;
        while i < args.len() && args[i] != b' ' {
            match args[i] {
                b'h' => opts.human_readable = true,
                b'H' => { opts.human_readable = true; opts.si_units = true; }
                b'T' => opts.show_type = true,
                b'a' => opts.show_all = true,
                _ => {}
            }
            i += 1;
        }
        while i < args.len() && args[i] == b' ' {
            i += 1;
        }
    }

    // Print header
    print_header(&opts);

    // Get list of drives
    let drives_len = unsafe { list_drives(&mut DRIVES_BUF) };
    let drives = unsafe { &DRIVES_BUF[..drives_len] };

    // Parse drives (format: "NAME:PATH:FSTYPE\n" per line)
    let mut line_start = 0;
    for j in 0..drives_len {
        if drives[j] == b'\n' {
            let line = &drives[line_start..j];

            // Parse "NAME:PATH:FSTYPE"
            let mut colon1 = 0;
            let mut colon2 = 0;
            for (k, &c) in line.iter().enumerate() {
                if c == b':' {
                    if colon1 == 0 {
                        colon1 = k;
                    } else {
                        colon2 = k;
                        break;
                    }
                }
            }

            if colon1 > 0 && colon2 > colon1 {
                let name = &line[..colon1];
                let path = &line[colon1 + 1..colon2];
                let fstype = &line[colon2 + 1..];

                // Skip pseudo filesystems unless -a
                let is_pseudo = fstype == b"devfs" || fstype == b"procfs";
                if opts.show_all || !is_pseudo {
                    print_drive_info(name, path, fstype, &opts);
                }
            }

            line_start = j + 1;
        }
    }

    // If no drives found, show a message
    if drives_len == 0 {
        write_str("No filesystems mounted\r\n");
    }

    exit(0);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    exit(1);
}
