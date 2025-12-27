#![no_std]
#![no_main]

use uefi::prelude::*;
use uefi::table::boot::{AllocateType, MemoryType};
use uefi::proto::console::gop::GraphicsOutput;
use uefi::proto::media::file::{File, FileAttribute, FileMode, FileType};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::cstr16;
use core::fmt::Write;

/// Maximum number of preloaded apps
const MAX_PRELOADED_APPS: usize = 16;

/// Entry for a preloaded application
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PreloadedApp {
    pub name: [u8; 32],    // Null-terminated name (e.g., "date", "echo")
    pub addr: u64,         // Load address
    pub size: u64,         // Size in bytes
}

impl PreloadedApp {
    const fn empty() -> Self {
        PreloadedApp {
            name: [0; 32],
            addr: 0,
            size: 0,
        }
    }
}

// Boot info structure passed to kernel at 0x80000
#[repr(C)]
pub struct BootInfo {
    pub magic: u32,                // WATOS magic
    pub framebuffer_addr: u64,     // GOP framebuffer address
    pub framebuffer_width: u32,    // Pixels
    pub framebuffer_height: u32,   // Pixels
    pub framebuffer_pitch: u32,    // Bytes per scanline
    pub framebuffer_bpp: u32,      // Bits per pixel
    pub pixel_format: u32,         // 0=RGB, 1=BGR
    pub init_app_addr: u64,        // Address of loaded init app (TERM.EXE)
    pub init_app_size: u64,        // Size of init app in bytes
    pub app_count: u32,            // Number of preloaded apps
    pub _pad: u32,                 // Padding for alignment
    pub apps: [PreloadedApp; MAX_PRELOADED_APPS], // Preloaded app table
}

const BOOT_INFO_ADDR: u64 = 0x80000;
const BOOT_MAGIC: u32 = 0x5741544F;  // "WATO" in ASCII

#[entry]
fn main(_image_handle: Handle, mut system_table: SystemTable<Boot>) -> Status {

    // Print boot message
    system_table
        .stdout()
        .write_str("WATOS UEFI Bootloader\n")
        .unwrap();
    system_table
        .stdout()
        .write_str("=====================\n")
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

    // Load SYSTEM/TERM.EXE from the boot filesystem
    let (init_app_addr, init_app_size) = load_init_app(&mut system_table);

    if init_app_addr != 0 {
        writeln!(system_table.stdout(), "Loaded SYSTEM/TERM.EXE at 0x{:x} ({} bytes)",
                 init_app_addr, init_app_size).unwrap();
    } else {
        writeln!(system_table.stdout(), "Warning: SYSTEM/TERM.EXE not found").unwrap();
    }

    // Load apps from /apps/system
    let (app_count, apps) = load_system_apps(&mut system_table);

    if app_count > 0 {
        writeln!(system_table.stdout(), "Loaded {} apps from /apps/system", app_count).unwrap();
    }

    // Load the WATOS kernel binary
    let kernel_binary = include_bytes!("../../../kernel.bin");

    unsafe {
        core::ptr::copy_nonoverlapping(
            kernel_binary.as_ptr(),
            kernel_addr as *mut u8,
            kernel_binary.len(),
        );
    }

    writeln!(system_table.stdout(), "Loaded WATOS kernel ({} bytes)", kernel_binary.len())
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
        init_app_addr,
        init_app_size,
        app_count,
        _pad: 0,
        apps,
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

/// Load SYSTEM/TERM.EXE from the boot filesystem
fn load_init_app(system_table: &mut SystemTable<Boot>) -> (u64, u64) {
    // Get the filesystem handle
    let fs_handle = match system_table
        .boot_services()
        .get_handle_for_protocol::<SimpleFileSystem>()
    {
        Ok(h) => h,
        Err(_) => return (0, 0),
    };

    let mut fs = match system_table
        .boot_services()
        .open_protocol_exclusive::<SimpleFileSystem>(fs_handle)
    {
        Ok(fs) => fs,
        Err(_) => return (0, 0),
    };

    // Open the root directory
    let mut root = match fs.open_volume() {
        Ok(r) => r,
        Err(_) => return (0, 0),
    };

    // Open SYSTEM directory
    let system_dir_handle = match root.open(cstr16!("SYSTEM"), FileMode::Read, FileAttribute::empty()) {
        Ok(h) => h,
        Err(_) => return (0, 0),
    };

    let mut system_dir = match system_dir_handle.into_type() {
        Ok(FileType::Dir(d)) => d,
        _ => return (0, 0),
    };

    // Open TERM.EXE
    let term_handle = match system_dir.open(cstr16!("TERM.EXE"), FileMode::Read, FileAttribute::empty()) {
        Ok(h) => h,
        Err(_) => return (0, 0),
    };

    let mut term_file = match term_handle.into_type() {
        Ok(FileType::Regular(f)) => f,
        _ => return (0, 0),
    };

    // Get file size
    let mut info_buf = [0u8; 256];
    let file_info = match term_file.get_info::<uefi::proto::media::file::FileInfo>(&mut info_buf) {
        Ok(i) => i,
        Err(_) => return (0, 0),
    };
    let file_size = file_info.file_size();

    // Allocate memory for the app at 0x600000 (6MB) - after kernel heap
    let app_pages = ((file_size + 0xFFF) / 0x1000) as usize;
    let app_addr: u64 = 0x600000;

    if system_table
        .boot_services()
        .allocate_pages(
            AllocateType::Address(app_addr),
            MemoryType::LOADER_DATA,
            app_pages,
        )
        .is_err()
    {
        return (0, 0);
    }

    // Read the file
    let buffer = unsafe {
        core::slice::from_raw_parts_mut(app_addr as *mut u8, file_size as usize)
    };

    if term_file.read(buffer).is_err() {
        return (0, 0);
    }

    (app_addr, file_size)
}

/// Load apps from /apps/system directory
fn load_system_apps(system_table: &mut SystemTable<Boot>) -> (u32, [PreloadedApp; MAX_PRELOADED_APPS]) {
    let mut apps = [PreloadedApp::empty(); MAX_PRELOADED_APPS];
    let mut count = 0u32;

    // Starting address for system apps (after TERM.EXE at 0x600000, give it 256KB)
    let mut next_addr: u64 = 0x700000;

    // Get the filesystem handle
    let fs_handle = match system_table
        .boot_services()
        .get_handle_for_protocol::<SimpleFileSystem>()
    {
        Ok(h) => h,
        Err(_) => return (0, apps),
    };

    let mut fs = match system_table
        .boot_services()
        .open_protocol_exclusive::<SimpleFileSystem>(fs_handle)
    {
        Ok(fs) => fs,
        Err(_) => return (0, apps),
    };

    // Open the root directory
    let mut root = match fs.open_volume() {
        Ok(r) => r,
        Err(_) => return (0, apps),
    };

    // Open apps directory
    let apps_dir_handle = match root.open(cstr16!("apps"), FileMode::Read, FileAttribute::empty()) {
        Ok(h) => h,
        Err(_) => return (0, apps),
    };

    let mut apps_dir = match apps_dir_handle.into_type() {
        Ok(FileType::Dir(d)) => d,
        _ => return (0, apps),
    };

    // Open system subdirectory
    let system_dir_handle = match apps_dir.open(cstr16!("system"), FileMode::Read, FileAttribute::empty()) {
        Ok(h) => h,
        Err(_) => return (0, apps),
    };

    let mut system_dir = match system_dir_handle.into_type() {
        Ok(FileType::Dir(d)) => d,
        _ => return (0, apps),
    };

    // Read directory entries
    let mut entry_buf = [0u8; 512];
    loop {
        let info = match system_dir.read_entry(&mut entry_buf) {
            Ok(Some(info)) => info,
            _ => break,
        };

        // Skip . and .. and directories
        if info.attribute().contains(FileAttribute::DIRECTORY) {
            continue;
        }

        // Get the filename as UTF-8
        let filename = info.file_name();
        let mut name_buf = [0u8; 32];
        let mut name_len = 0;
        for c in filename.iter() {
            if name_len >= 31 {
                break;
            }
            // Only handle ASCII for now - Char16 implements Into<u16>
            let ch: u16 = (*c).into();
            if ch == 0 || ch > 127 {
                if ch == 0 { break; }
                continue;
            }
            name_buf[name_len] = ch as u8;
            name_len += 1;
        }

        if name_len == 0 {
            continue;
        }

        // Open and load the file
        let file_handle = match system_dir.open(filename, FileMode::Read, FileAttribute::empty()) {
            Ok(h) => h,
            Err(_) => continue,
        };

        let mut file = match file_handle.into_type() {
            Ok(FileType::Regular(f)) => f,
            _ => continue,
        };

        // Get file size
        let mut file_info_buf = [0u8; 256];
        let file_info = match file.get_info::<uefi::proto::media::file::FileInfo>(&mut file_info_buf) {
            Ok(i) => i,
            Err(_) => continue,
        };
        let file_size = file_info.file_size();

        if file_size == 0 {
            continue;
        }

        // Allocate memory for the app
        let app_pages = ((file_size + 0xFFF) / 0x1000) as usize;

        if system_table
            .boot_services()
            .allocate_pages(
                AllocateType::Address(next_addr),
                MemoryType::LOADER_DATA,
                app_pages,
            )
            .is_err()
        {
            continue;
        }

        // Read the file
        let buffer = unsafe {
            core::slice::from_raw_parts_mut(next_addr as *mut u8, file_size as usize)
        };

        if file.read(buffer).is_err() {
            continue;
        }

        // Store in apps table
        if (count as usize) < MAX_PRELOADED_APPS {
            apps[count as usize] = PreloadedApp {
                name: name_buf,
                addr: next_addr,
                size: file_size,
            };

            count += 1;
            // Align next_addr to 64KB boundary
            next_addr = (next_addr + file_size + 0xFFFF) & !0xFFFF;
        }
    }

    (count, apps)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}