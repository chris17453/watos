/// Virtual Terminal structure
/// Represents a single VT (like /dev/tty1)

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

// For 1280x800 with 8x16 font: 160 cols x 50 rows
// This fits common resolutions better than 80x25
pub const VT_WIDTH: usize = 160;
pub const VT_HEIGHT: usize = 50;

pub struct VirtualTerminal {
    /// Text buffer (character grid)
    cells: [[Cell; VT_WIDTH]; VT_HEIGHT],

    /// Cursor position
    cursor_x: usize,
    cursor_y: usize,

    /// Current colors
    current_fg: Color,
    current_bg: Color,

    /// Is this VT active (visible on screen)?
    active: bool,

    /// Needs redraw?
    dirty: bool,

    /// VT number (1-based, like /dev/tty1)
    vt_num: usize,
}

impl VirtualTerminal {
    pub fn new(vt_num: usize) -> Self {
        VirtualTerminal {
            cells: [[Cell::default(); VT_WIDTH]; VT_HEIGHT],
            cursor_x: 0,
            cursor_y: 0,
            current_fg: Color::LIGHT_GRAY,
            current_bg: Color::BLACK,
            active: false,
            dirty: true,
            vt_num,
        }
    }

    /// Write a byte to the VT (processes control characters)
    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.newline(),
            b'\r' => self.cursor_x = 0,
            b'\t' => {
                // Tab to next 8-column boundary
                self.cursor_x = (self.cursor_x + 8) & !7;
                if self.cursor_x >= VT_WIDTH {
                    self.newline();
                }
            }
            b'\x08' => {
                // Backspace
                if self.cursor_x > 0 {
                    self.cursor_x -= 1;
                }
            }
            0x20..=0x7E => {
                // Printable ASCII
                self.put_char(byte as char);
            }
            _ => {
                // Ignore other control characters for now
            }
        }
        self.dirty = true;
    }

    /// Write multiple bytes to the VT
    pub fn write(&mut self, data: &[u8]) {
        for &byte in data {
            self.write_byte(byte);
        }
    }

    /// Put a character at the current cursor position
    fn put_char(&mut self, ch: char) {
        if self.cursor_x >= VT_WIDTH {
            self.newline();
        }

        self.cells[self.cursor_y][self.cursor_x] = Cell {
            ch,
            fg: self.current_fg,
            bg: self.current_bg,
        };

        self.cursor_x += 1;
        if self.cursor_x >= VT_WIDTH {
            self.newline();
        }
    }

    /// Move to next line (scroll if needed)
    fn newline(&mut self) {
        self.cursor_x = 0;
        self.cursor_y += 1;

        if self.cursor_y >= VT_HEIGHT {
            self.scroll_up();
            self.cursor_y = VT_HEIGHT - 1;
        }
    }

    /// Scroll the screen up by one line
    fn scroll_up(&mut self) {
        for y in 1..VT_HEIGHT {
            self.cells[y - 1] = self.cells[y];
        }
        // Clear last line
        self.cells[VT_HEIGHT - 1] = [Cell::default(); VT_WIDTH];
    }

    /// Clear the screen
    pub fn clear(&mut self) {
        self.cells = [[Cell::default(); VT_WIDTH]; VT_HEIGHT];
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.dirty = true;
    }

    /// Get cursor position
    pub fn cursor(&self) -> (usize, usize) {
        (self.cursor_x, self.cursor_y)
    }

    /// Get cell at position
    pub fn get_cell(&self, x: usize, y: usize) -> Option<Cell> {
        if x < VT_WIDTH && y < VT_HEIGHT {
            Some(self.cells[y][x])
        } else {
            None
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

    /// Set foreground color
    pub fn set_fg_color(&mut self, color: Color) {
        self.current_fg = color;
    }

    /// Set background color
    pub fn set_bg_color(&mut self, color: Color) {
        self.current_bg = color;
    }
}
