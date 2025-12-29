/// Virtual Terminal structure
/// Represents a single VT (like /dev/tty1)
/// Uses the full watos-terminal emulator for ANSI/VT100 support

use watos_terminal::terminal::Terminal;
use watos_terminal::color::Color as TermColor;
use watos_terminal::cell::Cell as TermCell;

// For 1280x800 with 8x16 font: 160 cols x 50 rows
// This fits common resolutions better than 80x25
pub const VT_WIDTH: usize = 160;
pub const VT_HEIGHT: usize = 50;

// Re-export Color for compatibility
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0 };
    pub const WHITE: Color = Color { r: 255, g: 255, b: 255 };
    pub const RED: Color = Color { r: 255, g: 0, b: 0 };
    pub const GREEN: Color = Color { r: 0, g: 255, b: 0 };
    pub const BLUE: Color = Color { r: 0, g: 0, b: 255 };
    pub const YELLOW: Color = Color { r: 255, g: 255, b: 0 };
    pub const CYAN: Color = Color { r: 0, g: 255, b: 255 };
    pub const MAGENTA: Color = Color { r: 255, g: 0, b: 255 };
    pub const GRAY: Color = Color { r: 128, g: 128, b: 128 };
    pub const LIGHT_GRAY: Color = Color { r: 192, g: 192, b: 192 };
}

impl From<TermColor> for Color {
    fn from(tc: TermColor) -> Self {
        Color { r: tc.r(), g: tc.g(), b: tc.b() }
    }
}

// Re-export Cell for compatibility
#[derive(Debug, Clone, Copy)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            ch: ' ',
            fg: Color::LIGHT_GRAY,
            bg: Color::BLACK,
        }
    }
}

extern crate alloc;
use alloc::boxed::Box;

pub struct VirtualTerminal {
    /// Full terminal emulator with ANSI parsing (boxed to avoid stack overflow)
    terminal: Box<Terminal>,

    /// Is this VT active (visible on screen)?
    active: bool,

    /// Needs redraw?
    dirty: bool,

    /// VT number (1-based, like /dev/tty1)
    vt_num: usize,

    /// Cursor blink state (for rendering)
    cursor_blink_on: bool,

    /// Tick counter for cursor blinking
    blink_ticks: u32,
}

impl VirtualTerminal {
    pub fn new(vt_num: usize) -> Self {
        VirtualTerminal {
            terminal: Box::new(Terminal::new(VT_WIDTH, VT_HEIGHT)),
            active: false,
            dirty: true,
            vt_num,
            cursor_blink_on: true,
            blink_ticks: 0,
        }
    }

    /// Write bytes to the VT (processes ANSI escape sequences)
    pub fn write(&mut self, data: &[u8]) {
        self.terminal.process_bytes(data);
        self.dirty = true;
    }

    /// Clear the screen
    pub fn clear(&mut self) {
        // Send ANSI clear screen sequence
        self.write(b"\x1b[2J\x1b[H");
    }

    /// Get cursor position
    pub fn cursor(&self) -> (usize, usize) {
        self.terminal.cursor()
    }

    /// Get cell at position (converts from terminal Cell to our Cell)
    pub fn get_cell(&self, x: usize, y: usize) -> Option<Cell> {
        if x < VT_WIDTH && y < VT_HEIGHT {
            let term_cell = self.terminal.grid.get(x, y)?;
            Some(Cell {
                ch: term_cell.ch,
                fg: Color::from(term_cell.fg),
                bg: Color::from(term_cell.bg),
            })
        } else {
            None
        }
    }

    /// Check if cursor should be visible
    pub fn cursor_visible(&self) -> bool {
        self.terminal.cursor_visible() && self.cursor_blink_on
    }

    /// Update cursor blink (call this periodically, e.g., every timer tick)
    pub fn tick_cursor(&mut self) {
        self.blink_ticks += 1;
        // Blink every 30 ticks (~500ms at 60Hz)
        if self.blink_ticks >= 30 {
            self.cursor_blink_on = !self.cursor_blink_on;
            self.blink_ticks = 0;
            self.dirty = true;
        }
    }

    /// Set active state
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
        if active {
            self.dirty = true; // Force redraw when switching to this VT
        }
    }

    /// Is this VT active?
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Is this VT dirty (needs redraw)?
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Clear dirty flag
    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    /// Get VT number
    pub fn vt_num(&self) -> usize {
        self.vt_num
    }
}
