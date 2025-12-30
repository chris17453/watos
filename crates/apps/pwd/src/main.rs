//! WATOS pwd command - print working directory
//!
//! Usage: pwd [OPTIONS]
//!
//! Options:
//!   -L    Print logical path (with symlinks, default)
//!   -P    Print physical path (resolve symlinks)
//!   -W    Print Windows-style path (C:\path\to\dir)

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

fn getcwd(buf: &mut [u8]) -> usize {
    unsafe { syscall2(syscall::SYS_GETCWD, buf.as_mut_ptr() as u64, buf.len() as u64) as usize }
}

fn get_args(buf: &mut [u8]) -> usize {
    unsafe { syscall2(syscall::SYS_GETARGS, buf.as_mut_ptr() as u64, buf.len() as u64) as usize }
}

struct Options {
    physical: bool,   // -P: resolve symlinks
    windows: bool,    // -W: Windows-style output
}

impl Options {
    fn new() -> Self {
        Options {
            physical: false,
            windows: false,
        }
    }
}

/// Convert Unix path to Windows-style path
/// /mount/c/users/bob -> C:\users\bob
fn to_windows_path(unix_path: &[u8], out: &mut [u8]) -> usize {
    // Check for /mount/X/ pattern
    if unix_path.len() >= 8 && &unix_path[..7] == b"/mount/" {
        let drive = unix_path[7];
        if drive.is_ascii_alphabetic() {
            // Convert to uppercase drive letter
            let drive_upper = if drive >= b'a' && drive <= b'z' {
                drive - 32
            } else {
                drive
            };

            out[0] = drive_upper;
            out[1] = b':';

            let mut out_idx = 2;
            let mut in_idx = 8; // Skip /mount/X

            // Handle root case (/mount/c with no trailing content)
            if in_idx >= unix_path.len() {
                out[out_idx] = b'\\';
                return out_idx + 1;
            }

            // Convert remaining path, changing / to \
            while in_idx < unix_path.len() && out_idx < out.len() {
                if unix_path[in_idx] == b'/' {
                    out[out_idx] = b'\\';
                } else {
                    out[out_idx] = unix_path[in_idx];
                }
                in_idx += 1;
                out_idx += 1;
            }

            return out_idx;
        }
    }

    // Not a /mount/X path, just copy with slashes converted
    let mut out_idx = 0;
    for &c in unix_path {
        if out_idx >= out.len() {
            break;
        }
        out[out_idx] = if c == b'/' { b'\\' } else { c };
        out_idx += 1;
    }
    out_idx
}

#[no_mangle]
extern "C" fn _start() -> ! {
    use core::ptr::addr_of_mut;
    static mut ARGS_BUF: [u8; 256] = [0u8; 256];
    static mut CWD_BUF: [u8; 256] = [0u8; 256];
    static mut WIN_BUF: [u8; 256] = [0u8; 256];

    let args_len = unsafe {
        let buf = &mut *addr_of_mut!(ARGS_BUF);
        get_args(buf)
    };
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
                b'L' => opts.physical = false,  // Logical (default)
                b'P' => opts.physical = true,   // Physical
                b'W' => opts.windows = true,    // Windows-style
                _ => {}
            }
            i += 1;
        }
        while i < args.len() && args[i] == b' ' {
            i += 1;
        }
    }

    let len = unsafe {
        let buf = &mut *addr_of_mut!(CWD_BUF);
        getcwd(buf)
    };

    if len > 0 {
        let cwd = unsafe { &CWD_BUF[..len] };

        // TODO: -P option would need a realpath syscall to resolve symlinks
        // For now, -P behaves the same as -L since we don't have that syscall yet

        if opts.windows {
            let win_len = unsafe {
                let buf = &mut *addr_of_mut!(WIN_BUF);
                to_windows_path(cwd, buf)
            };
            write_bytes(unsafe { &WIN_BUF[..win_len] });
        } else {
            write_bytes(cwd);
        }
    } else {
        write_str("(unknown)");
    }
    write_str("\r\n");

    exit(0);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    exit(1);
}
