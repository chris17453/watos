//! CAT - Concatenate and display files
//! Usage: CAT <file1> [file2] [file3] ...

#![no_std]
#![no_main]

use watos_coreutils::{println, exit};

#[no_mangle]
pub extern "C" fn _start() -> ! {
    println("CAT: Native 64-bit WATOS utility");
    println("File I/O support coming soon");
    exit(0)
}
