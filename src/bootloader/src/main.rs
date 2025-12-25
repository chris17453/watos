#![no_std]
#![no_main]

use uefi::prelude::*;
use uefi::table::boot::{AllocateType, MemoryType};
use uefi::proto::console::gop::GraphicsOutput;
use core::fmt::Write;

// Boot info structure passed to kernel at 0x80000
#[repr(C)]
pub struct BootInfo {
    pub magic: u32,                // 0xDOS64 magic
    pub framebuffer_addr: u64,     // GOP framebuffer address
    pub framebuffer_width: u32,    // Pixels
    pub framebuffer_height: u32,   // Pixels
    pub framebuffer_pitch: u32,    // Bytes per scanline
    pub framebuffer_bpp: u32,      // Bits per pixel
    pub pixel_format: u32,         // 0=RGB, 1=BGR
}

const BOOT_INFO_ADDR: u64 = 0x80000;
const BOOT_MAGIC: u32 = 0xD0564;  // "DOS64" in hex-ish

#[entry]
fn main(_image_handle: Handle, mut system_table: SystemTable<Boot>) -> Status {

    // Print boot message
    system_table
        .stdout()
        .write_str("DOS64 UEFI Bootloader\n")
        .unwrap();
    system_table
        .stdout()
        .write_str("====================\n")
        .unwrap();

    // Get GOP info in a separate scope so we can use system_table later
    let (fb_addr, width, height, stride, pf_value) = {
        let gop_handle = system_table
            .boot_services()
            .get_handle_for_protocol::<GraphicsOutput>()
            .expect("Failed to get GOP handle");

        let mut gop = system_table
            .boot_services()
            .open_protocol_exclusive::<GraphicsOutput>(gop_handle)
            .expect("Failed to open GOP");

        let mode_info = gop.current_mode_info();
        let (w, h) = mode_info.resolution();
        let s = mode_info.stride();
        let pixel_format = mode_info.pixel_format();
        let addr = gop.frame_buffer().as_mut_ptr() as u64;

        let pf = match pixel_format {
            uefi::proto::console::gop::PixelFormat::Rgb => 0u32,
            uefi::proto::console::gop::PixelFormat::Bgr => 1u32,
            _ => 0u32,
        };

        (addr, w, h, s, pf)
    };

    writeln!(system_table.stdout(), "GOP: {}x{} stride={} fb=0x{:x}",
             width, height, stride, fb_addr).unwrap();

    // Allocate memory for kernel at a fixed low address
    let kernel_pages = 64; // 64 * 4KB = 256KB for kernel
    let kernel_addr: u64 = 0x100000; // Load at 1MB - standard location

    system_table
        .boot_services()
        .allocate_pages(
            AllocateType::Address(kernel_addr),
            MemoryType::LOADER_CODE,
            kernel_pages,
        )
        .expect("Failed to allocate kernel memory at 1MB");

    writeln!(system_table.stdout(), "Allocated kernel memory at 0x{:x}", kernel_addr)
        .unwrap();

    // Load the DOS64 kernel binary
    let kernel_binary = include_bytes!("../../../kernel.bin");

    unsafe {
        core::ptr::copy_nonoverlapping(
            kernel_binary.as_ptr(),
            kernel_addr as *mut u8,
            kernel_binary.len(),
        );
    }

    writeln!(system_table.stdout(), "Loaded DOS64 kernel ({} bytes)", kernel_binary.len())
        .unwrap();

    // Write boot info structure for kernel
    let boot_info = BootInfo {
        magic: BOOT_MAGIC,
        framebuffer_addr: fb_addr,
        framebuffer_width: width as u32,
        framebuffer_height: height as u32,
        framebuffer_pitch: (stride * 4) as u32, // 4 bytes per pixel
        framebuffer_bpp: 32,
        pixel_format: pf_value,
    };

    unsafe {
        core::ptr::write(BOOT_INFO_ADDR as *mut BootInfo, boot_info);
    }

    writeln!(system_table.stdout(), "Boot info at 0x{:x}", BOOT_INFO_ADDR).unwrap();

    system_table
        .stdout()
        .write_str("Exiting boot services...\n")
        .unwrap();

    // Exit boot services
    let (_rt, _map) = system_table.exit_boot_services();

    // Jump to kernel
    let kernel_entry: extern "C" fn() -> ! = unsafe {
        core::mem::transmute(kernel_addr as *const ())
    };

    kernel_entry();
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}