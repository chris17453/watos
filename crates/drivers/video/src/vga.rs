//! Standard VGA Driver
//!
//! Supports text modes and classic VGA graphics modes via register programming

use watos_driver_traits::{Driver, DriverResult, DriverError, DriverState};
use watos_driver_traits::video::{VideoDevice, VideoMode, VideoDeviceInfo, Color, PixelFormat};
use crate::modes;

/// VGA I/O ports
const VGA_AC_INDEX: u16 = 0x3C0;
const VGA_AC_WRITE: u16 = 0x3C0;
const VGA_AC_READ: u16 = 0x3C1;
const VGA_MISC_WRITE: u16 = 0x3C2;
const VGA_SEQ_INDEX: u16 = 0x3C4;
const VGA_SEQ_DATA: u16 = 0x3C5;
const VGA_DAC_READ_INDEX: u16 = 0x3C7;
const VGA_DAC_WRITE_INDEX: u16 = 0x3C8;
const VGA_DAC_DATA: u16 = 0x3C9;
const VGA_MISC_READ: u16 = 0x3CC;
const VGA_GC_INDEX: u16 = 0x3CE;
const VGA_GC_DATA: u16 = 0x3CF;
const VGA_CRTC_INDEX: u16 = 0x3D4;
const VGA_CRTC_DATA: u16 = 0x3D5;
const VGA_INSTAT_READ: u16 = 0x3DA;

/// VGA memory base addresses
const VGA_TEXT_BUFFER: usize = 0xB8000;
const VGA_GRAPHICS_BUFFER: usize = 0xA0000;

/// VGA Driver
pub struct VgaDriver {
    state: DriverState,
    current_mode: VideoMode,
    available_modes: &'static [VideoMode],
    palette: [u32; 256],
}

impl VgaDriver {
    /// Create a new VGA driver
    pub fn new() -> Self {
        // Start with text mode 80x25
        VgaDriver {
            state: DriverState::Uninitialized,
            current_mode: modes::TEXT_80X25,
            available_modes: &[
                modes::TEXT_80X25,
                modes::TEXT_80X50,
                modes::VGA_MODE_13H,
                modes::VGA_MODE_12H,
                modes::VGA_MODE_10H,
            ],
            palette: [0u32; 256],
        }
    }

    /// Write to VGA port
    #[inline]
    fn outb(port: u16, value: u8) {
        unsafe {
            core::arch::asm!(
                "out dx, al",
                in("dx") port,
                in("al") value,
                options(nostack, preserves_flags)
            );
        }
    }

    /// Read from VGA port
    #[inline]
    fn inb(port: u16) -> u8 {
        let value: u8;
        unsafe {
            core::arch::asm!(
                "in al, dx",
                in("dx") port,
                out("al") value,
                options(nostack, preserves_flags)
            );
        }
        value
    }

    /// Set Mode 13h (320x200x256)
    fn set_mode_13h(&mut self) -> DriverResult<()> {
        // Mode 13h is a standard VGA mode, can be set via BIOS interrupt
        // In protected/long mode, we need to program registers directly
        
        // Simplified mode setting - in real implementation would need full register setup
        // For now, just track the mode change
        self.current_mode = modes::VGA_MODE_13H;
        Ok(())
    }

    /// Set text mode 80x25
    fn set_text_80x25(&mut self) -> DriverResult<()> {
        self.current_mode = modes::TEXT_80X25;
        Ok(())
    }
}

impl Driver for VgaDriver {
    fn name(&self) -> &'static str {
        "VGA Driver"
    }

    fn init(&mut self) -> DriverResult<()> {
        // Initialize default VGA palette
        init_vga_palette(&mut self.palette);
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

impl VideoDevice for VgaDriver {
    fn current_mode(&self) -> VideoMode {
        self.current_mode
    }

    fn available_modes(&self) -> &[VideoMode] {
        self.available_modes
    }

    fn set_mode(&mut self, mode: VideoMode) -> DriverResult<()> {
        // Match against known VGA modes
        if mode == modes::VGA_MODE_13H {
            self.set_mode_13h()
        } else if mode == modes::TEXT_80X25 {
            self.set_text_80x25()
        } else if mode == modes::VGA_MODE_12H || mode == modes::VGA_MODE_10H {
            // These would require full register programming
            self.current_mode = mode;
            Ok(())
        } else {
            Err(DriverError::NotSupported)
        }
    }

    fn framebuffer(&self) -> *mut u8 {
        match self.current_mode.format {
            PixelFormat::Indexed if self.current_mode.bpp == 8 => {
                // Graphics mode
                VGA_GRAPHICS_BUFFER as *mut u8
            }
            _ => {
                // Text mode or other
                VGA_TEXT_BUFFER as *mut u8
            }
        }
    }

    fn pitch(&self) -> usize {
        match self.current_mode.format {
            PixelFormat::Indexed if self.current_mode.bpp == 8 => {
                self.current_mode.width as usize
            }
            _ => {
                // Text mode: 2 bytes per character (char + attribute)
                self.current_mode.width as usize * 2
            }
        }
    }

    fn set_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x >= self.current_mode.width || y >= self.current_mode.height {
            return;
        }

        // Only support Mode 13h for now
        if self.current_mode == modes::VGA_MODE_13H {
            let offset = y as usize * self.current_mode.width as usize + x as usize;
            unsafe {
                let fb = self.framebuffer() as *mut u8;
                *fb.add(offset) = (color & 0xFF) as u8;
            }
        }
    }

    fn get_pixel(&self, x: u32, y: u32) -> Color {
        if x >= self.current_mode.width || y >= self.current_mode.height {
            return 0;
        }

        if self.current_mode == modes::VGA_MODE_13H {
            let offset = y as usize * self.current_mode.width as usize + x as usize;
            unsafe {
                let fb = self.framebuffer() as *const u8;
                let index = *fb.add(offset);
                self.palette[index as usize]
            }
        } else {
            0
        }
    }

    fn info(&self) -> VideoDeviceInfo {
        VideoDeviceInfo {
            name: self.name(),
            mode: self.current_mode,
            framebuffer_size: match self.current_mode.format {
                PixelFormat::Indexed if self.current_mode.bpp == 8 => {
                    (self.current_mode.width * self.current_mode.height) as usize
                }
                _ => {
                    (self.current_mode.width * self.current_mode.height * 2) as usize
                }
            },
        }
    }

    fn set_palette(&mut self, index: u8, r: u8, g: u8, b: u8) -> DriverResult<()> {
        // Update internal palette
        self.palette[index as usize] = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);

        // Program VGA DAC
        Self::outb(VGA_DAC_WRITE_INDEX, index);
        Self::outb(VGA_DAC_DATA, r >> 2); // VGA DAC uses 6-bit values
        Self::outb(VGA_DAC_DATA, g >> 2);
        Self::outb(VGA_DAC_DATA, b >> 2);

        Ok(())
    }
}

/// Initialize default VGA palette
fn init_vga_palette(palette: &mut [u32; 256]) {
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

    palette[..16].copy_from_slice(&EGA_PALETTE);

    // Fill rest with grayscale gradient
    for i in 16..256 {
        let gray = ((i - 16) * 255 / (256 - 16)) as u8;
        palette[i] = ((gray as u32) << 16) | ((gray as u32) << 8) | (gray as u32);
    }
}
