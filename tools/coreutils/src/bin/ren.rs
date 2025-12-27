//! REN - Rename files
//! Usage: REN <oldname> <newname>

#![no_std]
#![no_main]

use watos_coreutils::{println, exit};

#[no_mangle]
pub extern "C" fn _start() -> ! {
    println("REN: Native 64-bit WATOS utility");
    println("File I/O support coming soon");
    exit(0)
}
