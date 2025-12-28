//! WATOS lsblk command - list block devices
//!
//! Usage: lsblk [OPTIONS]
//!
//! Lists information about block devices (disks, partitions).

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

fn lsblk(buf: &mut [u8]) -> usize {
    unsafe { syscall2(syscall::SYS_LSBLK, buf.as_mut_ptr() as u64, buf.len() as u64) as usize }
}

fn list_drives(buf: &mut [u8]) -> usize {
    unsafe { syscall2(syscall::SYS_LISTDRIVES, buf.as_mut_ptr() as u64, buf.len() as u64) as usize }
}

#[no_mangle]
extern "C" fn _start() -> ! {
    static mut BLK_BUF: [u8; 1024] = [0u8; 1024];
    static mut DRV_BUF: [u8; 512] = [0u8; 512];

    write_str("NAME        SIZE   TYPE   FSTYPE  MOUNT\r\n");
    write_str("----------- ------ ------ ------- -----\r\n");

    // Get block device info
    let blk_len = unsafe { lsblk(&mut BLK_BUF) };

    if blk_len > 0 {
        // Parse and display block devices
        // Format from kernel: "NAME:SIZE:TYPE:FSTYPE\n" per device
        let blk = unsafe { &BLK_BUF[..blk_len] };
        let mut line_start = 0;

        for i in 0..blk_len {
            if blk[i] == b'\n' {
                let line = &blk[line_start..i];
                display_device_line(line);
                line_start = i + 1;
            }
        }
    } else {
        // No block device syscall, show what we know from drives
        let drv_len = unsafe { list_drives(&mut DRV_BUF) };

        if drv_len > 0 {
            let drv = unsafe { &DRV_BUF[..drv_len] };
            let mut line_start = 0;

            for i in 0..drv_len {
                if drv[i] == b'\n' {
                    let line = &drv[line_start..i];
                    // Format: NAME:PATH:FSTYPE[*]
                    display_drive_line(line);
                    line_start = i + 1;
                }
            }
        } else {
            write_str("(no block devices detected)\r\n");
        }
    }

    exit(0);
}

fn display_device_line(line: &[u8]) {
    // Parse "NAME:SIZE:TYPE:FSTYPE"
    let mut parts: [&[u8]; 4] = [&[], &[], &[], &[]];
    let mut part_idx = 0;
    let mut start = 0;

    for (i, &c) in line.iter().enumerate() {
        if c == b':' && part_idx < 3 {
            parts[part_idx] = &line[start..i];
            part_idx += 1;
            start = i + 1;
        }
    }
    if part_idx < 4 {
        parts[part_idx] = &line[start..];
    }

    // NAME (12 chars)
    write_bytes(parts[0]);
    for _ in parts[0].len()..12 {
        write_str(" ");
    }

    // SIZE (7 chars)
    write_bytes(parts[1]);
    for _ in parts[1].len()..7 {
        write_str(" ");
    }

    // TYPE (7 chars)
    write_bytes(parts[2]);
    for _ in parts[2].len()..7 {
        write_str(" ");
    }

    // FSTYPE
    write_bytes(parts[3]);
    write_str("\r\n");
}

fn display_drive_line(line: &[u8]) {
    // Parse "NAME:PATH:FSTYPE[*]"
    let mut parts: [&[u8]; 3] = [&[], &[], &[]];
    let mut part_idx = 0;
    let mut start = 0;
    let mut is_current = false;

    for (i, &c) in line.iter().enumerate() {
        if c == b':' && part_idx < 2 {
            parts[part_idx] = &line[start..i];
            part_idx += 1;
            start = i + 1;
        } else if c == b'*' {
            is_current = true;
        }
    }
    if part_idx < 3 {
        let end = if is_current && start < line.len() { line.len() - 1 } else { line.len() };
        parts[part_idx] = &line[start..end];
    }

    // NAME: (drive letter as disk name)
    write_bytes(parts[0]);
    write_str(":         ");  // Pad to 12 chars

    // SIZE: unknown
    write_str("-      ");

    // TYPE: disk
    write_str("disk   ");

    // FSTYPE
    write_bytes(parts[2]);
    for _ in parts[2].len()..8 {
        write_str(" ");
    }

    // MOUNT
    write_bytes(parts[1]);
    if is_current {
        write_str(" *");
    }
    write_str("\r\n");
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    exit(1);
}
