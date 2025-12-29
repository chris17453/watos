//! GOP/Linear Framebuffer Driver
//!
//! Works with UEFI GOP (Graphics Output Protocol) framebuffer

use watos_driver_traits::{Driver, DriverResult, DriverError, DriverState};
use watos_driver_traits::video::{VideoDevice, VideoMode, VideoDeviceInfo, Color, PixelFormat};
use spin::Mutex;

/// Framebuffer driver using linear memory-mapped framebuffer (from UEFI GOP)
pub struct FramebufferDriver {
    state: DriverState,
    framebuffer_addr: u64,
    current_mode: VideoMode,
    pitch: usize,
    palette: [u32; 256], // For indexed color modes
}

impl FramebufferDriver {
    /// Create a new framebuffer driver from boot info
    pub fn new(fb_addr: u64, width: u32, height: u32, pitch: u32, bpp: u32, is_bgr: bool) -> Self {
        let format = if bpp == 8 {
            PixelFormat::Indexed
        } else if is_bgr {
            PixelFormat::Bgr
        } else {
            PixelFormat::Rgb
        };

        let mode = VideoMode {
            width,
            height,
            bpp: bpp as u8,
            format,
        };

        // Initialize default VGA palette
        let mut palette = [0u32; 256];
        init_default_palette(&mut palette);

        FramebufferDriver {
            state: DriverState::Initialized,
            framebuffer_addr: fb_addr,
            current_mode: mode,
            pitch: pitch as usize,
            palette,
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

impl Driver for FramebufferDriver {
    fn name(&self) -> &'static str {
        "GOP Framebuffer"
    }

    fn init(&mut self) -> DriverResult<()> {
        self.state = DriverState::Initialized;
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

impl VideoDevice for FramebufferDriver {
    fn current_mode(&self) -> VideoMode {
        self.current_mode
    }

    fn available_modes(&self) -> &[VideoMode] {
        // For GOP, we only support the mode set by UEFI
        core::slice::from_ref(&self.current_mode)
    }

    fn set_mode(&mut self, mode: VideoMode) -> DriverResult<()> {
        // GOP framebuffer mode is fixed at boot time
        // We can only "set" the mode if it matches the current one
        if mode.width == self.current_mode.width 
            && mode.height == self.current_mode.height
            && mode.bpp == self.current_mode.bpp {
            Ok(())
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
                // Indexed color - use color as palette index
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
                // RGB24 or BGR24
                if self.current_mode.format == PixelFormat::Bgr {
                    fb[offset] = b;
                    fb[offset + 1] = g;
                    fb[offset + 2] = r;
                } else {
                    fb[offset] = r;
                    fb[offset + 1] = g;
                    fb[offset + 2] = b;
                }
            }
            4 => {
                // RGBA32 or BGRA32
                if self.current_mode.format == PixelFormat::Bgr {
                    fb[offset] = b;
                    fb[offset + 1] = g;
                    fb[offset + 2] = r;
                    fb[offset + 3] = 0xFF;
                } else {
                    fb[offset] = r;
                    fb[offset + 1] = g;
                    fb[offset + 2] = b;
                    fb[offset + 3] = 0xFF;
                }
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
                // Indexed color - look up in palette
                let index = fb[offset] as usize;
                self.palette[index]
            }
            2 => {
                // RGB565
                let val = u16::from_le_bytes([fb[offset], fb[offset + 1]]);
                let r = ((val >> 11) & 0x1F) as u8;
                let g = ((val >> 5) & 0x3F) as u8;
                let b = (val & 0x1F) as u8;
                ((r << 19) | (g << 10) | (b << 3) | 0xFF000000) as Color
            }
            3 => {
                // RGB24 or BGR24
                if self.current_mode.format == PixelFormat::Bgr {
                    let b = fb[offset];
                    let g = fb[offset + 1];
                    let r = fb[offset + 2];
                    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32) | 0xFF000000
                } else {
                    let r = fb[offset];
                    let g = fb[offset + 1];
                    let b = fb[offset + 2];
                    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32) | 0xFF000000
                }
            }
            4 => {
                // RGBA32 or BGRA32
                if self.current_mode.format == PixelFormat::Bgr {
                    let b = fb[offset];
                    let g = fb[offset + 1];
                    let r = fb[offset + 2];
                    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32) | 0xFF000000
                } else {
                    let r = fb[offset];
                    let g = fb[offset + 1];
                    let b = fb[offset + 2];
                    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32) | 0xFF000000
                }
            }
            _ => 0,
        }
    }

    fn info(&self) -> VideoDeviceInfo {
        VideoDeviceInfo {
            name: self.name(),
            mode: self.current_mode,
            framebuffer_size: self.pitch * self.current_mode.height as usize,
        }
    }

    fn set_palette(&mut self, index: u8, r: u8, g: u8, b: u8) -> DriverResult<()> {
        self.palette[index as usize] = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
        Ok(())
    }
}

/// Initialize default VGA palette (EGA/CGA compatible)
fn init_default_palette(palette: &mut [u32; 256]) {
    // Standard 16 color EGA palette
    const EGA_PALETTE: [u32; 16] = [
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

    // Copy EGA palette
    palette[..16].copy_from_slice(&EGA_PALETTE);

    // Fill rest with grayscale gradient
    for i in 16..256 {
        let gray = ((i - 16) * 255 / (256 - 16)) as u8;
        palette[i] = ((gray as u32) << 16) | ((gray as u32) << 8) | (gray as u32);
    }
}

/// Global framebuffer driver instance
static FRAMEBUFFER_DRIVER: Mutex<Option<FramebufferDriver>> = Mutex::new(None);

/// Initialize the global framebuffer driver
pub fn init(fb_addr: u64, width: u32, height: u32, pitch: u32, bpp: u32, is_bgr: bool) {
    let driver = FramebufferDriver::new(fb_addr, width, height, pitch, bpp, is_bgr);
    *FRAMEBUFFER_DRIVER.lock() = Some(driver);
}

/// Get a reference to the global framebuffer driver
pub fn get() -> Option<&'static Mutex<Option<FramebufferDriver>>> {
    Some(&FRAMEBUFFER_DRIVER)
}
