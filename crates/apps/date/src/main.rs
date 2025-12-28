//! WATOS date command
//!
//! Displays the current date and time.

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use watos_syscall::numbers as syscall;

// ============================================================================
// Syscall wrappers
// ============================================================================

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
        // fd=1 is stdout, which goes to kernel console buffer
        syscall3(syscall::SYS_WRITE, 1, s.as_ptr() as u64, s.len() as u64);
    }
}

fn exit(code: i32) -> ! {
    unsafe {
        syscall1(syscall::SYS_EXIT, code as u64);
    }
    loop {}
}

fn get_date() -> (u16, u8, u8) {
    let packed = unsafe { syscall0(syscall::SYS_GETDATE) } as u32;
    let year = (packed >> 16) as u16;
    let month = ((packed >> 8) & 0xFF) as u8;
    let day = (packed & 0xFF) as u8;
    (year, month, day)
}

fn get_time() -> (u8, u8, u8) {
    let packed = unsafe { syscall0(syscall::SYS_GETTIME) } as u32;
    let hours = ((packed >> 16) & 0xFF) as u8;
    let minutes = ((packed >> 8) & 0xFF) as u8;
    let seconds = (packed & 0xFF) as u8;
    (hours, minutes, seconds)
}

// ============================================================================
// Number formatting
// ============================================================================

fn write_num(n: u32, width: usize) {
    let mut buf = [b'0'; 10];
    let mut i = buf.len();
    let mut val = n;

    loop {
        i -= 1;
        buf[i] = b'0' + (val % 10) as u8;
        val /= 10;
        if val == 0 {
            break;
        }
    }

    // Pad with zeros to width
    let digits = buf.len() - i;
    for _ in 0..(width.saturating_sub(digits)) {
        write_str("0");
    }

    if let Ok(s) = core::str::from_utf8(&buf[i..]) {
        write_str(s);
    }
}

// ============================================================================
// Entry point
// ============================================================================

#[no_mangle]
extern "C" fn _start() -> ! {
    let (year, month, day) = get_date();
    let (hours, minutes, seconds) = get_time();

    // Print date in YYYY-MM-DD format
    write_num(year as u32, 4);
    write_str("-");
    write_num(month as u32, 2);
    write_str("-");
    write_num(day as u32, 2);

    write_str(" ");

    // Print time in HH:MM:SS format
    write_num(hours as u32, 2);
    write_str(":");
    write_num(minutes as u32, 2);
    write_str(":");
    write_num(seconds as u32, 2);

    write_str("\r\n");

    exit(0);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    write_str("PANIC in date\r\n");
    exit(1);
}
