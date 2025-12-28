//! WATOS cat command - concatenate and display files
//!
//! Usage: cat [OPTIONS] [FILE...]
//!
//! Options:
//!   -n    Number all output lines
//!   -b    Number non-blank lines only
//!   -E    Display $ at end of each line
//!   -T    Display TAB characters as ^I
//!   -s    Squeeze multiple blank lines into one
//!
//! With no FILE, or when FILE is -, read standard input.

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

fn write_char(c: u8) {
    unsafe {
        syscall3(syscall::SYS_WRITE, 1, &c as *const u8 as u64, 1);
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

struct Options {
    number_lines: bool,      // -n
    number_nonblank: bool,   // -b
    show_ends: bool,         // -E
    show_tabs: bool,         // -T
    squeeze_blank: bool,     // -s
}

impl Options {
    fn new() -> Self {
        Options {
            number_lines: false,
            number_nonblank: false,
            show_ends: false,
            show_tabs: false,
            squeeze_blank: false,
        }
    }
}

fn format_line_number(mut n: u32, buf: &mut [u8]) -> usize {
    if n == 0 {
        buf[0] = b' ';
        buf[1] = b' ';
        buf[2] = b' ';
        buf[3] = b' ';
        buf[4] = b' ';
        buf[5] = b'0';
        return 6;
    }

    let mut tmp = [0u8; 10];
    let mut i = 0;
    while n > 0 {
        tmp[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }

    // Right-align to 6 characters
    let mut out_idx = 0;
    for _ in 0..(6 - i) {
        buf[out_idx] = b' ';
        out_idx += 1;
    }
    for j in 0..i {
        buf[out_idx] = tmp[i - 1 - j];
        out_idx += 1;
    }
    out_idx
}

fn cat_file(path: &[u8], opts: &Options, line_num: &mut u32, prev_blank: &mut bool) -> i32 {
    let fd = open(path, 0); // O_RDONLY
    if fd < 0 {
        write_str("cat: ");
        write_bytes(path);
        write_str(": No such file or directory\r\n");
        return 1;
    }

    static mut READ_BUF: [u8; 4096] = [0u8; 4096];
    let mut at_line_start = true;
    let mut line_buf = [0u8; 16];

    loop {
        let n = unsafe { read(fd as u64, &mut READ_BUF) };
        if n <= 0 {
            break;
        }

        let data = unsafe { &READ_BUF[..n as usize] };

        for &c in data {
            // Check for blank line (just newline at start of line)
            let is_newline = c == b'\n';
            let is_blank_line = at_line_start && is_newline;

            // Squeeze blank lines
            if opts.squeeze_blank && is_blank_line && *prev_blank {
                continue;
            }

            // Line numbering at start of line
            if at_line_start {
                let should_number = opts.number_lines ||
                    (opts.number_nonblank && !is_blank_line);

                if should_number {
                    let len = format_line_number(*line_num, &mut line_buf);
                    write_bytes(&line_buf[..len]);
                    write_str("  ");
                    *line_num += 1;
                }
            }

            // Output the character with transformations
            if c == b'\t' && opts.show_tabs {
                write_str("^I");
            } else if is_newline {
                if opts.show_ends {
                    write_str("$");
                }
                write_str("\r\n");
                *prev_blank = is_blank_line;
            } else if c != b'\r' {  // Skip CR, we handle line endings ourselves
                write_char(c);
            }

            at_line_start = is_newline;
        }
    }

    close(fd as u64);
    0
}

#[no_mangle]
extern "C" fn _start() -> ! {
    static mut ARGS_BUF: [u8; 1024] = [0u8; 1024];

    let args_len = unsafe { get_args(&mut ARGS_BUF) };
    let args = unsafe { &ARGS_BUF[..args_len] };

    let mut opts = Options::new();
    let mut i = 0;
    let mut exit_code = 0;
    let mut line_num: u32 = 1;
    let mut prev_blank = false;
    let mut file_count = 0;

    // Skip command name
    while i < args.len() && args[i] != b' ' {
        i += 1;
    }
    while i < args.len() && args[i] == b' ' {
        i += 1;
    }

    // Parse options
    while i < args.len() && args[i] == b'-' {
        let opt_start = i;
        i += 1;

        // Check for "--" (end of options) or "-" (stdin)
        if i >= args.len() || args[i] == b' ' {
            // Just "-", treat as stdin placeholder (we don't support stdin yet)
            i = opt_start;
            break;
        }
        if args[i] == b'-' {
            // "--", end of options
            i += 1;
            while i < args.len() && args[i] == b' ' {
                i += 1;
            }
            break;
        }

        while i < args.len() && args[i] != b' ' {
            match args[i] {
                b'n' => opts.number_lines = true,
                b'b' => { opts.number_nonblank = true; opts.number_lines = false; }
                b'E' => opts.show_ends = true,
                b'T' => opts.show_tabs = true,
                b's' => opts.squeeze_blank = true,
                b'A' => {  // -A = -vET (show all)
                    opts.show_ends = true;
                    opts.show_tabs = true;
                }
                _ => {}
            }
            i += 1;
        }
        while i < args.len() && args[i] == b' ' {
            i += 1;
        }
    }

    // Process files
    while i < args.len() {
        let path_start = i;
        while i < args.len() && args[i] != b' ' {
            i += 1;
        }
        let path = &args[path_start..i];

        if !path.is_empty() {
            if path == b"-" {
                write_str("cat: reading from stdin not supported yet\r\n");
            } else {
                let result = cat_file(path, &opts, &mut line_num, &mut prev_blank);
                if result != 0 {
                    exit_code = 1;
                }
            }
            file_count += 1;
        }

        while i < args.len() && args[i] == b' ' {
            i += 1;
        }
    }

    if file_count == 0 {
        write_str("cat: missing operand\r\n");
        write_str("Usage: cat [OPTIONS] FILE...\r\n");
        exit(1);
    }

    exit(exit_code);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    exit(1);
}
