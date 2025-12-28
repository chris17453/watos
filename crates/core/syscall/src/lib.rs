//! WATOS System Call Interface
//!
//! This crate provides the canonical system call definitions and wrapper functions
//! for WATOS. It's used by both the kernel and user applications to ensure
//! consistency in the syscall ABI.

#![cfg_attr(feature = "no-std", no_std)]

/// WATOS System Call Numbers - AUTHORITATIVE DEFINITION
pub mod numbers {
    // Console/IO
    pub const SYS_WRITE: u32 = 1;
    pub const SYS_READ: u32 = 2;
    pub const SYS_OPEN: u32 = 3;
    pub const SYS_CLOSE: u32 = 4;
    pub const SYS_GETKEY: u32 = 5;
    pub const SYS_EXIT: u32 = 6;

    // System
    pub const SYS_SLEEP: u32 = 11;
    pub const SYS_GETPID: u32 = 12;
    pub const SYS_TIME: u32 = 13;
    pub const SYS_TIMER: u32 = SYS_TIME; // Alias for SYS_TIME
    pub const SYS_MALLOC: u32 = 14;
    pub const SYS_FREE: u32 = 15;

    // Additional syscalls
    pub const SYS_PUTCHAR: u32 = 16;
    pub const SYS_CURSOR: u32 = 17;
    pub const SYS_CLEAR: u32 = 18;
    pub const SYS_COLOR: u32 = 19;
    
    // Console handle management
    pub const SYS_CONSOLE_IN: u32 = 20;    // Get stdin handle
    pub const SYS_CONSOLE_OUT: u32 = 21;   // Get stdout handle
    pub const SYS_CONSOLE_ERR: u32 = 22;   // Get stderr handle
    pub const SYS_CONSOLE_READ: u32 = 23;  // Read from kernel console buffer (for terminal app)

    // VGA Graphics
    pub const SYS_VGA_SET_MODE: u32 = 30;
    pub const SYS_VGA_SET_PIXEL: u32 = 31;
    pub const SYS_VGA_GET_PIXEL: u32 = 32;
    pub const SYS_VGA_BLIT: u32 = 33;
    pub const SYS_VGA_CLEAR: u32 = 34;
    pub const SYS_VGA_FLIP: u32 = 35;
    pub const SYS_VGA_SET_PALETTE: u32 = 36;
    
    // Graphics extensions
    pub const SYS_GFX_PSET: u32 = 40;      // Set pixel
    pub const SYS_GFX_LINE: u32 = 41;      // Draw line
    pub const SYS_GFX_CIRCLE: u32 = 42;    // Draw circle
    pub const SYS_GFX_CLS: u32 = 43;       // Clear graphics screen
    pub const SYS_GFX_MODE: u32 = 44;      // Set graphics mode
    pub const SYS_GFX_DISPLAY: u32 = 45;   // Display graphics buffer

    // Framebuffer (for direct UEFI GOP access)
    pub const SYS_FB_INFO: u32 = 50;       // Get framebuffer info (returns BootInfo ptr)
    pub const SYS_FB_ADDR: u32 = 51;       // Get framebuffer address
    pub const SYS_FB_DIMENSIONS: u32 = 52; // Get width/height/pitch

    // Raw keyboard (PS/2 scancodes)
    pub const SYS_READ_SCANCODE: u32 = 60; // Read raw keyboard scancode (non-blocking)

    // Filesystem operations
    pub const SYS_STAT: u32 = 70;          // Get file/directory info
    pub const SYS_READDIR: u32 = 71;       // Read directory entries
    pub const SYS_MKDIR: u32 = 72;         // Create directory
    pub const SYS_UNLINK: u32 = 73;        // Delete file
    pub const SYS_RMDIR: u32 = 74;         // Remove directory
    pub const SYS_RENAME: u32 = 75;        // Rename file/directory
    pub const SYS_GETCWD: u32 = 76;        // Get current working directory
    pub const SYS_CHDIR: u32 = 77;         // Change current directory
    pub const SYS_MOUNT: u32 = 78;         // Mount drive (drive_name, device_id, fs_type)
    pub const SYS_UNMOUNT: u32 = 79;       // Unmount drive (drive_name)
    pub const SYS_LISTDRIVES: u32 = 85;    // List mounted drives

    // Process execution
    pub const SYS_EXEC: u32 = 80;          // Execute program (replace current process)
    pub const SYS_SPAWN: u32 = 81;         // Spawn new process
    pub const SYS_WAIT: u32 = 82;          // Wait for child process
    pub const SYS_GETARGS: u32 = 83;       // Get command line arguments (copies to buffer)

    // Date/Time
    pub const SYS_GETDATE: u32 = 90;       // Get current date (year, month, day)
    pub const SYS_GETTIME: u32 = 91;       // Get current time (hour, min, sec)
    pub const SYS_GETTICKS: u32 = 92;      // Get system ticks since boot
}

/// Raw syscall interface - performs INT 0x80
///
/// # Safety
/// This function performs a raw system call. The caller must ensure:
/// - The syscall number is valid
/// - Arguments match the expected types for the syscall
/// - Pointers (if any) are valid and point to accessible memory
#[inline(always)]
pub unsafe fn raw_syscall0(num: u32) -> u64 {
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
pub unsafe fn raw_syscall1(num: u32, arg1: u64) -> u64 {
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
pub unsafe fn raw_syscall2(num: u32, arg1: u64, arg2: u64) -> u64 {
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
pub unsafe fn raw_syscall3(num: u32, arg1: u64, arg2: u64, arg3: u64) -> u64 {
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

/// High-level syscall wrappers
pub mod syscalls {
    use super::{numbers::*, raw_syscall0, raw_syscall1, raw_syscall2, raw_syscall3};

    /// Exit the current process
    pub fn exit(code: i32) -> ! {
        unsafe {
            raw_syscall1(SYS_EXIT, code as u64);
        }
        loop {}
    }

    /// Write data to a file descriptor
    pub fn write(fd: i32, buf: &[u8]) -> usize {
        unsafe {
            raw_syscall3(SYS_WRITE, fd as u64, buf.as_ptr() as u64, buf.len() as u64) as usize
        }
    }

    /// Read data from a file descriptor
    pub fn read(fd: i32, buf: &mut [u8]) -> usize {
        unsafe {
            raw_syscall3(SYS_READ, fd as u64, buf.as_mut_ptr() as u64, buf.len() as u64) as usize
        }
    }

    /// Open a file
    pub fn open(path: &str, mode: u32) -> i32 {
        unsafe {
            raw_syscall3(SYS_OPEN, path.as_ptr() as u64, path.len() as u64, mode as u64) as i32
        }
    }

    /// Close a file
    pub fn close(fd: i32) -> i32 {
        unsafe {
            raw_syscall1(SYS_CLOSE, fd as u64) as i32
        }
    }

    /// Get a key without blocking
    pub fn getkey() -> u8 {
        unsafe {
            raw_syscall0(SYS_GETKEY) as u8
        }
    }

    /// Sleep for milliseconds
    pub fn sleep(ms: u32) {
        unsafe {
            raw_syscall1(SYS_SLEEP, ms as u64);
        }
    }

    /// Get current process ID
    pub fn getpid() -> u32 {
        unsafe {
            raw_syscall0(SYS_GETPID) as u32
        }
    }

    /// Get system time (ticks since boot)
    pub fn time() -> u64 {
        unsafe {
            raw_syscall0(SYS_TIME)
        }
    }

    /// Allocate memory
    pub fn malloc(size: usize) -> *mut u8 {
        unsafe {
            raw_syscall1(SYS_MALLOC, size as u64) as *mut u8
        }
    }

    /// Free memory
    pub fn free(ptr: *mut u8) {
        unsafe {
            raw_syscall1(SYS_FREE, ptr as u64);
        }
    }

    /// Output a single character
    pub fn putchar(ch: char) {
        unsafe {
            raw_syscall1(SYS_PUTCHAR, ch as u64);
        }
    }

    /// Set cursor position
    pub fn set_cursor(x: u32, y: u32) {
        unsafe {
            raw_syscall2(SYS_CURSOR, x as u64, y as u64);
        }
    }

    /// Clear screen
    pub fn clear_screen() {
        unsafe {
            raw_syscall0(SYS_CLEAR);
        }
    }

    /// Set text color
    pub fn set_color(color: u32) {
        unsafe {
            raw_syscall1(SYS_COLOR, color as u64);
        }
    }

    /// Set VGA graphics mode
    pub fn vga_set_mode(mode: u8) -> u64 {
        unsafe {
            raw_syscall1(SYS_VGA_SET_MODE, mode as u64)
        }
    }

    /// Set pixel in graphics mode
    pub fn vga_set_pixel(x: i32, y: i32, color: u8) {
        unsafe {
            raw_syscall3(SYS_VGA_SET_PIXEL, x as u64, y as u64, color as u64);
        }
    }

    /// Get pixel from graphics mode
    pub fn vga_get_pixel(x: i32, y: i32) -> u8 {
        unsafe {
            raw_syscall2(SYS_VGA_GET_PIXEL, x as u64, y as u64) as u8
        }
    }

    /// Get framebuffer info pointer (returns pointer to BootInfo struct)
    pub fn fb_info() -> u64 {
        unsafe {
            raw_syscall0(SYS_FB_INFO)
        }
    }

    /// Get framebuffer address
    pub fn fb_addr() -> u64 {
        unsafe {
            raw_syscall0(SYS_FB_ADDR)
        }
    }

    /// Get framebuffer dimensions: returns (width, height, pitch) packed
    /// Format: high 32 bits = width | mid 16 bits = height | low 16 bits = pitch/4
    pub fn fb_dimensions() -> (u32, u32, u32) {
        unsafe {
            let packed = raw_syscall0(SYS_FB_DIMENSIONS);
            let width = (packed >> 32) as u32;
            let height = ((packed >> 16) & 0xFFFF) as u32;
            let pitch = (packed & 0xFFFF) as u32 * 4; // pitch stored as /4
            (width, height, pitch)
        }
    }

    /// Read raw keyboard scancode (non-blocking, returns 0 if no key)
    pub fn read_scancode() -> u8 {
        unsafe {
            raw_syscall0(SYS_READ_SCANCODE) as u8
        }
    }

    /// Get current date (year, month, day)
    pub fn get_date() -> (u16, u8, u8) {
        let packed = unsafe { raw_syscall0(SYS_GETDATE) } as u32;
        let year = (packed >> 16) as u16;
        let month = ((packed >> 8) & 0xFF) as u8;
        let day = (packed & 0xFF) as u8;
        (year, month, day)
    }

    /// Get current time (hours, minutes, seconds)
    pub fn get_time() -> (u8, u8, u8) {
        let packed = unsafe { raw_syscall0(SYS_GETTIME) } as u32;
        let hours = ((packed >> 16) & 0xFF) as u8;
        let minutes = ((packed >> 8) & 0xFF) as u8;
        let seconds = (packed & 0xFF) as u8;
        (hours, minutes, seconds)
    }

    /// Get system ticks since boot
    pub fn get_ticks() -> u64 {
        unsafe { raw_syscall0(SYS_GETTICKS) }
    }

    /// Execute a program by name
    /// Returns 0 on success, non-zero on error
    /// Error codes:
    ///   1 = exec failed
    ///   2 = program not found
    ///   u64::MAX = invalid arguments
    pub fn exec(name: &str) -> u64 {
        unsafe {
            raw_syscall2(SYS_EXEC, name.as_ptr() as u64, name.len() as u64)
        }
    }

    /// Mount a drive with a given name
    /// name: Drive name (e.g., "C", "D", "MYDATA")
    /// mount_path: Null-terminated mount path (e.g., "/mnt/c\0")
    /// Returns 0 on success, error code on failure
    pub fn mount(name: &str, mount_path: &[u8]) -> u64 {
        unsafe {
            raw_syscall3(
                SYS_MOUNT,
                name.as_ptr() as u64,
                name.len() as u64,
                mount_path.as_ptr() as u64,
            )
        }
    }

    /// Unmount a drive by name
    /// Returns 0 on success, error code on failure
    pub fn unmount(name: &str) -> u64 {
        unsafe {
            raw_syscall2(SYS_UNMOUNT, name.as_ptr() as u64, name.len() as u64)
        }
    }

    /// List mounted drives
    /// Format: "NAME:PATH:FSTYPE\n" for each drive, * marks current drive
    /// Returns bytes written to buffer
    pub fn list_drives(buf: &mut [u8]) -> usize {
        unsafe {
            raw_syscall2(SYS_LISTDRIVES, buf.as_mut_ptr() as u64, buf.len() as u64) as usize
        }
    }

    /// Change current drive/directory
    /// If path ends with ':', changes drive (e.g., "D:")
    /// Otherwise changes directory (not yet implemented)
    /// Returns 0 on success
    pub fn chdir(path: &str) -> u64 {
        unsafe {
            raw_syscall2(SYS_CHDIR, path.as_ptr() as u64, path.len() as u64)
        }
    }

    /// Get current working directory
    /// Returns bytes written (e.g., "C:\path")
    pub fn getcwd(buf: &mut [u8]) -> usize {
        unsafe {
            raw_syscall2(SYS_GETCWD, buf.as_mut_ptr() as u64, buf.len() as u64) as usize
        }
    }

    /// Read directory entries
    /// path: directory path (empty for current directory)
    /// buf: output buffer for entries (format: "TYPE NAME SIZE\n" per entry)
    /// Returns bytes written
    pub fn readdir(path: &str, buf: &mut [u8]) -> usize {
        unsafe {
            raw_syscall3(
                SYS_READDIR,
                path.as_ptr() as u64,
                path.len() as u64,
                buf.as_mut_ptr() as u64,
            ) as usize
        }
    }

    /// Create a directory
    /// Returns 0 on success
    pub fn mkdir(path: &str) -> u64 {
        unsafe {
            raw_syscall2(SYS_MKDIR, path.as_ptr() as u64, path.len() as u64)
        }
    }

    /// Get file/directory status
    /// Returns (type, size) where type: 0=file, 1=directory
    pub fn stat(path: &str) -> Option<(u64, u64)> {
        let mut stat_buf: [u64; 2] = [0; 2];
        let result = unsafe {
            raw_syscall3(
                SYS_STAT,
                path.as_ptr() as u64,
                path.len() as u64,
                stat_buf.as_mut_ptr() as u64,
            )
        };
        if result == 0 {
            Some((stat_buf[0], stat_buf[1]))
        } else {
            None
        }
    }
}

/// Convenience functions for common operations
pub mod io {
    use super::syscalls;

    /// Print a string to stdout
    pub fn print(s: &str) {
        syscalls::write(1, s.as_bytes());
    }

    /// Print a string with newline
    pub fn println(s: &str) {
        print(s);
        print("\n");
    }

    /// Read a single character from keyboard
    pub fn getch() -> char {
        loop {
            let key = syscalls::getkey();
            if key != 0 {
                return key as char;
            }
            // Small delay to avoid busy waiting
            syscalls::sleep(1);
        }
    }
}