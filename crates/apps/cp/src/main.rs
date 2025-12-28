//! WATOS cp command - copy files and directories
//!
//! Usage: cp [OPTIONS] SOURCE DEST
//!        cp [OPTIONS] SOURCE... DIRECTORY
//!
//! Options:
//!   -r, -R    Copy directories recursively
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

fn write_fd(fd: u64, buf: &[u8]) -> i64 {
    unsafe {
        syscall3(
            syscall::SYS_WRITE,
            fd,
            buf.as_ptr() as u64,
            buf.len() as u64,
        ) as i64
    }
}

fn close(fd: u64) {
    unsafe {
        syscall2(syscall::SYS_CLOSE, fd, 0);
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

fn mkdir(path: &[u8]) -> i64 {
    unsafe {
        syscall2(
            syscall::SYS_MKDIR,
            path.as_ptr() as u64,
            path.len() as u64,
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

// Open flags
const O_RDONLY: u32 = 0x00;
const O_WRONLY: u32 = 0x01;
const O_CREAT: u32 = 0x40;
const O_TRUNC: u32 = 0x200;

// File type constants
const S_IFDIR: u64 = 0o040000;
const S_IFMT: u64 = 0o170000;

struct Options {
    recursive: bool,
    verbose: bool,
}

impl Options {
    fn new() -> Self {
        Options {
            recursive: false,
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

fn file_exists(path: &[u8]) -> bool {
    let mut stat_buf: [u64; 8] = [0; 8];
    stat(path, &mut stat_buf) == 0
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

fn copy_file(src: &[u8], dest: &[u8], opts: &Options) -> i32 {
    static mut COPY_BUF: [u8; 4096] = [0u8; 4096];

    let src_fd = open(src, O_RDONLY);
    if src_fd < 0 {
        write_str("cp: cannot open '");
        write_bytes(src);
        write_str("' for reading\r\n");
        return 1;
    }

    let dest_fd = open(dest, O_WRONLY | O_CREAT | O_TRUNC);
    if dest_fd < 0 {
        close(src_fd as u64);
        write_str("cp: cannot create '");
        write_bytes(dest);
        write_str("'\r\n");
        return 1;
    }

    loop {
        let n = unsafe { read(src_fd as u64, &mut COPY_BUF) };
        if n <= 0 {
            break;
        }
        let written = write_fd(dest_fd as u64, unsafe { &COPY_BUF[..n as usize] });
        if written != n {
            write_str("cp: error writing to '");
            write_bytes(dest);
            write_str("'\r\n");
            close(src_fd as u64);
            close(dest_fd as u64);
            return 1;
        }
    }

    close(src_fd as u64);
    close(dest_fd as u64);

    if opts.verbose {
        write_str("'");
        write_bytes(src);
        write_str("' -> '");
        write_bytes(dest);
        write_str("'\r\n");
    }

    0
}

fn copy_recursive(src: &[u8], dest: &[u8], opts: &Options) -> i32 {
    static mut DIR_BUF: [u8; 4096] = [0u8; 4096];
    static mut SRC_PATH_BUF: [u8; 512] = [0u8; 512];
    static mut DEST_PATH_BUF: [u8; 512] = [0u8; 512];

    // Create destination directory
    if !file_exists(dest) {
        if mkdir(dest) != 0 {
            write_str("cp: cannot create directory '");
            write_bytes(dest);
            write_str("'\r\n");
            return 1;
        }
        if opts.verbose {
            write_str("created directory '");
            write_bytes(dest);
            write_str("'\r\n");
        }
    }

    // Read source directory contents
    let n = unsafe { readdir(src, &mut DIR_BUF) };
    if n < 0 {
        write_str("cp: cannot read directory '");
        write_bytes(src);
        write_str("'\r\n");
        return 1;
    }

    let entries = unsafe { &DIR_BUF[..n as usize] };
    let mut exit_code = 0;

    // Parse entries (newline-separated)
    let mut start = 0;
    for j in 0..entries.len() {
        if entries[j] == b'\n' || j == entries.len() - 1 {
            let end = if entries[j] == b'\n' { j } else { j + 1 };
            let entry = &entries[start..end];

            if entry != b"." && entry != b".." && !entry.is_empty() {
                // Build source path
                let src_len = unsafe { build_dest_path(src, entry, &mut SRC_PATH_BUF) };
                let src_child = unsafe { &SRC_PATH_BUF[..src_len] };

                // Build dest path
                let dest_len = unsafe { build_dest_path(dest, entry, &mut DEST_PATH_BUF) };
                let dest_child = unsafe { &DEST_PATH_BUF[..dest_len] };

                if is_directory(src_child) {
                    exit_code |= copy_recursive(src_child, dest_child, opts);
                } else {
                    exit_code |= copy_file(src_child, dest_child, opts);
                }
            }

            start = j + 1;
        }
    }

    exit_code
}

fn copy_entry(src: &[u8], dest: &[u8], opts: &Options) -> i32 {
    if is_directory(src) {
        if !opts.recursive {
            write_str("cp: -r not specified; omitting directory '");
            write_bytes(src);
            write_str("'\r\n");
            return 1;
        }
        copy_recursive(src, dest, opts)
    } else {
        copy_file(src, dest, opts)
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
                b'r' | b'R' => opts.recursive = true,
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
        write_str("cp: missing file operand\r\n");
        write_str("Usage: cp [OPTIONS] SOURCE DEST\r\n");
        write_str("       cp [OPTIONS] SOURCE... DIRECTORY\r\n");
        exit(1);
    }

    // Last argument is destination
    let dest = unsafe { &PATHS[path_count - 1][..PATH_LENS[path_count - 1]] };
    let dest_is_dir = is_directory(dest);

    // If multiple sources, dest must be a directory
    if path_count > 2 && !dest_is_dir {
        write_str("cp: target '");
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

        let result = copy_entry(src, actual_dest, &opts);
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
