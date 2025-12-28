//! WATOS uptime command
//!
//! Show how long the system has been running.

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
        syscall3(syscall::SYS_WRITE, 1, s.as_ptr() as u64, s.len() as u64);
    }
}

fn exit(code: i32) -> ! {
    unsafe {
        syscall1(syscall::SYS_EXIT, code as u64);
    }
    loop {}
}

fn get_ticks() -> u64 {
    unsafe { syscall0(syscall::SYS_GETTICKS) }
}

fn get_time() -> (u8, u8, u8) {
    let packed = unsafe { syscall0(syscall::SYS_GETTIME) } as u32;
    let hours = ((packed >> 16) & 0xFF) as u8;
    let minutes = ((packed >> 8) & 0xFF) as u8;
    let seconds = (packed & 0xFF) as u8;
    (hours, minutes, seconds)
}

fn write_num(n: u32) {
    if n >= 10 {
        write_num(n / 10);
    }
    let digit = (n % 10) as u8 + b'0';
    unsafe {
        syscall3(syscall::SYS_WRITE, 1, &digit as *const u8 as u64, 1);
    }
}

fn write_num_padded(n: u32, width: usize) {
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

    let digits = buf.len() - i;
    for _ in 0..(width.saturating_sub(digits)) {
        write_str("0");
    }

    if let Ok(s) = core::str::from_utf8(&buf[i..]) {
        write_str(s);
    }
}

#[no_mangle]
extern "C" fn _start() -> ! {
    // Get current time
    let (hours, minutes, seconds) = get_time();

    // Get ticks since boot (PIT runs at ~18.2 Hz)
    let ticks = get_ticks();

    // Convert ticks to seconds (approximately 18.2 ticks per second)
    let total_seconds = ticks * 10 / 182; // More accurate: 1000/18.2 ≈ 54.9ms per tick

    let uptime_hours = total_seconds / 3600;
    let uptime_minutes = (total_seconds % 3600) / 60;
    let uptime_seconds = total_seconds % 60;

    // Print current time
    write_str(" ");
    write_num_padded(hours as u32, 2);
    write_str(":");
    write_num_padded(minutes as u32, 2);
    write_str(":");
    write_num_padded(seconds as u32, 2);

    // Print uptime
    write_str(" up ");
    if uptime_hours > 0 {
        write_num(uptime_hours as u32);
        if uptime_hours == 1 {
            write_str(" hour, ");
        } else {
            write_str(" hours, ");
        }
    }
    write_num(uptime_minutes as u32);
    if uptime_minutes == 1 {
        write_str(" minute");
    } else {
        write_str(" minutes");
    }

    write_str("\r\n");
    exit(0);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    exit(1);
}
