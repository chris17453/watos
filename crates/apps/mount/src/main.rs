//! WATOS mount command - mount filesystems
//!
//! Usage: mount                    - Show mounted filesystems
//!        mount DEVICE MOUNTPOINT  - Mount device at mountpoint
//!        mount -t TYPE DEVICE MOUNTPOINT
//!
//! Examples:
//!   mount                   # List all mounts
//!   mount /dev/sda1 /mnt/c  # Mount device
//!   mount D: /mnt/d         # Mount drive letter

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

fn list_drives(buf: &mut [u8]) -> usize {
    unsafe { syscall2(syscall::SYS_LISTDRIVES, buf.as_mut_ptr() as u64, buf.len() as u64) as usize }
}

fn mount_drive(name: &[u8], path: &[u8]) -> u64 {
    // Create null-terminated path
    static mut PATH_BUF: [u8; 65] = [0u8; 65];
    let path_len = path.len().min(64);
    unsafe {
        PATH_BUF[..path_len].copy_from_slice(&path[..path_len]);
        PATH_BUF[path_len] = 0;

        syscall3(
            syscall::SYS_MOUNT,
            name.as_ptr() as u64,
            name.len() as u64,
            PATH_BUF.as_ptr() as u64,
        )
    }
}

fn show_mounts() {
    static mut BUF: [u8; 1024] = [0u8; 1024];

    let len = unsafe { list_drives(&mut BUF) };

    if len == 0 {
        write_str("No filesystems mounted.\r\n");
        return;
    }

    let buf = unsafe { &BUF[..len] };
    let mut line_start = 0;

    for i in 0..len {
        if buf[i] == b'\n' {
            let line = &buf[line_start..i];

            // Parse "NAME:PATH:FSTYPE[*]"
            let mut colon1 = 0;
            let mut colon2 = 0;
            for (j, &c) in line.iter().enumerate() {
                if c == b':' {
                    if colon1 == 0 {
                        colon1 = j;
                    } else {
                        colon2 = j;
                        break;
                    }
                }
            }

            if colon1 > 0 && colon2 > colon1 {
                let name = &line[..colon1];
                let path = &line[colon1 + 1..colon2];
                let fstype_raw = &line[colon2 + 1..];
                let fstype = if fstype_raw.last() == Some(&b'*') {
                    &fstype_raw[..fstype_raw.len() - 1]
                } else {
                    fstype_raw
                };

                write_bytes(name);
                write_str(": on ");
                write_bytes(path);
                write_str(" type ");
                write_bytes(fstype);
                write_str("\r\n");
            }

            line_start = i + 1;
        }
    }
}

#[no_mangle]
extern "C" fn _start() -> ! {
    static mut ARGS_BUF: [u8; 256] = [0u8; 256];

    let args_len = unsafe { get_args(&mut ARGS_BUF) };
    let args = unsafe { &ARGS_BUF[..args_len] };

    // Parse arguments
    let mut words: [&[u8]; 5] = [&[], &[], &[], &[], &[]];
    let mut word_idx = 0;
    let mut start = 0;
    let mut in_word = false;

    for (i, &c) in args.iter().enumerate() {
        if c == b' ' || c == b'\t' {
            if in_word {
                if word_idx < 5 {
                    words[word_idx] = &args[start..i];
                    word_idx += 1;
                }
                in_word = false;
            }
        } else {
            if !in_word {
                start = i;
                in_word = true;
            }
        }
    }
    if in_word && word_idx < 5 {
        words[word_idx] = &args[start..];
        word_idx += 1;
    }

    // words[0] = "mount"
    // words[1..] = arguments

    if word_idx <= 1 {
        // Just "mount" - list mounts
        show_mounts();
        exit(0);
    }

    // Check for -t option
    let (device, mountpoint) = if words[1] == b"-t" {
        // mount -t TYPE DEVICE MOUNTPOINT
        if word_idx < 5 {
            write_str("Usage: mount -t TYPE DEVICE MOUNTPOINT\r\n");
            exit(1);
        }
        // TYPE is words[2], ignore it for now (we auto-detect)
        (words[3], words[4])
    } else {
        // mount DEVICE MOUNTPOINT
        if word_idx < 3 {
            write_str("Usage: mount DEVICE MOUNTPOINT\r\n");
            write_str("       mount (to list mounts)\r\n");
            exit(1);
        }
        (words[1], words[2])
    };

    // Extract drive name from device (e.g., "D:" -> "D", "/dev/sda1" -> "sda1")
    let drive_name = if device.len() >= 2 && device[device.len() - 1] == b':' {
        &device[..device.len() - 1]
    } else if device.starts_with(b"/dev/") {
        &device[5..]
    } else {
        device
    };

    let result = mount_drive(drive_name, mountpoint);

    match result {
        0 => {
            write_bytes(drive_name);
            write_str(": mounted on ");
            write_bytes(mountpoint);
            write_str("\r\n");
        }
        1 => {
            write_str("mount: invalid drive name\r\n");
            exit(1);
        }
        2 => {
            write_str("mount: invalid mount path\r\n");
            exit(1);
        }
        3 => {
            write_str("mount: already mounted\r\n");
            exit(1);
        }
        4 => {
            write_str("mount: too many mounts\r\n");
            exit(1);
        }
        _ => {
            write_str("mount: failed\r\n");
            exit(1);
        }
    }

    exit(0);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    exit(1);
}
