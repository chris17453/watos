//! WATOS mem command - display memory usage
//!
//! Usage: mem [-h]
//!
//! Options:
//!   -h    Human-readable sizes

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use watos_syscall::numbers as syscall;

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

fn meminfo(buf: &mut [u64; 5]) -> u64 {
    unsafe { syscall1(syscall::SYS_MEMINFO, buf.as_mut_ptr() as u64) }
}

/// Format a u64 number to decimal string
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

/// Format bytes to human-readable size (KiB, MiB, GiB)
fn format_size_human(bytes: u64, buf: &mut [u8]) -> usize {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes;
    let mut unit_idx = 0;

    while size >= 1024 && unit_idx < UNITS.len() - 1 {
        size /= 1024;
        unit_idx += 1;
    }

    let len = format_u64(size, buf);
    let unit = UNITS[unit_idx].as_bytes();
    for (i, &b) in unit.iter().enumerate() {
        if len + i < buf.len() {
            buf[len + i] = b;
        }
    }
    len + unit.len()
}

/// Write string padded on the right to reach minimum width
fn write_padded_right(s: &[u8], width: usize) {
    let padding = if s.len() < width { width - s.len() } else { 0 };
    for _ in 0..padding {
        write_str(" ");
    }
    write_bytes(s);
}

/// Write string padded on the left to reach minimum width
fn write_padded_left(s: &[u8], width: usize) {
    write_bytes(s);
    let padding = if s.len() < width { width - s.len() } else { 0 };
    for _ in 0..padding {
        write_str(" ");
    }
}

#[no_mangle]
extern "C" fn _start() -> ! {
    static mut ARGS_BUF: [u8; 256] = [0u8; 256];
    static mut MEM_BUF: [u64; 5] = [0; 5];

    let args_len = unsafe { get_args(&mut ARGS_BUF) };
    let args = unsafe { &ARGS_BUF[..args_len] };

    // Check for -h flag (human-readable)
    let mut human = false;
    let mut i = 0;

    // Skip command name
    while i < args.len() && args[i] != b' ' {
        i += 1;
    }
    while i < args.len() && args[i] == b' ' {
        i += 1;
    }

    // Check for -h flag
    if i < args.len() && args[i] == b'-' && i + 1 < args.len() && args[i + 1] == b'h' {
        human = true;
    }

    // Get memory info
    let result = unsafe { meminfo(&mut MEM_BUF) };
    if result != 0 {
        write_str("Error: Failed to get memory information\r\n");
        exit(1);
    }

    let mem_stats = unsafe { &MEM_BUF };
    let phys_total = mem_stats[0];
    let phys_free = mem_stats[1];
    let phys_used = mem_stats[2];
    let heap_total = mem_stats[3];
    let heap_used = mem_stats[4];

    let mut buf = [0u8; 32];

    // Print header
    write_str("              ");
    write_padded_right(b"total", 12);
    write_padded_right(b"used", 12);
    write_padded_right(b"free", 12);
    write_str("\r\n");

    // Physical memory
    write_padded_left(b"Phys:", 14);

    let len = if human {
        format_size_human(phys_total, &mut buf)
    } else {
        format_u64(phys_total, &mut buf)
    };
    write_padded_right(&buf[..len], 12);

    let len = if human {
        format_size_human(phys_used, &mut buf)
    } else {
        format_u64(phys_used, &mut buf)
    };
    write_padded_right(&buf[..len], 12);

    let len = if human {
        format_size_human(phys_free, &mut buf)
    } else {
        format_u64(phys_free, &mut buf)
    };
    write_padded_right(&buf[..len], 12);
    write_str("\r\n");

    // Kernel heap
    let heap_free = heap_total - heap_used;
    write_padded_left(b"Heap:", 14);

    let len = if human {
        format_size_human(heap_total, &mut buf)
    } else {
        format_u64(heap_total, &mut buf)
    };
    write_padded_right(&buf[..len], 12);

    let len = if human {
        format_size_human(heap_used, &mut buf)
    } else {
        format_u64(heap_used, &mut buf)
    };
    write_padded_right(&buf[..len], 12);

    let len = if human {
        format_size_human(heap_free, &mut buf)
    } else {
        format_u64(heap_free, &mut buf)
    };
    write_padded_right(&buf[..len], 12);
    write_str("\r\n");

    exit(0);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    exit(1);
}
