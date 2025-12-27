#![no_std]

extern crate alloc;
use core::panic::PanicInfo;


pub mod interrupts;
pub mod net;
pub mod disk;
pub mod runtime;
pub mod process;
pub mod console;
pub mod mmu;
pub mod io;
pub mod globals;


#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        unsafe { core::arch::asm!("hlt") };
    }
}
