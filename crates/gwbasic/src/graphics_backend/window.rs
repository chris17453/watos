//! GUI window graphics backend using minifb

use crate::error::{Error, Result};
use crate::graphics_backend::GraphicsBackend;
use minifb::{Window, WindowOptions};
use std::time::Duration;

/// Color palette for GW-BASIC (EGA/CGA colors)
const PALETTE: [u32; 16] = [
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

/// GUI window backend that renders in a real window
pub struct WindowBackend {
    window: Window,
    buffer: Vec<u32>,
    width: usize,
    height: usize,
    cursor_x: usize,
    cursor_y: usize,
    fg_color: u8,
    bg_color: u8,
    _scale: usize, // Pixel scaling factor (stored for potential future use)
}

impl WindowBackend {
    pub fn new(width: usize, height: usize) -> Result<Self> {
        Self::new_with_scale(width, height, 2)
    }

    pub fn new_with_scale(width: usize, height: usize, scale: usize) -> Result<Self> {
        let mut window = Window::new(
            "GW-BASIC Graphics",
            width * scale,
            height * scale,
            WindowOptions::default(),
        )
        .map_err(|e| Error::RuntimeError(format!("Failed to create window: {}", e)))?;

        // Limit update rate
        window.limit_update_rate(Some(Duration::from_micros(16600))); // ~60 FPS

        let buffer = vec![PALETTE[0]; width * height];

        Ok(WindowBackend {
            window,
            buffer,
            width,
            height,
            cursor_x: 0,
            cursor_y: 0,
            fg_color: 7,
            bg_color: 0,
            _scale: scale,
        })
    }

    fn set_pixel(&mut self, x: usize, y: usize, color: u32) {
        if x < self.width && y < self.height {
            self.buffer[y * self.width + x] = color;
        }
    }

    fn get_color(&self, color_index: u8) -> u32 {
        PALETTE[(color_index as usize) % PALETTE.len()]
    }
}

impl GraphicsBackend for WindowBackend {
    fn pset(&mut self, x: i32, y: i32, color: u8) -> Result<()> {
        if x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height {
            let pixel_color = self.get_color(color);
            self.set_pixel(x as usize, y as usize, pixel_color);
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
        let bg = self.get_color(self.bg_color);
        self.buffer.fill(bg);
        self.cursor_x = 0;
        self.cursor_y = 0;
    }

    fn locate(&mut self, row: usize, col: usize) -> Result<()> {
        if row >= self.height || col >= self.width {
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
            self.fg_color = foreground % 16;
        }
        if let Some(background) = bg {
            self.bg_color = background % 16;
        }
    }

    fn display(&mut self) {
        // Update the window with the buffer
        self.update().ok();
    }

    fn get_size(&self) -> (usize, usize) {
        (self.height, self.width)
    }

    fn get_cursor(&self) -> (usize, usize) {
        (self.cursor_y, self.cursor_x)
    }

    fn should_close(&self) -> bool {
        // Close if window X button clicked or any key is pressed
        !self.window.is_open() || !self.window.get_keys().is_empty()
    }

    fn update(&mut self) -> Result<()> {
        // Update window with scaled buffer
        self.window
            .update_with_buffer(&self.buffer, self.width, self.height)
            .map_err(|e| Error::RuntimeError(format!("Failed to update window: {}", e)))?;
        Ok(())
    }
}
