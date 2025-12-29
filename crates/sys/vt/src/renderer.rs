/// VT Renderer - renders VT text buffer to framebuffer

use crate::vt::{Cell, Color, VirtualTerminal, VT_HEIGHT, VT_WIDTH};
use watos_terminal::renderer::FONT_8X16;

/// Framebuffer abstraction
pub trait Framebuffer {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn set_pixel(&mut self, x: u32, y: u32, color: Color);
    fn fill_rect(&mut self, x: u32, y: u32, width: u32, height: u32, color: Color);
}

/// Simple framebuffer implementation for kernel
pub struct KernelFramebuffer {
    addr: usize,
    width: u32,
    height: u32,
    pitch: u32,
    bpp: u32,
    is_bgr: bool,
}

impl KernelFramebuffer {
    pub fn new(addr: usize, width: u32, height: u32, pitch: u32, bpp: u32, is_bgr: bool) -> Self {
        KernelFramebuffer {
            addr,
            width,
            height,
            pitch,
            bpp,
            is_bgr,
        }
    }
}

impl Framebuffer for KernelFramebuffer {
    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn set_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x >= self.width || y >= self.height {
            return;
        }

        let offset = (y * self.pitch + x * (self.bpp / 8)) as usize;
        let ptr = (self.addr + offset) as *mut u8;

        unsafe {
            if self.is_bgr {
                *ptr.offset(0) = color.b;
                *ptr.offset(1) = color.g;
                *ptr.offset(2) = color.r;
            } else {
                *ptr.offset(0) = color.r;
                *ptr.offset(1) = color.g;
                *ptr.offset(2) = color.b;
            }
        }
    }

    fn fill_rect(&mut self, x: u32, y: u32, width: u32, height: u32, color: Color) {
        for dy in 0..height {
            for dx in 0..width {
                self.set_pixel(x + dx, y + dy, color);
            }
        }
    }
}

const CHAR_WIDTH: u32 = 8;
const CHAR_HEIGHT: u32 = 16;

/// VT Renderer
pub struct VTRenderer {
    char_width: u32,
    char_height: u32,
}

impl VTRenderer {
    pub fn new() -> Self {
        VTRenderer {
            char_width: CHAR_WIDTH,
            char_height: CHAR_HEIGHT,
        }
    }

    /// Render a VT to the framebuffer
    pub fn render<F: Framebuffer>(&self, fb: &mut F, vt: &VirtualTerminal) {
        for y in 0..VT_HEIGHT {
            for x in 0..VT_WIDTH {
                if let Some(cell) = vt.get_cell(x, y) {
                    self.render_cell(fb, x as u32, y as u32, &cell);
                }
            }
        }

        // Render cursor if visible (handles blinking)
        if vt.cursor_visible() {
            let (cursor_x, cursor_y) = vt.cursor();
            self.render_cursor(fb, cursor_x as u32, cursor_y as u32);
        }
    }

    /// Render a single cell
    fn render_cell<F: Framebuffer>(&self, fb: &mut F, grid_x: u32, grid_y: u32, cell: &Cell) {
        let pixel_x = grid_x * self.char_width;
        let pixel_y = grid_y * self.char_height;

        // Fill background
        fb.fill_rect(pixel_x, pixel_y, self.char_width, self.char_height, cell.bg);

        // Draw character glyph
        self.draw_glyph(fb, pixel_x, pixel_y, cell.ch, cell.fg);
    }

    /// Draw a character glyph
    fn draw_glyph<F: Framebuffer>(&self, fb: &mut F, x: u32, y: u32, ch: char, fg: Color) {
        let glyph_idx = (ch as usize) & 0xFF; // Mask to 0-255
        let glyph = &FONT_8X16[glyph_idx];

        for (row, &bits) in glyph.iter().enumerate() {
            for col in 0..8 {
                if (bits >> (7 - col)) & 1 != 0 {
                    fb.set_pixel(x + col, y + row as u32, fg);
                }
            }
        }
    }

    /// Render cursor (simple block cursor)
    fn render_cursor<F: Framebuffer>(&self, fb: &mut F, grid_x: u32, grid_y: u32) {
        let pixel_x = grid_x * self.char_width;
        let pixel_y = grid_y * self.char_height;

        // Draw cursor as inverted block (white)
        fb.fill_rect(
            pixel_x,
            pixel_y + self.char_height - 2,
            self.char_width,
            2,
            Color::WHITE,
        );
    }
}
