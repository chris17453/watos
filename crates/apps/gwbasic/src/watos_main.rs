//! WATOS GW-BASIC Entry Point
//!
//! This is the main entry point for the GW-BASIC interpreter when running
//! as a native application on WATOS.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::panic::PanicInfo;
use rust_gwbasic::{Lexer, Parser, Interpreter};
use rust_gwbasic::platform::{WatosConsole, Console};

/// Entry point for WATOS executable
///
/// # Safety
/// This function is called by the WATOS kernel to start the program.
#[no_mangle]
pub extern "C" fn _start() -> ! {
    main();
    exit(0);
}

fn main() {
    // Initialize console handles first
    init_console_handles();
    
    let mut console = WatosConsole::new();

    console.print("GW-BASIC (Rust) v");
    console.print(rust_gwbasic::VERSION);
    console.print(" for WATOS\n");
    console.print("Type BASIC statements or 'EXIT' to quit\n\n");

    let mut interpreter = Interpreter::new();

    loop {
        console.print("> ");

        let input = console.read_line();
        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        if input.eq_ignore_ascii_case("EXIT") || input.eq_ignore_ascii_case("QUIT") {
            break;
        }

        // Try to tokenize, parse, and execute
        let mut lexer = Lexer::new(input);
        let tokens = match lexer.tokenize() {
            Ok(t) => t,
            Err(e) => {
                console.print("Lexer error: ");
                // Note: Display trait not available in no_std, using debug
                console.print(&alloc::format!("{:?}\n", e));
                continue;
            }
        };

        let mut parser = Parser::new(tokens);
        let ast = match parser.parse() {
            Ok(a) => a,
            Err(e) => {
                console.print("Parser error: ");
                console.print(&alloc::format!("{:?}\n", e));
                continue;
            }
        };

        if let Err(e) = interpreter.execute(ast) {
            console.print("Runtime error: ");
            console.print(&alloc::format!("{:?}\n", e));
        }
    }

    console.print("Goodbye!\n");
}

/// Exit the program
fn exit(code: i32) -> ! {
    rust_gwbasic::platform::exit(code);
}

/// Panic handler for WATOS
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // Try to print panic message
    let mut console = WatosConsole::new();
    console.print("PANIC: ");
    if let Some(location) = info.location() {
        console.print(location.file());
        console.print(":");
        console.print(&alloc::format!("{}", location.line()));
    }
    console.print("\n");

    exit(1);
}

/// Global allocator - simple bump allocator for WATOS
#[global_allocator]
static ALLOCATOR: WatosAllocator = WatosAllocator;

struct WatosAllocator;

// Static heap for the allocator
static mut HEAP: [u8; 2 * 1024 * 1024] = [0; 2 * 1024 * 1024]; // 2MB heap
static mut HEAP_POS: usize = 0;

unsafe impl alloc::alloc::GlobalAlloc for WatosAllocator {
    unsafe fn alloc(&self, layout: alloc::alloc::Layout) -> *mut u8 {
        let align = layout.align();
        let size = layout.size();

        // Align the position
        let aligned_pos = (HEAP_POS + align - 1) & !(align - 1);

        if aligned_pos + size > HEAP.len() {
            return core::ptr::null_mut();
        }

        HEAP_POS = aligned_pos + size;
        HEAP.as_mut_ptr().add(aligned_pos)
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: alloc::alloc::Layout) {
        // Simple bump allocator doesn't deallocate
    }
}

// Global console handles for the GWBASIC application
static mut STDOUT_HANDLE: Option<u64> = None;
static mut STDIN_HANDLE: Option<u64> = None;
static mut STDERR_HANDLE: Option<u64> = None;

/// Initialize console handles - should be called early
fn init_console_handles() {
    unsafe {
        // Request console handles from WATOS
        let stdout: u64;
        let stdin: u64;
        let stderr: u64;
        
        core::arch::asm!(
            "int 0x80",
            in("eax") 21u32, // SYS_CONSOLE_OUT
            lateout("rax") stdout,
            options(nostack)
        );
        
        core::arch::asm!(
            "int 0x80",
            in("eax") 20u32, // SYS_CONSOLE_IN
            lateout("rax") stdin,
            options(nostack)
        );
        
        core::arch::asm!(
            "int 0x80",
            in("eax") 22u32, // SYS_CONSOLE_ERR
            lateout("rax") stderr,
            options(nostack)
        );
        
        // Store handles if valid (errors are > 2^32)
        STDOUT_HANDLE = if stdout < 0x100000000 { Some(stdout) } else { None };
        STDIN_HANDLE = if stdin < 0x100000000 { Some(stdin) } else { None };
        STDERR_HANDLE = if stderr < 0x100000000 { Some(stderr) } else { None };
    }
}

/// Console write function expected by the gwbasic library
#[no_mangle]
pub extern "C" fn watos_console_write(buf: *const u8, len: usize) {
    if buf.is_null() || len == 0 {
        return;
    }
    
    unsafe {
        if let Some(handle) = STDOUT_HANDLE {
            // Use proper handle-based I/O
            core::arch::asm!(
                "int 0x80",
                in("eax") 1u32,     // SYS_WRITE
                in("rdi") handle,   // stdout handle
                in("rsi") buf,
                in("rdx") len,
                options(nostack)
            );
        }
        // Fallback: try direct putchar
        else {
            for i in 0..len {
                let ch = *buf.add(i);
                core::arch::asm!(
                    "int 0x80",
                    in("eax") 16u32,    // SYS_PUTCHAR
                    in("rdi") ch as u64,
                    options(nostack)
                );
            }
        }
    }
}

/// Console read function
#[no_mangle]
pub extern "C" fn watos_console_read(buf: *mut u8, max_len: usize) -> usize {
    if buf.is_null() || max_len == 0 {
        return 0;
    }
    
    unsafe {
        if let Some(handle) = STDIN_HANDLE {
            // Use proper handle-based I/O
            let result: u64;
            core::arch::asm!(
                "int 0x80",
                in("eax") 2u32,     // SYS_READ
                in("rdi") handle,   // stdin handle
                in("rsi") buf,
                in("rdx") max_len,
                lateout("rax") result,
                options(nostack)
            );
            result as usize
        } else {
            // Fallback: direct keyboard input
            let result: u64;
            core::arch::asm!(
                "int 0x80",
                in("eax") 5u32,     // SYS_GETKEY
                lateout("rax") result,
                options(nostack)
            );
            if result > 0 && max_len > 0 {
                *buf = result as u8;
                1
            } else {
                0
            }
        }
    }
}

/// File open syscall - delegates to kernel via INT 0x80
#[no_mangle]
pub extern "C" fn watos_file_open(path: *const u8, len: usize, mode: u64) -> i64 {
    if path.is_null() || len == 0 {
        return -1;
    }
    unsafe {
        let result: u64;
        core::arch::asm!(
            "int 0x80",
            in("eax") 3u32,     // SYS_OPEN
            in("rdi") path,
            in("rsi") len,
            in("rdx") mode,
            lateout("rax") result,
            options(nostack)
        );
        // Check for error (WATOS returns error codes > 2^32 for failures)
        if result > 0x100000000 {
            -1  // Return error
        } else {
            result as i64  // Return valid handle
        }
    }
}

/// File close syscall - delegates to kernel via INT 0x80
#[no_mangle]
pub extern "C" fn watos_file_close(handle: i64) {
    unsafe {
        core::arch::asm!(
            "int 0x80",
            in("eax") 4u32,     // SYS_CLOSE
            in("rdi") handle,
            options(nostack)
        );
    }
}

/// File read syscall - delegates to kernel via INT 0x80
#[no_mangle]
pub extern "C" fn watos_file_read(handle: i64, buf: *mut u8, len: usize) -> usize {
    if buf.is_null() || len == 0 {
        return 0;
    }
    unsafe {
        let result: usize;
        core::arch::asm!(
            "int 0x80",
            in("eax") 2u32,     // SYS_READ (can also handle files)
            in("rdi") handle,
            in("rsi") buf,
            in("rdx") len,
            lateout("rax") result,
            options(nostack)
        );
        result
    }
}

/// File write syscall - delegates to kernel via INT 0x80
#[no_mangle]
pub extern "C" fn watos_file_write(handle: i64, buf: *const u8, len: usize) -> usize {
    if buf.is_null() || len == 0 {
        return 0;
    }
    unsafe {
        let result: usize;
        core::arch::asm!(
            "int 0x80",
            in("eax") 1u32,     // SYS_WRITE (can handle both console and files)
            in("rdi") handle,
            in("rsi") buf,
            in("rdx") len,
            lateout("rax") result,
            options(nostack)
        );
        result
    }
}

/// Get free memory
#[no_mangle]
pub extern "C" fn watos_get_free_memory() -> usize {
    // Return remaining heap space
    unsafe {
        HEAP.len().saturating_sub(HEAP_POS)
    }
}

/// Get cursor column
#[no_mangle]
pub extern "C" fn watos_get_cursor_col() -> u8 {
    0 // Placeholder
}

/// Get cursor row
#[no_mangle]
pub extern "C" fn watos_get_cursor_row() -> u8 {
    0 // Placeholder
}

/// Get current date (year, month, day)
#[no_mangle]
pub extern "C" fn watos_get_date() -> (u16, u8, u8) {
    (2025, 1, 1) // Placeholder
}

/// Get current time (hour, minute, second)
#[no_mangle]
pub extern "C" fn watos_get_time() -> (u8, u8, u8) {
    (12, 0, 0) // Placeholder
}

/// Get key without waiting (returns 0 if no key)
#[no_mangle]
pub extern "C" fn watos_get_key_no_wait() -> u8 {
    unsafe {
        let result: u64;
        core::arch::asm!(
            "int 0x80",
            in("eax") 5u32,     // SYS_GETKEY
            lateout("rax") result,
            options(nostack)
        );
        result as u8
    }
}

/// Get pixel at position
#[no_mangle]
pub extern "C" fn watos_get_pixel(x: i32, y: i32) -> u8 {
    // Use SYS_VGA_GET_PIXEL syscall (32)
    unsafe {
        let result: u8;
        core::arch::asm!(
            "int 0x80",
            in("eax") 32u32,  // SYS_VGA_GET_PIXEL
            in("rdi") x as i64,
            in("rsi") y as i64,
            lateout("al") result,
            options(nostack)
        );
        result
    }
}

/// Get timer value
#[no_mangle]
pub extern "C" fn watos_timer_syscall() -> u64 {
    0 // Placeholder
}
