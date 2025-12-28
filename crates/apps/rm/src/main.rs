//! WATOS rm command - remove files or directories
//!
//! Usage: rm [OPTIONS] FILE...
//!
//! Options:
//!   -r, -R    Remove directories and their contents recursively
//!   -f        Force removal, ignore nonexistent files
//!   -d        Remove empty directories
//!   -v        Verbose mode, explain what is being done

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

fn unlink(path: &[u8]) -> i64 {
    unsafe {
        syscall2(
            syscall::SYS_UNLINK,
            path.as_ptr() as u64,
            path.len() as u64,
        ) as i64
    }
}

fn rmdir(path: &[u8]) -> i64 {
    unsafe {
        syscall2(
            syscall::SYS_RMDIR,
            path.as_ptr() as u64,
            path.len() as u64,
        ) as i64
    }
}

fn stat(path: &[u8], buf: &mut [u64; 8]) -> i64 {
    unsafe {
        syscall3(
            syscall::SYS_STAT,
            path.as_ptr() as u64,
            path.len() as u64,
            buf.as_mut_ptr() as u64,
        ) as i64
    }
}

fn readdir(path: &[u8], buf: &mut [u8]) -> i64 {
    unsafe {
        syscall3(
            syscall::SYS_READDIR,
            path.as_ptr() as u64,
            path.len() as u64,
            buf.as_mut_ptr() as u64,
        ) as i64
    }
}

struct Options {
    recursive: bool,   // -r, -R
    force: bool,       // -f
    dir: bool,         // -d
    verbose: bool,     // -v
}

impl Options {
    fn new() -> Self {
        Options {
            recursive: false,
            force: false,
            dir: false,
            verbose: false,
        }
    }
}

// File type constants from stat
const S_IFDIR: u64 = 0o040000;
const S_IFMT: u64 = 0o170000;

fn is_directory(path: &[u8]) -> bool {
    let mut stat_buf: [u64; 8] = [0; 8];
    if stat(path, &mut stat_buf) == 0 {
        (stat_buf[0] & S_IFMT) == S_IFDIR
    } else {
        false
    }
}

fn remove_recursive(path: &[u8], opts: &Options, path_buf: &mut [u8]) -> i32 {
    static mut DIR_BUF: [u8; 4096] = [0u8; 4096];

    // First, read directory contents
    let n = unsafe { readdir(path, &mut DIR_BUF) };
    if n < 0 {
        // Not a directory or error - try unlink
        let result = unlink(path);
        if result == 0 {
            if opts.verbose {
                write_str("removed '");
                write_bytes(path);
                write_str("'\r\n");
            }
            return 0;
        } else if !opts.force {
            write_str("rm: cannot remove '");
            write_bytes(path);
            write_str("'\r\n");
            return 1;
        }
        return 0;
    }

    let entries = unsafe { &DIR_BUF[..n as usize] };
    let mut exit_code = 0;

    // Parse entries (newline-separated)
    let mut start = 0;
    for j in 0..entries.len() {
        if entries[j] == b'\n' || j == entries.len() - 1 {
            let end = if entries[j] == b'\n' { j } else { j + 1 };
            let entry = &entries[start..end];

            // Skip . and ..
            if entry != b"." && entry != b".." && !entry.is_empty() {
                // Build full path
                let mut idx = 0;
                for &c in path {
                    if idx < path_buf.len() {
                        path_buf[idx] = c;
                        idx += 1;
                    }
                }
                if idx < path_buf.len() && idx > 0 && path_buf[idx - 1] != b'/' {
                    path_buf[idx] = b'/';
                    idx += 1;
                }
                for &c in entry {
                    if idx < path_buf.len() {
                        path_buf[idx] = c;
                        idx += 1;
                    }
                }

                let child_path = &path_buf[..idx];

                // Recurse or remove
                if is_directory(child_path) {
                    // Need a separate buffer for recursion
                    static mut RECURSE_BUF: [u8; 512] = [0u8; 512];
                    let recurse_buf = unsafe { &mut RECURSE_BUF };
                    exit_code |= remove_recursive(child_path, opts, recurse_buf);
                } else {
                    let result = unlink(child_path);
                    if result != 0 && !opts.force {
                        write_str("rm: cannot remove '");
                        write_bytes(child_path);
                        write_str("'\r\n");
                        exit_code = 1;
                    } else if opts.verbose {
                        write_str("removed '");
                        write_bytes(child_path);
                        write_str("'\r\n");
                    }
                }
            }

            start = j + 1;
        }
    }

    // Now remove the directory itself
    let result = rmdir(path);
    if result == 0 {
        if opts.verbose {
            write_str("removed directory '");
            write_bytes(path);
            write_str("'\r\n");
        }
    } else if !opts.force {
        write_str("rm: cannot remove '");
        write_bytes(path);
        write_str("'\r\n");
        exit_code = 1;
    }

    exit_code
}

fn remove_file(path: &[u8], opts: &Options) -> i32 {
    let is_dir = is_directory(path);

    if is_dir {
        if opts.recursive {
            static mut PATH_BUF: [u8; 512] = [0u8; 512];
            return remove_recursive(path, opts, unsafe { &mut PATH_BUF });
        } else if opts.dir {
            let result = rmdir(path);
            if result == 0 {
                if opts.verbose {
                    write_str("removed directory '");
                    write_bytes(path);
                    write_str("'\r\n");
                }
                return 0;
            } else if !opts.force {
                write_str("rm: cannot remove '");
                write_bytes(path);
                write_str("': Directory not empty or error\r\n");
                return 1;
            }
        } else {
            if !opts.force {
                write_str("rm: cannot remove '");
                write_bytes(path);
                write_str("': Is a directory\r\n");
                return 1;
            }
        }
        return 0;
    }

    let result = unlink(path);
    if result == 0 {
        if opts.verbose {
            write_str("removed '");
            write_bytes(path);
            write_str("'\r\n");
        }
        0
    } else if opts.force {
        0
    } else {
        write_str("rm: cannot remove '");
        write_bytes(path);
        write_str("': No such file or directory\r\n");
        1
    }
}

#[no_mangle]
extern "C" fn _start() -> ! {
    static mut ARGS_BUF: [u8; 1024] = [0u8; 1024];

    let args_len = unsafe { get_args(&mut ARGS_BUF) };
    let args = unsafe { &ARGS_BUF[..args_len] };

    let mut opts = Options::new();
    let mut i = 0;
    let mut exit_code = 0;
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
        i += 1;

        // Check for "--" (end of options)
        if i < args.len() && args[i] == b'-' {
            i += 1;
            while i < args.len() && args[i] == b' ' {
                i += 1;
            }
            break;
        }

        while i < args.len() && args[i] != b' ' {
            match args[i] {
                b'r' | b'R' => opts.recursive = true,
                b'f' => opts.force = true,
                b'd' => opts.dir = true,
                b'v' => opts.verbose = true,
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
            let result = remove_file(path, &opts);
            if result != 0 {
                exit_code = 1;
            }
            file_count += 1;
        }

        while i < args.len() && args[i] == b' ' {
            i += 1;
        }
    }

    if file_count == 0 && !opts.force {
        write_str("rm: missing operand\r\n");
        write_str("Usage: rm [OPTIONS] FILE...\r\n");
        exit(1);
    }

    exit(exit_code);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    exit(1);
}
