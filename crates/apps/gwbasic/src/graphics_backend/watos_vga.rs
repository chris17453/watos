//! WATOS VGA/SVGA Graphics Backend
//!
//! Implements pixel-buffer graphics for WATOS using kernel SVGA driver via sessions.
//! Supports multiple video modes including:
//! - Mode 0: 80x25 text mode (handled by console)
//! - Mode 1: 320x200 VGA (256 colors)
//! - Mode 2: 640x200 VGA (16 colors)
//! - Mode 3: 640x480 VGA (16 colors)
//! - Mode 4: 800x600 SVGA (256 colors)
//! - Mode 5: 1024x768 SVGA (256 colors)

#![cfg(not(feature = "std"))]

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use alloc::format;
use crate::error::{Error, Result};
use crate::graphics_backend::GraphicsBackend;

/// Color palette for VGA (EGA/CGA compatible 16 colors)
pub const PALETTE_16: [u32; 16] = [
    0x000000, // 0: Black
    0x0000AA, // 1: Blue
    0x00AA00, // 2: Green
    0x00AAAA, // 3: Cyan
    0xAA0000, // 4: Red
    0xAA00AA, // 5: Magenta
    0xAA5500, // 6: Brown
    0xAAAAAA, // 7: Light Gray
    0x555555, // 8: Dark Gray
    0x5555FF, // 9: Light Blue
    0x55FF55, // 10: Light Green
    0x55FFFF, // 11: Light Cyan
    0xFF5555, // 12: Light Red
    0xFF55FF, // 13: Light Magenta
    0xFFFF55, // 14: Yellow
    0xFFFFFF, // 15: White
];

/// Video mode configuration
#[derive(Debug, Clone, Copy)]
pub struct VideoMode {
    pub width: usize,
    pub height: usize,
    pub bpp: u8,        // Bits per pixel (4, 8, 16, 24, 32)
    pub mode_num: u8,   // Mode number for syscall
}

impl VideoMode {
    pub const TEXT_80X25: Self = VideoMode { width: 80, height: 25, bpp: 0, mode_num: 0 };
    pub const VGA_320X200: Self = VideoMode { width: 320, height: 200, bpp: 8, mode_num: 1 };
    pub const VGA_640X200: Self = VideoMode { width: 640, height: 200, bpp: 4, mode_num: 2 };
    pub const VGA_640X480: Self = VideoMode { width: 640, height: 480, bpp: 4, mode_num: 3 };
    pub const SVGA_800X600: Self = VideoMode { width: 800, height: 600, bpp: 32, mode_num: 4 };
    pub const SVGA_1024X768: Self = VideoMode { width: 1024, height: 768, bpp: 32, mode_num: 5 };

    pub fn from_basic_mode(mode: u8) -> Self {
        match mode {
            0 => Self::TEXT_80X25,
            1 => Self::VGA_320X200,
            2 => Self::VGA_640X200,
            3 => Self::VGA_640X480,
            4 => Self::SVGA_800X600,
            5 => Self::SVGA_1024X768,
            _ => Self::TEXT_80X25,
        }
    }
}

/// WATOS VGA/SVGA graphics backend
pub struct WatosVgaBackend {
    mode: VideoMode,
    framebuffer: Vec<u8>,    // Pixel data - size depends on bpp (8-bit indexed or 32-bit RGBA)
    cursor_x: usize,
    cursor_y: usize,
    fg_color: u8,
    bg_color: u8,
    dirty: bool,             // Track if buffer needs refresh
    session_id: Option<u32>, // VGA session ID for multi-session support
}

/// WATOS VGA syscalls
mod syscall {
    pub const SYS_VGA_SET_MODE: u32 = 30;
    pub const SYS_VGA_SET_PIXEL: u32 = 31;
    pub const SYS_VGA_GET_PIXEL: u32 = 32;
    pub const SYS_VGA_BLIT: u32 = 33;
    pub const SYS_VGA_CLEAR: u32 = 34;
    pub const SYS_VGA_FLIP: u32 = 35;
    pub const SYS_VGA_SET_PALETTE: u32 = 36;
    pub const SYS_VGA_CREATE_SESSION: u32 = 37;
    pub const SYS_VGA_DESTROY_SESSION: u32 = 38;
    pub const SYS_VGA_SET_ACTIVE_SESSION: u32 = 39;
    pub const SYS_VGA_GET_SESSION_INFO: u32 = 40;
    pub const SYS_VGA_ENUMERATE_MODES: u32 = 41;

    #[inline(always)]
    pub unsafe fn syscall0(num: u32) -> u64 {
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
    pub unsafe fn syscall1(num: u32, arg1: u64) -> u64 {
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
    pub unsafe fn syscall2(num: u32, arg1: u64, arg2: u64) -> u64 {
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
    pub unsafe fn syscall3(num: u32, arg1: u64, arg2: u64, arg3: u64) -> u64 {
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
    pub unsafe fn syscall4(num: u32, arg1: u64, arg2: u64, arg3: u64, arg4: u64) -> u64 {
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
}

impl WatosVgaBackend {
    /// Create a new VGA backend with the specified mode using SVGA sessions
    pub fn new(mode: VideoMode) -> Result<Self> {
        // Calculate buffer size based on bits per pixel
        // For 8-bit modes: 1 byte per pixel
        // For 32-bit modes: 4 bytes per pixel (RGBA)
        let bytes_per_pixel = if mode.bpp <= 8 { 1 } else { (mode.bpp as usize + 7) / 8 };
        let buffer_size = mode.width * mode.height * bytes_per_pixel;
        let framebuffer = vec![0u8; buffer_size];

        // For text mode (mode 0), we don't create a graphics session
        let session_id = if mode.mode_num == 0 {
            None
        } else {
            // Create a VGA session for graphics mode
            // Args: width, height, bpp
            let session_result = unsafe {
                syscall::syscall3(
                    syscall::SYS_VGA_CREATE_SESSION,
                    mode.width as u64,
                    mode.height as u64,
                    mode.bpp as u64,
                )
            };

            if session_result == u64::MAX {
                return Err(Error::RuntimeError(format!(
                    "Failed to create VGA session for mode {}x{}x{}",
                    mode.width, mode.height, mode.bpp
                )));
            }

            // Set this session as active
            unsafe {
                syscall::syscall1(syscall::SYS_VGA_SET_ACTIVE_SESSION, session_result);
            }

            Some(session_result as u32)
        };

        Ok(WatosVgaBackend {
            mode,
            framebuffer,
            cursor_x: 0,
            cursor_y: 0,
            fg_color: 15,  // White
            bg_color: 0,   // Black
            dirty: true,
            session_id,
        })
    }

    /// Create backend for 320x200 VGA mode (default graphics mode)
    pub fn new_vga() -> Result<Self> {
        Self::new(VideoMode::VGA_320X200)
    }

    /// Create backend for 640x480 VGA mode
    pub fn new_vga_hi() -> Result<Self> {
        Self::new(VideoMode::VGA_640X480)
    }

    /// Create backend for 800x600 SVGA mode
    pub fn new_svga() -> Result<Self> {
        Self::new(VideoMode::SVGA_800X600)
    }

    /// Create backend for 1024x768 SVGA mode
    pub fn new_svga_hi() -> Result<Self> {
        Self::new(VideoMode::SVGA_1024X768)
    }

    /// Get pixel buffer for direct access
    pub fn get_framebuffer(&self) -> &[u8] {
        &self.framebuffer
    }

    /// Get mutable pixel buffer for direct access
    pub fn get_framebuffer_mut(&mut self) -> &mut [u8] {
        self.dirty = true;
        &mut self.framebuffer
    }

    /// Set pixel in local buffer
    fn set_pixel_local(&mut self, x: usize, y: usize, color: u8) {
        if x < self.mode.width && y < self.mode.height {
            if self.mode.bpp <= 8 {
                // 8-bit indexed color
                let idx = y * self.mode.width + x;
                self.framebuffer[idx] = color;
            } else {
                // 32-bit RGBA color - expand 8-bit color index to RGBA
                let bytes_per_pixel = (self.mode.bpp as usize + 7) / 8;
                let idx = (y * self.mode.width + x) * bytes_per_pixel;
                
                // Map 8-bit color to RGB (simple palette mapping)
                // For now, use EGA-style 16-color palette
                let rgb = if color < 16 {
                    PALETTE_16[color as usize]
                } else {
                    // For higher indices, create a grayscale
                    let gray = color;
                    ((gray as u32) << 16) | ((gray as u32) << 8) | (gray as u32)
                };
                
                // Write BGRA (assuming BGR pixel format)
                self.framebuffer[idx] = (rgb & 0xFF) as u8;     // B
                self.framebuffer[idx + 1] = ((rgb >> 8) & 0xFF) as u8;  // G
                self.framebuffer[idx + 2] = ((rgb >> 16) & 0xFF) as u8; // R
                if bytes_per_pixel == 4 {
                    self.framebuffer[idx + 3] = 0xFF;           // A
                }
            }
            self.dirty = true;
        }
    }

    /// Get pixel from local buffer
    fn get_pixel_local(&self, x: usize, y: usize) -> u8 {
        if x < self.mode.width && y < self.mode.height {
            if self.mode.bpp <= 8 {
                // 8-bit indexed color
                let idx = y * self.mode.width + x;
                self.framebuffer[idx]
            } else {
                // 32-bit RGBA - return approximation as 8-bit index
                // For simplicity, just return a grayscale value
                let bytes_per_pixel = (self.mode.bpp as usize + 7) / 8;
                let idx = (y * self.mode.width + x) * bytes_per_pixel;
                let r = self.framebuffer[idx + 2];
                let g = self.framebuffer[idx + 1];
                let b = self.framebuffer[idx];
                // Simple grayscale conversion
                ((r as u16 + g as u16 + b as u16) / 3) as u8
            }
        } else {
            0
        }
    }

    /// Commit framebuffer to VGA memory via kernel
    pub fn commit(&self) {
        unsafe {
            // Blit entire framebuffer to kernel VGA driver
            // Args: buffer_ptr, width, height, stride
            syscall::syscall4(
                syscall::SYS_VGA_BLIT,
                self.framebuffer.as_ptr() as u64,
                self.mode.width as u64,
                self.mode.height as u64,
                self.mode.width as u64,  // stride = width for packed buffer
            );

            // Flip to display
            syscall::syscall0(syscall::SYS_VGA_FLIP);
        }
    }
}

impl GraphicsBackend for WatosVgaBackend {
    fn pset(&mut self, x: i32, y: i32, color: u8) -> Result<()> {
        if x >= 0 && y >= 0 {
            self.set_pixel_local(x as usize, y as usize, color);
        }
        Ok(())
    }

    fn line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: u8) -> Result<()> {
        // Bresenham's line algorithm
        let dx = (x2 - x1).abs();
        let dy = (y2 - y1).abs();
        let sx = if x1 < x2 { 1 } else { -1 };
        let sy = if y1 < y2 { 1 } else { -1 };
        let mut err = dx - dy;
        let mut x = x1;
        let mut y = y1;

        loop {
            self.pset(x, y, color)?;
            if x == x2 && y == y2 {
                break;
            }
            let e2 = 2 * err;
            if e2 > -dy {
                err -= dy;
                x += sx;
            }
            if e2 < dx {
                err += dx;
                y += sy;
            }
        }
        Ok(())
    }

    fn circle(&mut self, x: i32, y: i32, radius: i32, color: u8) -> Result<()> {
        // Midpoint circle algorithm
        let mut dx = radius;
        let mut dy = 0;
        let mut err = 0;

        while dx >= dy {
            self.pset(x + dx, y + dy, color)?;
            self.pset(x + dy, y + dx, color)?;
            self.pset(x - dy, y + dx, color)?;
            self.pset(x - dx, y + dy, color)?;
            self.pset(x - dx, y - dy, color)?;
            self.pset(x - dy, y - dx, color)?;
            self.pset(x + dy, y - dx, color)?;
            self.pset(x + dx, y - dy, color)?;

            if err <= 0 {
                dy += 1;
                err += 2 * dy + 1;
            }
            if err > 0 {
                dx -= 1;
                err -= 2 * dx + 1;
            }
        }
        Ok(())
    }

    fn cls(&mut self) {
        // Fill buffer with background color
        for pixel in &mut self.framebuffer {
            *pixel = self.bg_color;
        }
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.dirty = true;

        // Also clear kernel VGA buffer
        unsafe {
            syscall::syscall1(syscall::SYS_VGA_CLEAR, self.bg_color as u64);
        }
    }

    fn locate(&mut self, row: usize, col: usize) -> Result<()> {
        if row >= self.mode.height || col >= self.mode.width {
            return Err(Error::RuntimeError(format!(
                "LOCATE position out of range: ({}, {})",
                row, col
            )));
        }
        self.cursor_y = row;
        self.cursor_x = col;
        Ok(())
    }

    fn color(&mut self, fg: Option<u8>, bg: Option<u8>) {
        if let Some(foreground) = fg {
            self.fg_color = foreground;
        }
        if let Some(background) = bg {
            self.bg_color = background;
        }
    }

    fn display(&mut self) {
        if self.dirty {
            self.commit();
            self.dirty = false;
        }
    }

    fn get_size(&self) -> (usize, usize) {
        (self.mode.height, self.mode.width)
    }

    fn get_cursor(&self) -> (usize, usize) {
        (self.cursor_y, self.cursor_x)
    }

    fn should_close(&self) -> bool {
        // On WATOS, check for ESC key or program termination
        // For now, always return false (program controls exit)
        false
    }

    fn update(&mut self) -> Result<()> {
        // Auto-commit if dirty
        if self.dirty {
            self.commit();
            self.dirty = false;
        }
        Ok(())
    }
}

/// Helper function to create VGA backend from GW-BASIC SCREEN mode
pub fn from_screen_mode(mode: u8) -> Result<WatosVgaBackend> {
    let video_mode = VideoMode::from_basic_mode(mode);
    WatosVgaBackend::new(video_mode)
}

impl Drop for WatosVgaBackend {
    fn drop(&mut self) {
        // Clean up VGA session when backend is dropped
        if let Some(session_id) = self.session_id {
            unsafe {
                syscall::syscall1(syscall::SYS_VGA_DESTROY_SESSION, session_id as u64);
            }
        }
    }
}
