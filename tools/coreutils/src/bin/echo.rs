//! ECHO - Display text to console
//! Usage: ECHO <text>

#![no_std]
#![no_main]

use watos_coreutils::{println, exit};

/// Entry point
#[no_mangle]
pub extern "C" fn _start() -> ! {
    // Get command line arguments from WATOS
    // For now, just print a simple message
    // TODO: Parse command line arguments when WATOS supports passing them
    
    println("ECHO: Command line argument passing not yet implemented");
    println("This is a native 64-bit WATOS utility");
    
    exit(0)
}
