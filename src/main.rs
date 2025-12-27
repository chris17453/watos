//! WATOS Kernel
//!
//! Minimal kernel that provides:
//! - Memory management
//! - Interrupt handling
//! - Syscall interface for user-space apps
//!
//! The kernel does NOT include terminal emulation - that's a user-space app.

#![no_std]
#![no_main]

extern crate alloc;

use core::panic::PanicInfo;
use linked_list_allocator::LockedHeap;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

const HEAP_START: usize = 0x200000;
const HEAP_SIZE: usize = 4 * 1024 * 1024;

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

/// Boot info passed from bootloader at 0x80000
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BootInfo {
    pub magic: u32,
    pub framebuffer_addr: u64,
    pub framebuffer_width: u32,
    pub framebuffer_height: u32,
    pub framebuffer_pitch: u32,
    pub framebuffer_bpp: u32,
    pub pixel_format: u32, // 0=RGB, 1=BGR
    pub init_app_addr: u64,   // Address of loaded init app (TERM.EXE)
    pub init_app_size: u64,   // Size of init app in bytes
    pub app_count: u32,       // Number of preloaded apps
    pub _pad: u32,            // Padding for alignment
    pub apps: [PreloadedApp; MAX_PRELOADED_APPS], // Preloaded app table
}

const BOOT_INFO_ADDR: usize = 0x80000;
const BOOT_MAGIC: u32 = 0x5741544F; // "WATO"

/// Global boot info (copied from bootloader)
static mut BOOT_INFO: Option<BootInfo> = None;

/// Find a preloaded app by name (case-insensitive)
fn find_preloaded_app(name: &[u8]) -> Option<(u64, u64)> {
    unsafe {
        let info = BOOT_INFO.as_ref()?;
        for i in 0..(info.app_count as usize) {
            let app = &info.apps[i];
            // Get the app's name length (null-terminated)
            let app_name_len = app.name.iter().position(|&c| c == 0).unwrap_or(32);
            let app_name = &app.name[..app_name_len];

            // Compare names (case-insensitive for flexibility)
            if name.len() == app_name.len() {
                let matches = name.iter().zip(app_name.iter()).all(|(a, b)| {
                    let a_lower = if *a >= b'A' && *a <= b'Z' { *a + 32 } else { *a };
                    let b_lower = if *b >= b'A' && *b <= b'Z' { *b + 32 } else { *b };
                    a_lower == b_lower
                });
                if matches {
                    return Some((app.addr, app.size));
                }
            }
        }
        None
    }
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    // 1. Init heap
    unsafe {
        ALLOCATOR.lock().init(HEAP_START as *mut u8, HEAP_SIZE);
    }

    // 2. Init architecture (GDT, IDT, PIC)
    let kernel_stack = HEAP_START as u64 + HEAP_SIZE as u64;
    watos_arch::init(kernel_stack);

    unsafe { watos_arch::serial_write(b"WATOS kernel started\r\n"); }

    // 3. Copy boot info
    unsafe {
        let boot_info = &*(BOOT_INFO_ADDR as *const BootInfo);
        if boot_info.magic != BOOT_MAGIC {
            watos_arch::serial_write(b"ERROR: Invalid boot magic\r\n");
            loop { watos_arch::halt(); }
        }
        BOOT_INFO = Some(*boot_info);

        watos_arch::serial_write(b"[KERNEL] Framebuffer: ");
        watos_arch::serial_hex(boot_info.framebuffer_width as u64);
        watos_arch::serial_write(b"x");
        watos_arch::serial_hex(boot_info.framebuffer_height as u64);
        watos_arch::serial_write(b"\r\n");
    }

    // 4. Install syscall handler
    watos_arch::idt::install_syscall_handler(syscall_handler);
    unsafe { watos_arch::serial_write(b"[KERNEL] Syscall handler installed\r\n"); }

    // 5. Initialize process subsystem
    watos_process::init();
    unsafe { watos_arch::serial_write(b"[KERNEL] Process subsystem initialized\r\n"); }

    // 6. Execute init app (TERM.EXE) if loaded by bootloader
    unsafe {
        if let Some(info) = BOOT_INFO {
            if info.init_app_addr != 0 && info.init_app_size != 0 {
                watos_arch::serial_write(b"[KERNEL] Launching TERM.EXE at 0x");
                watos_arch::serial_hex(info.init_app_addr);
                watos_arch::serial_write(b" (");
                watos_arch::serial_hex(info.init_app_size);
                watos_arch::serial_write(b" bytes)\r\n");

                // Create slice from the loaded app data
                let app_data = core::slice::from_raw_parts(
                    info.init_app_addr as *const u8,
                    info.init_app_size as usize,
                );

                // Execute the app
                match watos_process::exec("TERM.EXE", app_data) {
                    Ok(pid) => {
                        watos_arch::serial_write(b"[KERNEL] TERM.EXE running as PID ");
                        watos_arch::serial_hex(pid as u64);
                        watos_arch::serial_write(b"\r\n");
                    }
                    Err(e) => {
                        watos_arch::serial_write(b"[KERNEL] Failed to exec TERM.EXE: ");
                        watos_arch::serial_write(e.as_bytes());
                        watos_arch::serial_write(b"\r\n");
                    }
                }
            } else {
                watos_arch::serial_write(b"[KERNEL] No init app loaded\r\n");
            }
        }
    }

    // 7. Idle loop (should not reach here if TERM.EXE runs)
    unsafe { watos_arch::serial_write(b"[KERNEL] Entering idle loop\r\n"); }
    loop {
        watos_arch::halt();
    }
}

// ============================================================================
// Syscall Interface (numbers from watos-syscall crate)
// ============================================================================

/// Syscall numbers - must match watos_syscall::numbers
mod syscall {
    // Console/IO
    pub const SYS_WRITE: u64 = 1;
    pub const SYS_READ: u64 = 2;
    pub const SYS_GETKEY: u64 = 5;
    pub const SYS_EXIT: u64 = 6;

    // System
    pub const SYS_MALLOC: u64 = 14;
    pub const SYS_FREE: u64 = 15;

    // Framebuffer
    pub const SYS_FB_INFO: u64 = 50;
    pub const SYS_FB_ADDR: u64 = 51;
    pub const SYS_FB_DIMENSIONS: u64 = 52;

    // Raw keyboard
    pub const SYS_READ_SCANCODE: u64 = 60;

    // Process execution
    pub const SYS_EXEC: u64 = 80;

    // Date/Time
    pub const SYS_GETDATE: u64 = 90;
    pub const SYS_GETTIME: u64 = 91;
    pub const SYS_GETTICKS: u64 = 92;
}

/// Syscall handler - naked function called from IDT
///
/// When INT 0x80 is called from Ring 3:
/// - CPU pushes SS, RSP, RFLAGS, CS, RIP onto kernel stack
/// - We must save user registers, handle syscall, restore registers, IRETQ
#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn syscall_handler() {
    core::arch::naked_asm!(
        // Save all caller-saved registers (syscall may clobber them)
        // RAX contains syscall number, will be overwritten with result
        // RDI, RSI, RDX contain args 1-3
        "push rbx",
        "push rcx",
        "push rdx",
        "push rsi",
        "push rdi",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        "push rbp",

        // Call the inner handler
        // Args are already in correct registers: rdi=arg1, rsi=arg2, rdx=arg3
        // Move syscall number (rax) to rdi, shift others

        // Read interrupt frame values for parent context saving
        // Stack layout: 10 saved regs (80 bytes) then interrupt frame
        // Interrupt frame: RIP at +80, CS at +88, RFLAGS at +96, RSP at +104, SS at +112
        "mov r8, [rsp + 80]",   // RIP from interrupt frame -> r8 (5th param)
        "mov r9, [rsp + 104]",  // RSP from interrupt frame -> r9 (6th param)

        "mov rcx, rdx",     // arg3 -> rcx (4th param)
        "mov rdx, rsi",     // arg2 -> rdx (3rd param)
        "mov rsi, rdi",     // arg1 -> rsi (2nd param)
        "mov rdi, rax",     // syscall_num -> rdi (1st param)

        // Call Rust handler
        "call {handler}",

        // Result is in RAX - leave it there for user

        // Restore registers (except RAX which has result)
        "pop rbp",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop rcx",
        "pop rbx",

        // Return to user mode
        "iretq",

        handler = sym handle_syscall_inner,
    );
}

/// Inner syscall handler - called from naked handler
/// return_rip and return_rsp are from the interrupt frame for saving parent context
#[inline(never)]
extern "C" fn handle_syscall_inner(num: u64, arg1: u64, arg2: u64, arg3: u64, return_rip: u64, return_rsp: u64) -> u64 {
    handle_syscall(num, arg1, arg2, arg3, return_rip, return_rsp)
}

fn handle_syscall(num: u64, arg1: u64, arg2: u64, arg3: u64, return_rip: u64, return_rsp: u64) -> u64 {
    match num {
        syscall::SYS_EXIT => {
            unsafe { watos_arch::serial_write(b"[SYSCALL] Exit\r\n"); }
            // Check if there's a parent process to return to
            if watos_process::has_parent_context() {
                unsafe { watos_arch::serial_write(b"[SYSCALL] Returning to parent\r\n"); }
                watos_process::resume_parent(); // Never returns
            }
            loop { watos_arch::halt(); }
        }

        syscall::SYS_WRITE => {
            // arg1 = fd (ignored for now, always serial)
            // arg2 = pointer to string
            // arg3 = length
            let ptr = arg2 as *const u8;
            let len = arg3 as usize;
            unsafe {
                let slice = core::slice::from_raw_parts(ptr, len);
                watos_arch::serial_write(slice);
            }
            len as u64
        }

        syscall::SYS_READ => {
            // arg1 = fd (ignored), arg2 = buffer, arg3 = max_len
            // For now, just return 0 (no data)
            0
        }

        syscall::SYS_GETKEY => {
            // Returns ASCII key or 0 if no key (higher-level than scancode)
            watos_arch::idt::get_scancode().map(|s| s as u64).unwrap_or(0)
        }

        syscall::SYS_MALLOC => {
            // arg1 = size, returns pointer
            use alloc::alloc::{alloc, Layout};
            let size = arg1 as usize;
            if size == 0 {
                return 0;
            }
            unsafe {
                let layout = Layout::from_size_align(size, 8).unwrap();
                alloc(layout) as u64
            }
        }

        syscall::SYS_FREE => {
            // arg1 = pointer, arg2 = size
            use alloc::alloc::{dealloc, Layout};
            let ptr = arg1 as *mut u8;
            let size = arg2 as usize;
            if ptr.is_null() || size == 0 {
                return 0;
            }
            unsafe {
                let layout = Layout::from_size_align(size, 8).unwrap();
                dealloc(ptr, layout);
            }
            0
        }

        syscall::SYS_FB_INFO => {
            // Returns pointer to BootInfo struct
            unsafe {
                BOOT_INFO.as_ref().map(|b| b as *const _ as u64).unwrap_or(0)
            }
        }

        syscall::SYS_FB_ADDR => {
            // Returns framebuffer address
            unsafe {
                BOOT_INFO.map(|b| b.framebuffer_addr).unwrap_or(0)
            }
        }

        syscall::SYS_FB_DIMENSIONS => {
            // Returns width/height/pitch packed
            // Format: high 32 bits = width | mid 16 bits = height | low 16 bits = pitch/4
            unsafe {
                BOOT_INFO.map(|b| {
                    let w = b.framebuffer_width as u64;
                    let h = b.framebuffer_height as u64;
                    let p = (b.framebuffer_pitch / 4) as u64;
                    (w << 32) | (h << 16) | p
                }).unwrap_or(0)
            }
        }

        syscall::SYS_READ_SCANCODE => {
            // Returns raw PS/2 scancode or 0 if no key
            watos_arch::idt::get_scancode().map(|s| s as u64).unwrap_or(0)
        }

        syscall::SYS_EXEC => {
            // arg1 = pointer to program name string
            // arg2 = length of program name
            // Returns: 0 on success, non-zero on error
            let name_ptr = arg1 as *const u8;
            let name_len = arg2 as usize;

            if name_ptr.is_null() || name_len == 0 || name_len > 64 {
                return u64::MAX; // Invalid args
            }

            let name_slice = unsafe { core::slice::from_raw_parts(name_ptr, name_len) };

            unsafe {
                watos_arch::serial_write(b"[KERNEL] SYS_EXEC: ");
                watos_arch::serial_write(name_slice);
                watos_arch::serial_write(b"\r\n");
            }

            // Find the app in preloaded apps table
            if let Some((addr, size)) = find_preloaded_app(name_slice) {
                unsafe {
                    watos_arch::serial_write(b"[KERNEL] Found app at 0x");
                    watos_arch::serial_hex(addr);
                    watos_arch::serial_write(b" size=");
                    watos_arch::serial_hex(size);
                    watos_arch::serial_write(b"\r\n");
                }

                // Get the app data slice
                let app_data = unsafe {
                    core::slice::from_raw_parts(addr as *const u8, size as usize)
                };

                // Save parent context with actual return address so we can return after child exits
                watos_process::save_parent_context_with_frame(return_rip, return_rsp);

                // Execute the app (child will run, then SYS_EXIT will return here)
                match watos_process::exec(
                    core::str::from_utf8(name_slice).unwrap_or("app"),
                    app_data,
                ) {
                    Ok(_pid) => 0, // Success
                    Err(_) => {
                        unsafe { watos_arch::serial_write(b"[KERNEL] exec failed\r\n"); }
                        1 // Error
                    }
                }
            } else {
                unsafe {
                    watos_arch::serial_write(b"[KERNEL] App not found: ");
                    watos_arch::serial_write(name_slice);
                    watos_arch::serial_write(b"\r\n");
                }
                2 // Not found
            }
        }

        syscall::SYS_GETDATE => {
            // Returns packed date: year << 16 | month << 8 | day
            watos_arch::rtc::get_packed_date() as u64
        }

        syscall::SYS_GETTIME => {
            // Returns packed time: hours << 16 | minutes << 8 | seconds
            watos_arch::rtc::get_packed_time() as u64
        }

        syscall::SYS_GETTICKS => {
            // Returns timer ticks since boot
            watos_arch::idt::get_ticks()
        }

        _ => {
            unsafe {
                watos_arch::serial_write(b"[SYSCALL] Unknown: ");
                watos_arch::serial_hex(num);
                watos_arch::serial_write(b"\r\n");
            }
            u64::MAX // Error
        }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    unsafe {
        watos_arch::serial_write(b"\r\n!!! KERNEL PANIC !!!\r\n");
    }
    loop {
        watos_arch::halt();
    }
}
