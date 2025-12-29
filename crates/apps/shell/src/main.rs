//! WATOS Shell - Simple command interpreter

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

fn read_line(buf: &mut [u8]) -> usize {
    let mut pos = 0;
    loop {
        let key = loop {
            let k = unsafe { syscall0(syscall::SYS_GETKEY) as u8 };
            if k != 0 {
                break k;
            }
            for _ in 0..10000 { core::hint::spin_loop(); }
        };

        if key == b'\n' || key == b'\r' {
            write_str("\r\n");
            break;
        } else if key == 0x08 || key == 0x7F {
            if pos > 0 {
                pos -= 1;
                write_str("\x08 \x08");
            }
        } else if key >= 0x20 && key < 0x7F && pos < buf.len() {
            buf[pos] = key;
            pos += 1;
            let echo = [key];
            unsafe {
                syscall3(syscall::SYS_WRITE, 1, echo.as_ptr() as u64, 1);
            }
        }
    }
    pos
}

fn exit(code: i32) -> ! {
    unsafe {
        syscall1(syscall::SYS_EXIT, code as u64);
    }
    loop {}
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    write_str("\r\n");
    write_str("WATOS Shell v0.1\r\n");
    write_str("Type 'help' for available commands\r\n");
    write_str("\r\n");

    let mut cmd_buf = [0u8; 128];

    loop {
        write_str("$ ");
        let len = read_line(&mut cmd_buf);

        if len == 0 {
            continue;
        }

        let cmd = &cmd_buf[..len];

        // Built-in commands
        if cmd == b"help" {
            write_str("Available commands:\r\n");
            write_str("  help     - Show this help\r\n");
            write_str("  clear    - Clear screen\r\n");
            write_str("  exit     - Exit shell\r\n");
            write_str("  echo     - Echo text\r\n");
            write_str("  ls       - List files\r\n");
            write_str("  pwd      - Print working directory\r\n");
            write_str("  cd       - Change directory\r\n");
            write_str("  uname    - System information\r\n");
            write_str("  ps       - Process list\r\n");
            write_str("  date     - Show date/time\r\n");
            write_str("\r\n");
        } else if cmd == b"exit" {
            write_str("Goodbye!\r\n");
            exit(0);
        } else if cmd == b"clear" {
            // ANSI clear screen
            write_str("\x1b[2J\x1b[H");
        } else {
            // Try to execute as external command
            // For now, just show a message (would need SYS_EXEC implementation)
            write_str("Command not found: ");
            unsafe {
                syscall3(syscall::SYS_WRITE, 1, cmd.as_ptr() as u64, len as u64);
            }
            write_str("\r\n");
        }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
