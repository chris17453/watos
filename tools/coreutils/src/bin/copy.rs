//! COPY - Copy files
//! Usage: COPY <source> <dest>

#![no_std]
#![no_main]

use watos_coreutils::{println, exit};

#[no_mangle]
pub extern "C" fn _start() -> ! {
    println("COPY: Native 64-bit WATOS utility");
    println("File I/O support coming soon");
    exit(0)
}
