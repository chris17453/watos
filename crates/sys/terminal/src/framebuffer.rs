//! Framebuffer abstraction
//!
//! Provides a trait for framebuffer backends, supporting:
//! - Multiple pixel formats (RGB, BGR)
//! - Double buffering
//! - Mode switching (resolution changes)

use crate::color::Color;

/// Pixel format of the framebuffer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// Red-Green-Blue (most common)
    Rgb,
    /// Blue-Green-Red (some hardware)
    Bgr,
}

/// Information about the current framebuffer mode
#[derive(Debug, Clone, Copy)]
pub struct FramebufferInfo {
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Bytes per row (may be larger than width * bpp due to padding)
    pub pitch: u32,
    /// Bits per pixel (typically 32)
    pub bpp: u32,
    /// Pixel format
    pub format: PixelFormat,
}

impl FramebufferInfo {
    /// Calculate number of character columns for given font width
    pub fn cols(&self, font_width: u32) -> u32 {
        self.width / font_width
    }

    /// Calculate number of character rows for given font height
    pub fn rows(&self, font_height: u32) -> u32 {
        self.height / font_height
    }
}

/// Framebuffer backend trait
///
/// Implementations can provide:
/// - Direct framebuffer access
/// - Double buffering with page flipping
/// - Hardware acceleration
pub trait Framebuffer {
    /// Get framebuffer information
    fn info(&self) -> FramebufferInfo;

    /// Get a mutable pointer to the back buffer
    /// For single-buffered, this is the visible buffer.
    /// For double-buffered, this is the off-screen buffer.
    fn back_buffer(&mut self) -> *mut u32;

    /// Swap front and back buffers (for double buffering)
    /// For single-buffered implementations, this is a no-op.
    fn swap_buffers(&mut self);

    /// Check if double buffering is supported
    fn is_double_buffered(&self) -> bool {
        false
    }

    /// Set a pixel in the back buffer
    fn set_pixel(&mut self, x: u32, y: u32, color: Color) {
        let info = self.info();
        if x >= info.width || y >= info.height {
            return;
        }

        let pixel = match info.format {
            PixelFormat::Rgb => color.to_u32(),
            PixelFormat::Bgr => color.to_bgr(),
        };

        unsafe {
            let offset = (y * info.pitch / 4 + x) as usize;
            *self.back_buffer().add(offset) = pixel;
        }
    }

    /// Fill a rectangle with a color
    fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: Color) {
        let info = self.info();
        let pixel = match info.format {
            PixelFormat::Rgb => color.to_u32(),
            PixelFormat::Bgr => color.to_bgr(),
        };

        let x_end = (x + w).min(info.width);
        let y_end = (y + h).min(info.height);
        let pitch = info.pitch / 4;

        unsafe {
            let buffer = self.back_buffer();
            for py in y..y_end {
                for px in x..x_end {
                    let offset = (py * pitch + px) as usize;
                    *buffer.add(offset) = pixel;
                }
            }
        }
    }

    /// Clear the entire framebuffer with a color
    fn clear(&mut self, color: Color) {
        let info = self.info();
        self.fill_rect(0, 0, info.width, info.height, color);
    }

    /// Copy a region within the framebuffer (for scrolling)
    fn copy_rect(&mut self, src_x: u32, src_y: u32, dst_x: u32, dst_y: u32, w: u32, h: u32) {
        let info = self.info();
        let pitch = info.pitch / 4;

        // Handle overlapping regions by choosing copy direction
        let copy_up = dst_y < src_y || (dst_y == src_y && dst_x < src_x);

        unsafe {
            let buffer = self.back_buffer();

            if copy_up {
                // Copy from top to bottom
                for row in 0..h {
                    for col in 0..w {
                        let src_offset = ((src_y + row) * pitch + src_x + col) as usize;
                        let dst_offset = ((dst_y + row) * pitch + dst_x + col) as usize;
                        *buffer.add(dst_offset) = *buffer.add(src_offset);
                    }
                }
            } else {
                // Copy from bottom to top
                for row in (0..h).rev() {
                    for col in (0..w).rev() {
                        let src_offset = ((src_y + row) * pitch + src_x + col) as usize;
                        let dst_offset = ((dst_y + row) * pitch + dst_x + col) as usize;
                        *buffer.add(dst_offset) = *buffer.add(src_offset);
                    }
                }
            }
        }
    }
}

/// Simple single-buffered framebuffer implementation
pub struct SimpleFramebuffer {
    buffer: *mut u32,
    info: FramebufferInfo,
}

impl SimpleFramebuffer {
    /// Create a new simple framebuffer from raw pointer and info
    ///
    /// # Safety
    /// The buffer pointer must be valid for the lifetime of this struct.
    pub unsafe fn new(buffer: *mut u32, info: FramebufferInfo) -> Self {
        Self { buffer, info }
    }

    /// Create from bootloader-provided information
    ///
    /// # Safety
    /// All pointers and values must be valid.
    pub unsafe fn from_boot_info(
        addr: u64,
        width: u32,
        height: u32,
        pitch: u32,
        bpp: u32,
        bgr: bool,
    ) -> Self {
        let info = FramebufferInfo {
            width,
            height,
            pitch,
            bpp,
            format: if bgr { PixelFormat::Bgr } else { PixelFormat::Rgb },
        };
        Self {
            buffer: addr as *mut u32,
            info,
        }
    }
}

impl Framebuffer for SimpleFramebuffer {
    fn info(&self) -> FramebufferInfo {
        self.info
    }

    fn back_buffer(&mut self) -> *mut u32 {
        self.buffer
    }

    fn swap_buffers(&mut self) {
        // No-op for single buffered
    }
}

// Safety: SimpleFramebuffer is Send if its contents are accessed properly
unsafe impl Send for SimpleFramebuffer {}
