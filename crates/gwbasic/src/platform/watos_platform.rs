//! WATOS bare-metal platform implementation (no_std)
//!
//! This module provides platform abstractions for running on WATOS.
//! It interfaces with the WATOS kernel through a syscall-like interface.

extern crate alloc;

use super::{Console, FileSystem, Graphics, System, FileOpenMode, FileHandle};
use alloc::string::{String, ToString};
use alloc::collections::BTreeMap;

/// WATOS syscall numbers
pub mod syscall {
    pub const SYS_EXIT: u32 = 0;
    pub const SYS_WRITE: u32 = 1;
    pub const SYS_READ: u32 = 2;
    pub const SYS_OPEN: u32 = 3;
    pub const SYS_CLOSE: u32 = 4;
    pub const SYS_GETKEY: u32 = 5;
    pub const SYS_PUTCHAR: u32 = 6;
    pub const SYS_CURSOR: u32 = 7;
    pub const SYS_CLEAR: u32 = 8;
    pub const SYS_COLOR: u32 = 9;
    pub const SYS_TIMER: u32 = 10;
    pub const SYS_SLEEP: u32 = 11;
    pub const SYS_GFX_PSET: u32 = 20;
    pub const SYS_GFX_LINE: u32 = 21;
    pub const SYS_GFX_CIRCLE: u32 = 22;
    pub const SYS_GFX_CLS: u32 = 23;
    pub const SYS_GFX_MODE: u32 = 24;
    pub const SYS_GFX_DISPLAY: u32 = 25;
}

/// Make a syscall to WATOS kernel
///
/// # Safety
/// This function performs a raw syscall into the WATOS kernel.
#[inline(always)]
unsafe fn syscall0(num: u32) -> u64 {
    let ret: u64;
    core::arch::asm!(
        "int 0x80",
        in("eax") num,
        lateout("rax") ret,
        options(nostack)
    );
    ret
}

#[inline(always)]
unsafe fn syscall1(num: u32, arg1: u64) -> u64 {
    let ret: u64;
    core::arch::asm!(
        "int 0x80",
        in("eax") num,
        in("rdi") arg1,
        lateout("rax") ret,
        options(nostack)
    );
    ret
}

#[inline(always)]
unsafe fn syscall2(num: u32, arg1: u64, arg2: u64) -> u64 {
    let ret: u64;
    core::arch::asm!(
        "int 0x80",
        in("eax") num,
        in("rdi") arg1,
        in("rsi") arg2,
        lateout("rax") ret,
        options(nostack)
    );
    ret
}

#[inline(always)]
unsafe fn syscall3(num: u32, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    let ret: u64;
    core::arch::asm!(
        "int 0x80",
        in("eax") num,
        in("rdi") arg1,
        in("rsi") arg2,
        in("rdx") arg3,
        lateout("rax") ret,
        options(nostack)
    );
    ret
}

#[inline(always)]
#[allow(dead_code)]
unsafe fn syscall4(num: u32, arg1: u64, arg2: u64, arg3: u64, arg4: u64) -> u64 {
    let ret: u64;
    core::arch::asm!(
        "int 0x80",
        in("eax") num,
        in("rdi") arg1,
        in("rsi") arg2,
        in("rdx") arg3,
        in("r10") arg4,
        lateout("rax") ret,
        options(nostack)
    );
    ret
}

#[inline(always)]
#[allow(dead_code)]
unsafe fn syscall5(num: u32, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> u64 {
    let ret: u64;
    core::arch::asm!(
        "int 0x80",
        in("eax") num,
        in("rdi") arg1,
        in("rsi") arg2,
        in("rdx") arg3,
        in("r10") arg4,
        in("r8") arg5,
        lateout("rax") ret,
        options(nostack)
    );
    ret
}

/// WATOS Console implementation
pub struct WatosConsole {
    cursor_row: usize,
    cursor_col: usize,
    input_buffer: [u8; 256],
    input_len: usize,
}

impl WatosConsole {
    pub fn new() -> Self {
        WatosConsole {
            cursor_row: 0,
            cursor_col: 0,
            input_buffer: [0; 256],
            input_len: 0,
        }
    }
}

impl Default for WatosConsole {
    fn default() -> Self {
        Self::new()
    }
}

impl Console for WatosConsole {
    fn print(&mut self, s: &str) {
        let bytes = s.as_bytes();
        unsafe {
            syscall2(syscall::SYS_WRITE, bytes.as_ptr() as u64, bytes.len() as u64);
        }
    }

    fn print_char(&mut self, ch: char) {
        unsafe {
            syscall1(syscall::SYS_PUTCHAR, ch as u64);
        }
    }

    fn read_line(&mut self) -> String {
        self.input_len = 0;
        loop {
            let key = unsafe { syscall0(syscall::SYS_GETKEY) } as u8;
            if key == 0 {
                continue;
            }
            if key == b'\r' || key == b'\n' {
                break;
            }
            if key == 0x08 { // Backspace
                if self.input_len > 0 {
                    self.input_len -= 1;
                    self.print_char('\x08');
                    self.print_char(' ');
                    self.print_char('\x08');
                }
                continue;
            }
            if self.input_len < 255 {
                self.input_buffer[self.input_len] = key;
                self.input_len += 1;
                self.print_char(key as char);
            }
        }
        self.print_char('\n');
        // Convert buffer to string
        let slice = &self.input_buffer[..self.input_len];
        String::from_utf8_lossy(slice).into_owned()
    }

    fn read_char(&mut self) -> Option<char> {
        let key = unsafe { syscall0(syscall::SYS_GETKEY) } as u8;
        if key == 0 {
            None
        } else {
            Some(key as char)
        }
    }

    fn clear(&mut self) {
        unsafe {
            syscall0(syscall::SYS_CLEAR);
        }
        self.cursor_row = 0;
        self.cursor_col = 0;
    }

    fn set_cursor(&mut self, row: usize, col: usize) {
        unsafe {
            syscall2(syscall::SYS_CURSOR, row as u64, col as u64);
        }
        self.cursor_row = row;
        self.cursor_col = col;
    }

    fn get_cursor(&self) -> (usize, usize) {
        (self.cursor_row, self.cursor_col)
    }

    fn set_color(&mut self, fg: u8, bg: u8) {
        unsafe {
            syscall2(syscall::SYS_COLOR, fg as u64, bg as u64);
        }
    }
}

/// WATOS File System implementation
pub struct WatosFileSystem {
    open_files: BTreeMap<i32, u64>, // handle -> kernel handle
    next_handle: i32,
}

impl WatosFileSystem {
    pub fn new() -> Self {
        WatosFileSystem {
            open_files: BTreeMap::new(),
            next_handle: 1,
        }
    }
}

impl Default for WatosFileSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl FileSystem for WatosFileSystem {
    fn open(&mut self, path: &str, mode: FileOpenMode) -> Result<FileHandle, &'static str> {
        let mode_num = match mode {
            FileOpenMode::Input => 0,
            FileOpenMode::Output => 1,
            FileOpenMode::Append => 2,
            FileOpenMode::Random => 3,
        };
        let bytes = path.as_bytes();
        let kernel_handle = unsafe {
            syscall3(syscall::SYS_OPEN, bytes.as_ptr() as u64, bytes.len() as u64, mode_num)
        };
        if kernel_handle == u64::MAX {
            return Err("Cannot open file");
        }
        let handle = self.next_handle;
        self.next_handle += 1;
        self.open_files.insert(handle, kernel_handle);
        Ok(FileHandle(handle))
    }

    fn close(&mut self, handle: FileHandle) -> Result<(), &'static str> {
        if let Some(kernel_handle) = self.open_files.remove(&handle.0) {
            unsafe {
                syscall1(syscall::SYS_CLOSE, kernel_handle);
            }
            Ok(())
        } else {
            Err("File not open")
        }
    }

    fn read_line(&mut self, handle: FileHandle) -> Result<String, &'static str> {
        if let Some(&kernel_handle) = self.open_files.get(&handle.0) {
            let mut buffer = [0u8; 256];
            let len = unsafe {
                syscall3(syscall::SYS_READ, kernel_handle, buffer.as_mut_ptr() as u64, 256)
            } as usize;
            if len == 0 {
                return Err("EOF");
            }
            Ok(String::from_utf8_lossy(&buffer[..len]).trim_end().to_string())
        } else {
            Err("File not open")
        }
    }

    fn write_line(&mut self, handle: FileHandle, data: &str) -> Result<(), &'static str> {
        if let Some(&kernel_handle) = self.open_files.get(&handle.0) {
            let bytes = data.as_bytes();
            unsafe {
                syscall3(syscall::SYS_WRITE, kernel_handle, bytes.as_ptr() as u64, bytes.len() as u64);
            }
            // Write newline
            unsafe {
                syscall3(syscall::SYS_WRITE, kernel_handle, b"\n".as_ptr() as u64, 1);
            }
            Ok(())
        } else {
            Err("File not open")
        }
    }

    fn eof(&self, handle: FileHandle) -> bool {
        !self.open_files.contains_key(&handle.0)
    }
}

/// WATOS Graphics implementation
pub struct WatosGraphics {
    width: usize,
    height: usize,
}

impl WatosGraphics {
    pub fn new() -> Self {
        WatosGraphics {
            width: 320,
            height: 200,
        }
    }
}

impl Default for WatosGraphics {
    fn default() -> Self {
        Self::new()
    }
}

impl Graphics for WatosGraphics {
    fn pset(&mut self, x: i32, y: i32, color: u8) {
        unsafe {
            syscall3(syscall::SYS_GFX_PSET, x as u64, y as u64, color as u64);
        }
    }

    fn line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: u8) {
        unsafe {
            syscall5(syscall::SYS_GFX_LINE, x1 as u64, y1 as u64, x2 as u64, y2 as u64, color as u64);
        }
    }

    fn circle(&mut self, x: i32, y: i32, radius: i32, color: u8) {
        unsafe {
            syscall4(syscall::SYS_GFX_CIRCLE, x as u64, y as u64, radius as u64, color as u64);
        }
    }

    fn cls(&mut self) {
        unsafe {
            syscall0(syscall::SYS_GFX_CLS);
        }
    }

    fn set_mode(&mut self, mode: u8) {
        let (w, h) = match mode {
            1 => (320, 200),
            2 => (640, 200),
            _ => (80, 25),
        };
        self.width = w;
        self.height = h;
        unsafe {
            syscall1(syscall::SYS_GFX_MODE, mode as u64);
        }
    }

    fn get_size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    fn display(&mut self) {
        unsafe {
            syscall0(syscall::SYS_GFX_DISPLAY);
        }
    }
}

/// WATOS System implementation
pub struct WatosSystem {
    rng_state: u64,
}

impl WatosSystem {
    pub fn new() -> Self {
        WatosSystem {
            rng_state: 12345,
        }
    }
}

impl Default for WatosSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl System for WatosSystem {
    fn timer(&self) -> f32 {
        let ticks = unsafe { syscall0(syscall::SYS_TIMER) };
        // Assuming WATOS returns timer ticks, convert to seconds since midnight
        (ticks as f32) / 18.2 // DOS-like timer frequency
    }

    fn sleep(&self, ms: u32) {
        unsafe {
            syscall1(syscall::SYS_SLEEP, ms as u64);
        }
    }

    fn random(&mut self, seed: Option<i32>) -> f32 {
        if let Some(sv) = seed {
            if sv < 0 {
                self.rng_state = (sv.abs() as u64) * 1000;
            } else if sv == 0 {
                return (self.rng_state % 1000) as f32 / 1000.0;
            }
        }
        // Simple LCG
        self.rng_state = (self.rng_state.wrapping_mul(1103515245).wrapping_add(12345)) & 0x7fffffff;
        (self.rng_state % 1000000) as f32 / 1000000.0
    }
}

/// Exit the program
pub fn exit(code: i32) -> ! {
    unsafe {
        syscall1(syscall::SYS_EXIT, code as u64);
    }
    loop {}
}

/// Get environment variable (not available on WATOS)
pub fn get_env(_name: &str) -> Option<String> {
    None
}

/// Get command line arguments (would need kernel support)
pub fn get_args() -> alloc::vec::Vec<String> {
    alloc::vec::Vec::new()
}
