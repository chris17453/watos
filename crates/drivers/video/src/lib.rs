//! WATOS Video Driver Subsystem
//!
//! Provides VGA, SVGA, and framebuffer drivers with multi-session support

#![no_std]

extern crate alloc;

pub mod modes;
pub mod vga;
pub mod svga;
pub mod framebuffer;
pub mod session;

use spin::Mutex;
use watos_driver_traits::video::{VideoDevice, VideoMode, Color};
use session::SessionManager;

/// Video driver type
pub enum VideoDriverType {
    /// GOP framebuffer (UEFI)
    Framebuffer(framebuffer::FramebufferDriver),
    /// Standard VGA
    Vga(vga::VgaDriver),
    /// SVGA/VESA
    Svga(svga::SvgaDriver),
}

impl VideoDriverType {
    /// Get the underlying VideoDevice trait object
    pub fn as_device_mut(&mut self) -> &mut dyn VideoDevice {
        match self {
            VideoDriverType::Framebuffer(d) => d,
            VideoDriverType::Vga(d) => d,
            VideoDriverType::Svga(d) => d,
        }
    }

    /// Get the underlying VideoDevice trait object (immutable)
    pub fn as_device(&self) -> &dyn VideoDevice {
        match self {
            VideoDriverType::Framebuffer(d) => d,
            VideoDriverType::Vga(d) => d,
            VideoDriverType::Svga(d) => d,
        }
    }
}

/// Global video driver instance
static VIDEO_DRIVER: Mutex<Option<VideoDriverType>> = Mutex::new(None);

/// Global session manager
static SESSION_MANAGER: Mutex<SessionManager> = Mutex::new(SessionManager::new());

/// Initialize video driver from boot info
pub fn init_from_boot_info(fb_addr: u64, width: u32, height: u32, pitch: u32, bpp: u32, is_bgr: bool) {
    let driver = framebuffer::FramebufferDriver::new(fb_addr, width, height, pitch, bpp, is_bgr);
    *VIDEO_DRIVER.lock() = Some(VideoDriverType::Framebuffer(driver));
}

/// Initialize VGA driver
pub fn init_vga() {
    let driver = vga::VgaDriver::new();
    *VIDEO_DRIVER.lock() = Some(VideoDriverType::Vga(driver));
}

/// Initialize SVGA driver
pub fn init_svga(fb_addr: u64, width: u32, height: u32, pitch: u32, bpp: u32) {
    let driver = svga::SvgaDriver::new(fb_addr, width, height, pitch, bpp);
    *VIDEO_DRIVER.lock() = Some(VideoDriverType::Svga(driver));
}

/// Get current video mode
pub fn get_current_mode() -> Option<VideoMode> {
    VIDEO_DRIVER.lock().as_ref().map(|d| d.as_device().current_mode())
}

/// Set video mode
pub fn set_mode(mode: VideoMode) -> Result<(), &'static str> {
    let mut driver = VIDEO_DRIVER.lock();
    if let Some(ref mut d) = *driver {
        d.as_device_mut().set_mode(mode).map_err(|_| "Failed to set video mode")
    } else {
        Err("No video driver initialized")
    }
}

/// Set pixel in physical framebuffer
pub fn set_pixel(x: u32, y: u32, color: Color) {
    let mut driver = VIDEO_DRIVER.lock();
    if let Some(ref mut d) = *driver {
        d.as_device_mut().set_pixel(x, y, color);
    }
}

/// Get pixel from physical framebuffer
pub fn get_pixel(x: u32, y: u32) -> Color {
    let driver = VIDEO_DRIVER.lock();
    if let Some(ref d) = *driver {
        d.as_device().get_pixel(x, y)
    } else {
        0
    }
}

/// Clear physical framebuffer
pub fn clear(color: Color) {
    let mut driver = VIDEO_DRIVER.lock();
    if let Some(ref mut d) = *driver {
        d.as_device_mut().clear(color);
    }
}

/// Fill rectangle in physical framebuffer
pub fn fill_rect(x: u32, y: u32, width: u32, height: u32, color: Color) {
    let mut driver = VIDEO_DRIVER.lock();
    if let Some(ref mut d) = *driver {
        d.as_device_mut().fill_rect(x, y, width, height, color);
    }
}

/// Set palette entry
pub fn set_palette(index: u8, r: u8, g: u8, b: u8) -> Result<(), &'static str> {
    let mut driver = VIDEO_DRIVER.lock();
    if let Some(ref mut d) = *driver {
        d.as_device_mut().set_palette(index, r, g, b).map_err(|_| "Failed to set palette")
    } else {
        Err("No video driver initialized")
    }
}

/// Get framebuffer address
pub fn get_framebuffer_addr() -> Option<*mut u8> {
    VIDEO_DRIVER.lock().as_ref().map(|d| d.as_device().framebuffer())
}

/// Get framebuffer pitch
pub fn get_pitch() -> Option<usize> {
    VIDEO_DRIVER.lock().as_ref().map(|d| d.as_device().pitch())
}

/// Get available video modes
pub fn get_available_modes() -> alloc::vec::Vec<VideoMode> {
    let driver = VIDEO_DRIVER.lock();
    if let Some(ref d) = *driver {
        d.as_device().available_modes().to_vec()
    } else {
        alloc::vec::Vec::new()
    }
}

// ============================================================================
// Session Management API
// ============================================================================

/// Create a new virtual framebuffer session
pub fn create_session(mode: VideoMode) -> Option<u32> {
    SESSION_MANAGER.lock().create_session(mode)
}

/// Destroy a session
pub fn destroy_session(session_id: u32) -> bool {
    SESSION_MANAGER.lock().destroy_session(session_id)
}

/// Set the active session (the one being displayed)
pub fn set_active_session(session_id: u32) -> bool {
    SESSION_MANAGER.lock().set_active_session(session_id)
}

/// Get the active session ID
pub fn get_active_session() -> Option<u32> {
    SESSION_MANAGER.lock().get_active_session()
}

/// Get session mode and info
pub fn get_session_info(session_id: u32) -> Option<VideoMode> {
    SESSION_MANAGER.lock()
        .get_session(session_id)
        .map(|fb| fb.mode)
}

/// Set pixel in a session's virtual framebuffer
pub fn session_set_pixel(session_id: u32, x: u32, y: u32, color: Color) {
    if let Some(fb) = SESSION_MANAGER.lock().get_session_mut(session_id) {
        fb.set_pixel(x, y, color);
    }
}

/// Get pixel from a session's virtual framebuffer
pub fn session_get_pixel(session_id: u32, x: u32, y: u32) -> Color {
    if let Some(fb) = SESSION_MANAGER.lock().get_session(session_id) {
        fb.get_pixel(x, y)
    } else {
        0
    }
}

/// Clear a session's virtual framebuffer
pub fn session_clear(session_id: u32, color: Color) {
    if let Some(fb) = SESSION_MANAGER.lock().get_session_mut(session_id) {
        fb.clear(color);
    }
}

/// Blit data to a session's virtual framebuffer
pub fn session_blit(session_id: u32, data: &[u8], width: usize, height: usize, stride: usize) {
    if let Some(fb) = SESSION_MANAGER.lock().get_session_mut(session_id) {
        let bytes_per_pixel = (fb.mode.bpp as usize + 7) / 8;
        // stride is in pixels, convert to bytes for row offset
        let row_stride = stride * bytes_per_pixel;
        for y in 0..height.min(fb.mode.height as usize) {
            for x in 0..width.min(fb.mode.width as usize) {
                let src_offset = y * row_stride + x * bytes_per_pixel;
                if src_offset + bytes_per_pixel <= data.len() {
                    // For 8-bit indexed, store byte directly
                    let color = data[src_offset] as Color;
                    fb.set_pixel(x as u32, y as u32, color);
                }
            }
        }
    }
}

/// EGA/VGA 16-color palette for indexed modes
const PALETTE_16: [u32; 16] = [
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

/// Convert indexed color to RGB
fn indexed_to_rgb(index: u8) -> u32 {
    if index < 16 {
        PALETTE_16[index as usize]
    } else {
        // Grayscale for 16-255
        let g = index as u32;
        (g << 16) | (g << 8) | g
    }
}

/// Composite a session to the physical display (flip/swap buffers)
pub fn session_flip(session_id: u32) {
    // Get session info and physical display info
    let (session_mode, phys_mode) = {
        let manager = SESSION_MANAGER.lock();
        let sm = manager.get_session(session_id).map(|fb| fb.mode);
        let pm = get_current_mode();
        (sm, pm)
    };

    let (session_mode, phys_mode) = match (session_mode, phys_mode) {
        (Some(s), Some(p)) => (s, p),
        _ => return,
    };

    // Calculate scaling factors
    let scale_x = phys_mode.width / session_mode.width.max(1);
    let scale_y = phys_mode.height / session_mode.height.max(1);
    let scale = scale_x.min(scale_y).max(1);

    // Offset to center the scaled image
    let offset_x = (phys_mode.width - session_mode.width * scale) / 2;
    let offset_y = (phys_mode.height - session_mode.height * scale) / 2;

    // Clear physical display first
    clear(0);

    // Copy pixels with scaling and palette conversion
    let manager = SESSION_MANAGER.lock();
    if let Some(fb) = manager.get_session(session_id) {
        let is_indexed = fb.mode.bpp <= 8;

        for y in 0..fb.mode.height {
            for x in 0..fb.mode.width {
                let color = fb.get_pixel(x, y);

                // Convert indexed to RGB if needed
                let rgb = if is_indexed {
                    indexed_to_rgb(color as u8)
                } else {
                    color
                };

                // Draw scaled pixel
                for sy in 0..scale {
                    for sx in 0..scale {
                        let px = offset_x + x * scale + sx;
                        let py = offset_y + y * scale + sy;
                        if px < phys_mode.width && py < phys_mode.height {
                            set_pixel(px, py, rgb);
                        }
                    }
                }
            }
        }
    }
}
