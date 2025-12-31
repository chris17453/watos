//! chcp - Change code page
//!
//! Usage: chcp [<codepage>]
//! Example: chcp        (show current code page)
//!          chcp 437    (change to CP437)
//!          chcp 850    (change to CP850)

#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use watos_runtime::{entry_point, print, println, exit};
use watos_syscall as syscall;

entry_point!(main);

fn main(_args: Vec<String>) -> i32 {
    let args = watos_runtime::args();

    // No arguments - show current code page
    if args.len() == 1 {
        let current_cp = unsafe { syscall::syscall0(syscall::SYS_GET_CODEPAGE) };
        println!("Active code page: {}", current_cp);
        return 0;
    }

    if args.len() != 2 {
        println!("Usage: chcp [<codepage>]");
        println!("       chcp         - Show current code page");
        println!("       chcp 437     - Change to CP437");
        println!("Available code pages: 437, 850, 1252");
        return 1;
    }

    // Parse code page number
    let cp_str = &args[1];
    let cp_num: u16 = match cp_str.parse() {
        Ok(n) => n,
        Err(_) => {
            println!("Error: Invalid code page number '{}'", cp_str);
            return 1;
        }
    };

    // Validate code page number
    match cp_num {
        437 | 850 | 1252 => {},
        _ => {
            println!("Error: Unsupported code page {}", cp_num);
            println!("Available code pages: 437, 850, 1252");
            return 1;
        }
    }

    // Build path to code page file
    let path = alloc::format!("/system/codepages/cp{}.cpg", cp_num);

    // Open and read the code page file
    let fd = unsafe { syscall::open(path.as_bytes(), syscall::O_RDONLY) };
    if fd < 0 {
        println!("Error: Could not open code page file '{}'", path);
        return 1;
    }

    // Read entire file
    let mut buffer = Vec::new();
    buffer.resize(2048, 0u8); // Code page files are ~1100 bytes

    let bytes_read = unsafe {
        syscall::read(fd as u64, buffer.as_mut_ptr(), buffer.len() as u64)
    };

    unsafe { syscall::close(fd as u64); }

    if bytes_read <= 0 {
        println!("Error: Failed to read code page file");
        return 1;
    }

    // Verify magic header "CPAG"
    if bytes_read < 4 || &buffer[0..4] != b"CPAG" {
        println!("Error: Invalid code page file format");
        return 1;
    }

    // Send code page to kernel via syscall
    let result = unsafe {
        syscall::syscall2(
            syscall::SYS_SET_CODEPAGE,
            buffer.as_ptr() as u64,
            bytes_read as u64
        )
    };

    if result != 0 {
        println!("Error: Failed to load code page (error code: {})", result);
        return 1;
    }

    println!("Code page set to: {}", cp_num);
    0
}
