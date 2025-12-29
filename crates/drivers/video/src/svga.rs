//! SVGA/VESA Driver
//!
//! Supports higher resolution SVGA modes

use watos_driver_traits::{Driver, DriverResult, DriverError, DriverState};
use watos_driver_traits::video::{VideoDevice, VideoMode, VideoDeviceInfo, Color};
use crate::modes;

/// SVGA Driver (VESA VBE)
///
/// Note: In long mode, we cannot make BIOS calls directly.
/// Real VESA mode switching would require:
/// 1. Storing mode info during boot (in real mode)
/// 2. Or using a VM86 monitor to execute BIOS calls
/// 3. Or pre-configuring modes via bootloader
///
/// For now, this driver provides the interface but delegates to GOP framebuffer
pub struct SvgaDriver {
    state: DriverState,
    current_mode: VideoMode,
    available_modes: &'static [VideoMode],
    framebuffer_addr: u64,
    pitch: usize,
    palette: [u32; 256],
}

impl SvgaDriver {
    /// Create a new SVGA driver
    pub fn new(fb_addr: u64, width: u32, height: u32, pitch: u32, bpp: u32) -> Self {
        let mode = modes::find_mode(width, height, bpp as u8)
            .unwrap_or(modes::SVGA_800X600X32);

        SvgaDriver {
            state: DriverState::Loaded,
            current_mode: mode,
            available_modes: &[
                // 800x600
                modes::SVGA_800X600X8,
                modes::SVGA_800X600X16,
                modes::SVGA_800X600X24,
                modes::SVGA_800X600X32,
                // 1024x768
                modes::SVGA_1024X768X8,
                modes::SVGA_1024X768X16,
                modes::SVGA_1024X768X24,
                modes::SVGA_1024X768X32,
                // 1280x1024
                modes::SVGA_1280X1024X8,
                modes::SVGA_1280X1024X16,
                modes::SVGA_1280X1024X24,
                modes::SVGA_1280X1024X32,
                // 1920x1080
                modes::SVGA_1920X1080X24,
                modes::SVGA_1920X1080X32,
            ],
            framebuffer_addr: fb_addr,
            pitch: pitch as usize,
            palette: [0u32; 256],
        }
    }

    /// Get framebuffer as mutable slice
    fn framebuffer_mut(&mut self) -> &mut [u8] {
        let size = self.pitch * self.current_mode.height as usize;
        unsafe {
            core::slice::from_raw_parts_mut(self.framebuffer_addr as *mut u8, size)
        }
    }

    /// Get framebuffer as immutable slice
    fn framebuffer_ref(&self) -> &[u8] {
        let size = self.pitch * self.current_mode.height as usize;
        unsafe {
            core::slice::from_raw_parts(self.framebuffer_addr as *const u8, size)
        }
    }
}

impl Driver for SvgaDriver {
    fn info(&self) -> watos_driver_traits::DriverInfo {
        watos_driver_traits::DriverInfo {
            name: "SVGA/VESA Driver",
            version: "0.1.0",
            author: "WATOS",
            description: "SVGA/VESA driver",
        }
    }

    fn init(&mut self) -> DriverResult<()> {
        // Initialize default palette
        init_svga_palette(&mut self.palette);
        self.state = DriverState::Ready;
        Ok(())
    }

    fn start(&mut self) -> DriverResult<()> {
        self.state = DriverState::Active;
        Ok(())
    }

    fn stop(&mut self) -> DriverResult<()> {
        self.state = DriverState::Stopped;
        Ok(())
    }

    fn state(&self) -> DriverState {
        self.state
    }
}

impl VideoDevice for SvgaDriver {
    fn current_mode(&self) -> VideoMode {
        self.current_mode
    }

    fn available_modes(&self) -> &[VideoMode] {
        self.available_modes
    }

    fn set_mode(&mut self, mode: VideoMode) -> DriverResult<()> {
        // Check if mode is in available list
        if self.available_modes.iter().any(|m| *m == mode) {
            // In a real implementation, would call VBE to switch modes
            // For now, only allow if it matches current GOP mode
            if mode.width == self.current_mode.width
                && mode.height == self.current_mode.height
                && mode.bpp == self.current_mode.bpp {
                self.current_mode = mode;
                Ok(())
            } else {
                Err(DriverError::NotSupported)
            }
        } else {
            Err(DriverError::NotSupported)
        }
    }

    fn framebuffer(&self) -> *mut u8 {
        self.framebuffer_addr as *mut u8
    }

    fn pitch(&self) -> usize {
        self.pitch
    }

    fn set_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x >= self.current_mode.width || y >= self.current_mode.height {
            return;
        }

        let bytes_per_pixel = (self.current_mode.bpp as usize + 7) / 8;
        let offset = y as usize * self.pitch + x as usize * bytes_per_pixel;

        let fb = self.framebuffer_mut();
        let r = ((color >> 16) & 0xFF) as u8;
        let g = ((color >> 8) & 0xFF) as u8;
        let b = (color & 0xFF) as u8;

        match bytes_per_pixel {
            1 => {
                fb[offset] = (color & 0xFF) as u8;
            }
            2 => {
                // RGB565
                let r5 = (r >> 3) & 0x1F;
                let g6 = (g >> 2) & 0x3F;
                let b5 = (b >> 3) & 0x1F;
                let val = ((r5 as u16) << 11) | ((g6 as u16) << 5) | (b5 as u16);
                fb[offset] = (val & 0xFF) as u8;
                fb[offset + 1] = (val >> 8) as u8;
            }
            3 => {
                // RGB24
                fb[offset] = b;
                fb[offset + 1] = g;
                fb[offset + 2] = r;
            }
            4 => {
                // RGBA32
                fb[offset] = b;
                fb[offset + 1] = g;
                fb[offset + 2] = r;
                fb[offset + 3] = 0xFF;
            }
            _ => {}
        }
    }

    fn get_pixel(&self, x: u32, y: u32) -> Color {
        if x >= self.current_mode.width || y >= self.current_mode.height {
            return 0;
        }

        let bytes_per_pixel = (self.current_mode.bpp as usize + 7) / 8;
        let offset = y as usize * self.pitch + x as usize * bytes_per_pixel;

        let fb = self.framebuffer_ref();

        match bytes_per_pixel {
            1 => {
                let index = fb[offset] as usize;
                self.palette[index]
            }
            2 => {
                let val = u16::from_le_bytes([fb[offset], fb[offset + 1]]);
                let r = ((val >> 11) & 0x1F) as u8;
                let g = ((val >> 5) & 0x3F) as u8;
                let b = (val & 0x1F) as u8;
                let r8 = (r * 255 / 31) as u32;
                let g8 = (g * 255 / 63) as u32;
                let b8 = (b * 255 / 31) as u32;
                (r8 << 16) | (g8 << 8) | b8 | 0xFF000000
            }
            3 => {
                let b = fb[offset];
                let g = fb[offset + 1];
                let r = fb[offset + 2];
                ((r as u32) << 16) | ((g as u32) << 8) | (b as u32) | 0xFF000000
            }
            4 => {
                let b = fb[offset];
                let g = fb[offset + 1];
                let r = fb[offset + 2];
                ((r as u32) << 16) | ((g as u32) << 8) | (b as u32) | 0xFF000000
            }
            _ => 0,
        }
    }

    fn info(&self) -> VideoDeviceInfo {
        VideoDeviceInfo {
            name: "SVGA/VESA Driver",
            mode: self.current_mode,
            framebuffer_size: self.pitch * self.current_mode.height as usize,
        }
    }

    fn set_palette(&mut self, index: u8, r: u8, g: u8, b: u8) -> DriverResult<()> {
        self.palette[index as usize] = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
        Ok(())
    }
}

/// Initialize default SVGA palette
fn init_svga_palette(palette: &mut [u32; 256]) {
    // Standard 16 color EGA palette
    const EGA_PALETTE: [u32; 16] = [
        0x000000, 0x0000AA, 0x00AA00, 0x00AAAA,
        0xAA0000, 0xAA00AA, 0xAA5500, 0xAAAAAA,
        0x555555, 0x5555FF, 0x55FF55, 0x55FFFF,
        0xFF5555, 0xFF55FF, 0xFFFF55, 0xFFFFFF,
    ];

    palette[..16].copy_from_slice(&EGA_PALETTE);

    // Fill rest with grayscale gradient
    for i in 16..256 {
        let gray = ((i - 16) * 255 / (256 - 16)) as u8;
        palette[i] = ((gray as u32) << 16) | ((gray as u32) << 8) | (gray as u32);
    }
}
