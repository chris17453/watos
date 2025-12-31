//! loadkeys - Load keyboard layout from file
//!
//! Usage: loadkeys <layout>
//! Example: loadkeys us
//!          loadkeys de

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

    if args.len() != 2 {
        println!("Usage: loadkeys <layout>");
        println!("Available layouts: us, uk, de, fr");
        return 1;
    }

    let layout = &args[1];

    // Validate layout name (simple check)
    match layout.as_str() {
        "us" | "uk" | "de" | "fr" => {},
        _ => {
            println!("Error: Unknown layout '{}'", layout);
            println!("Available layouts: us, uk, de, fr");
            return 1;
        }
    }

    // Build path to keymap file
    let path = alloc::format!("/system/keymaps/{}.kmap", layout);

    // Open and read the keymap file
    let fd = unsafe { syscall::open(path.as_bytes(), syscall::O_RDONLY) };
    if fd < 0 {
        println!("Error: Could not open keymap file '{}'", path);
        return 1;
    }

    // Read entire file
    let mut buffer = Vec::new();
    buffer.resize(2048, 0u8); // Keymap files are ~776 bytes

    let bytes_read = unsafe {
        syscall::read(fd as u64, buffer.as_mut_ptr(), buffer.len() as u64)
    };

    unsafe { syscall::close(fd as u64); }

    if bytes_read <= 0 {
        println!("Error: Failed to read keymap file");
        return 1;
    }

    // Verify magic header "KMAP"
    if bytes_read < 4 || &buffer[0..4] != b"KMAP" {
        println!("Error: Invalid keymap file format");
        return 1;
    }

    // Send keymap to kernel via syscall
    let result = unsafe {
        syscall::syscall2(
            syscall::SYS_SET_KEYMAP,
            buffer.as_ptr() as u64,
            bytes_read as u64
        )
    };

    if result != 0 {
        println!("Error: Failed to load keymap (error code: {})", result);
        return 1;
    }

    println!("Keyboard layout set to: {}", layout);
    0
}
