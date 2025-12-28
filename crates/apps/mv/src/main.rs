//! WATOS mv command - move (rename) files and directories
//!
//! Usage: mv [OPTIONS] SOURCE DEST
//!        mv [OPTIONS] SOURCE... DIRECTORY
//!
//! Options:
//!   -v        Verbose mode
//!   -f        Force overwrite without prompting

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

#[inline(always)]
unsafe fn syscall4(num: u32, arg1: u64, arg2: u64, arg3: u64, arg4: u64) -> u64 {
    let ret: u64;
    core::arch::asm!(
        "int 0x80",
        in("eax") num,
        in("rdi") arg1,
        in("rsi") arg2,
        in("rdx") arg3,
        in("r10") arg4,
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

fn rename(old_path: &[u8], new_path: &[u8]) -> i64 {
    unsafe {
        syscall4(
            syscall::SYS_RENAME,
            old_path.as_ptr() as u64,
            old_path.len() as u64,
            new_path.as_ptr() as u64,
            new_path.len() as u64,
        ) as i64
    }
}

// File type constants
const S_IFDIR: u64 = 0o040000;
const S_IFMT: u64 = 0o170000;

struct Options {
    verbose: bool,
}

impl Options {
    fn new() -> Self {
        Options {
            verbose: false,
        }
    }
}

fn is_directory(path: &[u8]) -> bool {
    let mut stat_buf: [u64; 8] = [0; 8];
    if stat(path, &mut stat_buf) == 0 {
        (stat_buf[0] & S_IFMT) == S_IFDIR
    } else {
        false
    }
}

fn get_basename(path: &[u8]) -> &[u8] {
    let mut last_slash = 0;
    for (i, &c) in path.iter().enumerate() {
        if c == b'/' {
            last_slash = i + 1;
        }
    }
    &path[last_slash..]
}

fn build_dest_path(dest: &[u8], src_basename: &[u8], buf: &mut [u8]) -> usize {
    let mut idx = 0;
    for &c in dest {
        if idx < buf.len() {
            buf[idx] = c;
            idx += 1;
        }
    }
    if idx > 0 && idx < buf.len() && buf[idx - 1] != b'/' {
        buf[idx] = b'/';
        idx += 1;
    }
    for &c in src_basename {
        if idx < buf.len() {
            buf[idx] = c;
            idx += 1;
        }
    }
    idx
}

fn move_entry(src: &[u8], dest: &[u8], opts: &Options) -> i32 {
    let result = rename(src, dest);
    if result == 0 {
        if opts.verbose {
            write_str("renamed '");
            write_bytes(src);
            write_str("' -> '");
            write_bytes(dest);
            write_str("'\r\n");
        }
        0
    } else {
        write_str("mv: cannot move '");
        write_bytes(src);
        write_str("' to '");
        write_bytes(dest);
        write_str("'\r\n");
        1
    }
}

#[no_mangle]
extern "C" fn _start() -> ! {
    static mut ARGS_BUF: [u8; 2048] = [0u8; 2048];
    static mut PATHS: [[u8; 256]; 16] = [[0u8; 256]; 16];
    static mut PATH_LENS: [usize; 16] = [0; 16];
    static mut DEST_BUF: [u8; 512] = [0u8; 512];

    let args_len = unsafe { get_args(&mut ARGS_BUF) };
    let args = unsafe { &ARGS_BUF[..args_len] };

    let mut opts = Options::new();
    let mut i = 0;
    let mut path_count = 0;

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

        if i < args.len() && args[i] == b'-' {
            i += 1;
            while i < args.len() && args[i] == b' ' {
                i += 1;
            }
            break;
        }

        while i < args.len() && args[i] != b' ' {
            match args[i] {
                b'v' => opts.verbose = true,
                b'f' => {} // Force, we always overwrite
                _ => {}
            }
            i += 1;
        }
        while i < args.len() && args[i] == b' ' {
            i += 1;
        }
    }

    // Collect paths
    while i < args.len() && path_count < 16 {
        let path_start = i;
        while i < args.len() && args[i] != b' ' {
            i += 1;
        }
        let path = &args[path_start..i];

        if !path.is_empty() {
            let len = path.len().min(256);
            unsafe {
                PATHS[path_count][..len].copy_from_slice(&path[..len]);
                PATH_LENS[path_count] = len;
            }
            path_count += 1;
        }

        while i < args.len() && args[i] == b' ' {
            i += 1;
        }
    }

    if path_count < 2 {
        write_str("mv: missing file operand\r\n");
        write_str("Usage: mv [OPTIONS] SOURCE DEST\r\n");
        write_str("       mv [OPTIONS] SOURCE... DIRECTORY\r\n");
        exit(1);
    }

    // Last argument is destination
    let dest = unsafe { &PATHS[path_count - 1][..PATH_LENS[path_count - 1]] };
    let dest_is_dir = is_directory(dest);

    // If multiple sources, dest must be a directory
    if path_count > 2 && !dest_is_dir {
        write_str("mv: target '");
        write_bytes(dest);
        write_str("' is not a directory\r\n");
        exit(1);
    }

    let mut exit_code = 0;

    for p in 0..(path_count - 1) {
        let src = unsafe { &PATHS[p][..PATH_LENS[p]] };

        let actual_dest = if dest_is_dir {
            let basename = get_basename(src);
            let len = unsafe { build_dest_path(dest, basename, &mut DEST_BUF) };
            unsafe { &DEST_BUF[..len] }
        } else {
            dest
        };

        let result = move_entry(src, actual_dest, &opts);
        if result != 0 {
            exit_code = 1;
        }
    }

    exit(exit_code);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    exit(1);
}
