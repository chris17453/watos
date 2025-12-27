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

/// Console write function expected by the gwbasic library
#[no_mangle]
pub extern "C" fn watos_console_write(buf: *const u8, len: usize) {
    if buf.is_null() || len == 0 {
        return;
    }
    unsafe {
        // SYS_WRITE syscall via INT 0x80
        core::arch::asm!(
            "int 0x80",
            in("eax") 1u32,     // SYS_WRITE
            in("rdi") 1u64,    // stdout fd (ignored, just writes to console)
            in("rsi") buf,
            in("rdx") len,
            options(nostack)
        );
    }
}

/// Console read function
#[no_mangle]
pub extern "C" fn watos_console_read(buf: *mut u8, max_len: usize) -> usize {
    if buf.is_null() || max_len == 0 {
        return 0;
    }
    unsafe {
        let result: u64;
        core::arch::asm!(
            "int 0x80",
            in("eax") 2u32,     // SYS_READ
            in("rdi") 0u64,    // stdin fd
            in("rsi") buf,
            in("rdx") max_len,
            lateout("rax") result,
            options(nostack)
        );
        result as usize
    }
}

/// File open syscall stub
#[no_mangle]
pub extern "C" fn watos_file_open(_path: *const u8, _len: usize, _mode: u64) -> i64 {
    -1 // Not implemented - return error
}

/// File close syscall stub
#[no_mangle]
pub extern "C" fn watos_file_close(_handle: i64) {
    // Not implemented
}

/// File read syscall stub
#[no_mangle]
pub extern "C" fn watos_file_read(_handle: i64, _buf: *mut u8, _len: usize) -> usize {
    0 // Not implemented
}

/// File write syscall stub
#[no_mangle]
pub extern "C" fn watos_file_write(_handle: i64, _buf: *const u8, _len: usize) -> usize {
    0 // Not implemented
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
