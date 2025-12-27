//! Video Device Trait
//!
//! Implemented by video drivers (VGA, GOP framebuffer, etc.)
//! Used by the console/graphics subsystem

use crate::DriverResult;

/// RGBA color (red, green, blue, alpha)
pub type Color = u32;

/// Video mode descriptor
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoMode {
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Bits per pixel (8, 16, 24, 32)
    pub bpp: u8,
    /// Pixel format
    pub format: PixelFormat,
}

/// Pixel format in framebuffer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// Red-Green-Blue (RGB888 or RGBX8888)
    Rgb,
    /// Blue-Green-Red (BGR888 or BGRX8888)
    Bgr,
    /// 8-bit indexed color
    Indexed,
}

/// Video device trait
pub trait VideoDevice: Send + Sync {
    /// Get current video mode
    fn current_mode(&self) -> VideoMode;

    /// Get available video modes
    fn available_modes(&self) -> &[VideoMode];

    /// Set video mode
    fn set_mode(&mut self, mode: VideoMode) -> DriverResult<()>;

    /// Get framebuffer address
    fn framebuffer(&self) -> *mut u8;

    /// Get framebuffer pitch (bytes per scanline)
    fn pitch(&self) -> usize;

    /// Set a single pixel
    fn set_pixel(&mut self, x: u32, y: u32, color: Color);

    /// Get a single pixel
    fn get_pixel(&self, x: u32, y: u32) -> Color;

    /// Fill rectangle with color
    fn fill_rect(&mut self, x: u32, y: u32, width: u32, height: u32, color: Color) {
        for py in y..y + height {
            for px in x..x + width {
                self.set_pixel(px, py, color);
            }
        }
    }

    /// Copy rectangle (blit)
    fn copy_rect(&mut self, src_x: u32, src_y: u32, dst_x: u32, dst_y: u32, width: u32, height: u32) {
        // Default naive implementation - drivers can optimize
        for dy in 0..height {
            for dx in 0..width {
                let color = self.get_pixel(src_x + dx, src_y + dy);
                self.set_pixel(dst_x + dx, dst_y + dy, color);
            }
        }
    }

    /// Clear screen with color
    fn clear(&mut self, color: Color) {
        let mode = self.current_mode();
        self.fill_rect(0, 0, mode.width, mode.height, color);
    }

    /// Get device information
    fn info(&self) -> VideoDeviceInfo;

    /// Set palette entry (for indexed modes)
    fn set_palette(&mut self, _index: u8, _r: u8, _g: u8, _b: u8) -> DriverResult<()> {
        Ok(()) // Default: ignore
    }
}

/// Information about a video device
#[derive(Debug, Clone)]
pub struct VideoDeviceInfo {
    /// Device name
    pub name: &'static str,
    /// Current mode
    pub mode: VideoMode,
    /// Framebuffer size in bytes
    pub framebuffer_size: usize,
}

/// Helper to create RGBA color
pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

/// Helper to create RGB color (alpha = 255)
pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
    rgba(r, g, b, 255)
}

/// Common colors
pub mod colors {
    use super::rgb;
    pub const BLACK: u32 = rgb(0, 0, 0);
    pub const WHITE: u32 = rgb(255, 255, 255);
    pub const RED: u32 = rgb(255, 0, 0);
    pub const GREEN: u32 = rgb(0, 255, 0);
    pub const BLUE: u32 = rgb(0, 0, 255);
    pub const YELLOW: u32 = rgb(255, 255, 0);
    pub const CYAN: u32 = rgb(0, 255, 255);
    pub const MAGENTA: u32 = rgb(255, 0, 255);
}
