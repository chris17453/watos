//! ASCII terminal graphics backend
//!
//! Platform-agnostic ASCII rendering for text-based graphics output.

#[cfg(not(feature = "std"))]
use alloc::{vec, vec::Vec, format};

use crate::error::{Error, Result};
use crate::graphics_backend::GraphicsBackend;

/// ASCII-based graphics backend that renders to terminal/console
pub struct AsciiBackend {
    width: usize,
    height: usize,
    buffer: Vec<Vec<char>>,
    cursor_x: usize,
    cursor_y: usize,
    fg_color: u8,
    bg_color: u8,
}

impl AsciiBackend {
    pub fn new(width: usize, height: usize) -> Self {
        AsciiBackend {
            width,
            height,
            buffer: vec![vec![' '; width]; height],
            cursor_x: 0,
            cursor_y: 0,
            fg_color: 7,
            bg_color: 0,
        }
    }

    /// Get the internal buffer for custom rendering
    pub fn get_buffer(&self) -> &Vec<Vec<char>> {
        &self.buffer
    }
}

impl GraphicsBackend for AsciiBackend {
    fn pset(&mut self, x: i32, y: i32, color: u8) -> Result<()> {
        if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
            return Ok(()); // Silently ignore out-of-bounds
        }

        // Color 0 is background (don't draw), others draw '#'
        if color == 0 {
            self.buffer[y as usize][x as usize] = ' ';
        } else {
            self.buffer[y as usize][x as usize] = '#';
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
        self.buffer = vec![vec![' '; self.width]; self.height];
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
            self.fg_color = foreground;
        }
        if let Some(background) = bg {
            self.bg_color = background;
        }
    }

    fn display(&mut self) {
        // Platform-specific display implementation
        #[cfg(feature = "std")]
        {
            println!("\n{}", "=".repeat(self.width + 2));
            for row in &self.buffer {
                print!("|");
                for &ch in row {
                    print!("{}", ch);
                }
                println!("|");
            }
            println!("{}", "=".repeat(self.width + 2));
        }

        #[cfg(not(feature = "std"))]
        {
            // For WATOS, we'll use syscalls to write to console
            // This is a placeholder - actual implementation will use platform module
            // The buffer can be read by the WATOS graphics backend
        }
    }

    fn get_size(&self) -> (usize, usize) {
        (self.height, self.width)
    }

    fn get_cursor(&self) -> (usize, usize) {
        (self.cursor_y, self.cursor_x)
    }
}
