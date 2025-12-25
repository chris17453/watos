#![no_std]

extern crate alloc;
use core::panic::PanicInfo;
use linked_list_allocator::LockedHeap;

#[global_allocator]
pub static ALLOCATOR: LockedHeap = LockedHeap::empty();

pub mod interrupts;
pub mod net;
pub mod disk;
pub mod runtime;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        unsafe { core::arch::asm!("hlt") };
    }
}
