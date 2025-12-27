//! Terminal cell representation
//!
//! Each cell in the terminal grid contains a character and its attributes.

use bitflags::bitflags;
use crate::color::Color;

bitflags! {
    /// Cell attribute flags
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct CellFlags: u8 {
        const BOLD       = 0b0000_0001;
        const ITALIC     = 0b0000_0010;
        const UNDERLINE  = 0b0000_0100;
        const REVERSE    = 0b0000_1000;
        const BLINK      = 0b0001_0000;
        const STRIKETHROUGH = 0b0010_0000;
        const WIDE_CHAR  = 0b0100_0000;  // Primary cell of a wide character
        const WIDE_SPACER = 0b1000_0000; // Continuation of a wide character
    }
}

/// A single cell in the terminal grid
#[derive(Debug, Clone, Copy)]
pub struct Cell {
    /// The character in this cell
    pub ch: char,
    /// Foreground color
    pub fg: Color,
    /// Background color
    pub bg: Color,
    /// Attribute flags
    pub flags: CellFlags,
}

impl Cell {
    /// Create a new cell with the given character and colors
    pub const fn new(ch: char, fg: Color, bg: Color) -> Self {
        Self {
            ch,
            fg,
            bg,
            flags: CellFlags::empty(),
        }
    }

    /// Create a new cell with flags
    pub const fn with_flags(ch: char, fg: Color, bg: Color, flags: CellFlags) -> Self {
        Self { ch, fg, bg, flags }
    }

    /// Create an empty (space) cell with given colors
    pub const fn empty(fg: Color, bg: Color) -> Self {
        Self::new(' ', fg, bg)
    }

    /// Create a cell with default colors (white on black)
    pub const fn default_colors(ch: char) -> Self {
        Self::new(ch, Color::WHITE, Color::BLACK)
    }

    /// Check if this cell is empty (space with no special attributes)
    pub fn is_empty(&self) -> bool {
        self.ch == ' ' && self.flags.is_empty()
    }

    /// Check if this is a wide character primary cell
    pub fn is_wide(&self) -> bool {
        self.flags.contains(CellFlags::WIDE_CHAR)
    }

    /// Check if this is a wide character spacer
    pub fn is_wide_spacer(&self) -> bool {
        self.flags.contains(CellFlags::WIDE_SPACER)
    }

    /// Get effective foreground color (handles reverse video)
    pub fn effective_fg(&self) -> Color {
        if self.flags.contains(CellFlags::REVERSE) {
            self.bg
        } else {
            self.fg
        }
    }

    /// Get effective background color (handles reverse video)
    pub fn effective_bg(&self) -> Color {
        if self.flags.contains(CellFlags::REVERSE) {
            self.fg
        } else {
            self.bg
        }
    }

    /// Reset to empty with given default colors
    pub fn reset(&mut self, fg: Color, bg: Color) {
        self.ch = ' ';
        self.fg = fg;
        self.bg = bg;
        self.flags = CellFlags::empty();
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self::empty(Color::WHITE, Color::BLACK)
    }
}

impl PartialEq for Cell {
    fn eq(&self, other: &Self) -> bool {
        self.ch == other.ch
            && self.fg == other.fg
            && self.bg == other.bg
            && self.flags == other.flags
    }
}

impl Eq for Cell {}
