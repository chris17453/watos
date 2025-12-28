//! WATOS ln command - create links
//!
//! Usage: ln [OPTIONS] TARGET LINK_NAME
//!        ln [OPTIONS] TARGET        (creates link in current directory)
//!
//! Options:
//!   -s    Create symbolic link (default, hard links not supported yet)
//!   -f    Force - remove existing destination files
//!   -v    Verbose - print each link created

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

fn symlink(target: &[u8], linkpath: &[u8]) -> u64 {
    unsafe {
        let packed = ((target.len() as u64) << 32) | (linkpath.len() as u64);
        syscall3(
            syscall::SYS_SYMLINK,
            target.as_ptr() as u64,
            packed,
            linkpath.as_ptr() as u64,
        )
    }
}

fn unlink(path: &[u8]) -> u64 {
    unsafe {
        syscall2(syscall::SYS_UNLINK, path.as_ptr() as u64, path.len() as u64)
    }
}

struct Options {
    symbolic: bool,
    force: bool,
    verbose: bool,
}

impl Options {
    fn new() -> Self {
        Options {
            symbolic: true, // Default to symbolic links
            force: false,
            verbose: false,
        }
    }
}

fn get_basename(path: &[u8]) -> &[u8] {
    // Find last / or \
    let mut last_sep = 0;
    for (i, &c) in path.iter().enumerate() {
        if c == b'/' || c == b'\\' {
            last_sep = i + 1;
        }
    }
    &path[last_sep..]
}

fn print_usage() {
    write_str("Usage: ln [-sfv] TARGET [LINK_NAME]\r\n");
    write_str("Create a link to TARGET with the name LINK_NAME.\r\n");
    write_str("\r\n");
    write_str("Options:\r\n");
    write_str("  -s    Create symbolic link (default)\r\n");
    write_str("  -f    Force - remove existing files\r\n");
    write_str("  -v    Verbose output\r\n");
}

#[no_mangle]
extern "C" fn _start() -> ! {
    static mut ARGS_BUF: [u8; 512] = [0u8; 512];

    let args_len = unsafe { get_args(&mut ARGS_BUF) };
    let args = unsafe { &ARGS_BUF[..args_len] };

    let mut opts = Options::new();
    let mut i = 0;

    // Skip command name "ln"
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
                b's' => opts.symbolic = true,
                b'f' => opts.force = true,
                b'v' => opts.verbose = true,
                b'h' => {
                    print_usage();
                    exit(0);
                }
                _ => {}
            }
            i += 1;
        }
        while i < args.len() && args[i] == b' ' {
            i += 1;
        }
    }

    // Parse TARGET
    if i >= args.len() {
        write_str("ln: missing file operand\r\n");
        print_usage();
        exit(1);
    }

    let target_start = i;
    while i < args.len() && args[i] != b' ' {
        i += 1;
    }
    let target = &args[target_start..i];

    // Skip space
    while i < args.len() && args[i] == b' ' {
        i += 1;
    }

    // Parse LINK_NAME (optional - defaults to basename of target in current dir)
    let linkpath: &[u8];
    let mut linkpath_buf = [0u8; 256];

    if i < args.len() {
        let link_start = i;
        while i < args.len() && args[i] != b' ' {
            i += 1;
        }
        linkpath = &args[link_start..i];
    } else {
        // Use basename of target
        let basename = get_basename(target);
        linkpath_buf[..basename.len()].copy_from_slice(basename);
        linkpath = &linkpath_buf[..basename.len()];
    }

    // Force remove existing?
    if opts.force {
        let _ = unlink(linkpath);
    }

    // Create the symlink
    let result = symlink(target, linkpath);

    if result != 0 {
        write_str("ln: failed to create symbolic link '");
        write_bytes(linkpath);
        write_str("' -> '");
        write_bytes(target);
        write_str("'\r\n");
        exit(1);
    }

    if opts.verbose {
        write_str("'");
        write_bytes(linkpath);
        write_str("' -> '");
        write_bytes(target);
        write_str("'\r\n");
    }

    exit(0);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    exit(1);
}
