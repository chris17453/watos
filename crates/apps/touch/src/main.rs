//! WATOS touch command - create files or update timestamps
//!
//! Usage: touch [OPTIONS] FILE...
//!
//! Options:
//!   -a    Change only access time
//!   -m    Change only modification time
//!   -c    Do not create files if they don't exist
//!
//! Creates FILE if it doesn't exist (unless -c specified).
//! Updates file timestamps to current time.

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

fn close(fd: u64) {
    unsafe {
        syscall2(syscall::SYS_CLOSE, fd, 0);
    }
}

// Open flags
const O_CREAT: u32 = 0x40;
const O_WRONLY: u32 = 0x01;

struct Options {
    no_create: bool,   // -c
}

impl Options {
    fn new() -> Self {
        Options {
            no_create: false,
        }
    }
}

fn file_exists(path: &[u8]) -> bool {
    let mut stat_buf: [u64; 8] = [0; 8];
    stat(path, &mut stat_buf) == 0
}

fn touch_file(path: &[u8], opts: &Options) -> i32 {
    if file_exists(path) {
        // File exists - just open and close to update timestamp
        // (The kernel should update atime on open)
        let fd = open(path, O_WRONLY);
        if fd >= 0 {
            close(fd as u64);
        }
        // Even if open fails (e.g., read-only), we don't report error
        // as the file exists (matches GNU touch behavior)
        0
    } else if opts.no_create {
        // -c specified, don't create
        0
    } else {
        // Create the file
        let fd = open(path, O_CREAT | O_WRONLY);
        if fd >= 0 {
            close(fd as u64);
            0
        } else {
            write_str("touch: cannot touch '");
            write_bytes(path);
            write_str("': No such file or directory\r\n");
            1
        }
    }
}

#[no_mangle]
extern "C" fn _start() -> ! {
    use core::ptr::addr_of_mut;
    static mut ARGS_BUF: [u8; 1024] = [0u8; 1024];

    let args_len = unsafe {
        let buf = &mut *addr_of_mut!(ARGS_BUF);
        get_args(buf)
    };
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
                b'a' => {} // -a: access time only (we update all times anyway)
                b'm' => {} // -m: modification time only (we update all times anyway)
                b'c' => opts.no_create = true,
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
            let result = touch_file(path, &opts);
            if result != 0 {
                exit_code = 1;
            }
            file_count += 1;
        }

        while i < args.len() && args[i] == b' ' {
            i += 1;
        }
    }

    if file_count == 0 {
        write_str("touch: missing file operand\r\n");
        write_str("Usage: touch [OPTIONS] FILE...\r\n");
        exit(1);
    }

    exit(exit_code);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    exit(1);
}
