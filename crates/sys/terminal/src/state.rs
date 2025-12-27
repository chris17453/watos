//! Terminal state - cursor, colors, modes
//!
//! Tracks all terminal state that affects character rendering.

use crate::cell::CellFlags;
use crate::color::Color;

/// Terminal operating mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Normal operation
    Normal,
    /// Application cursor keys mode
    ApplicationCursor,
    /// Application keypad mode
    ApplicationKeypad,
}

/// Terminal state
pub struct TerminalState {
    // Cursor position
    pub cursor_x: i32,
    pub cursor_y: i32,

    // Dimensions
    pub width: i32,
    pub height: i32,

    // Colors
    pub fg: Color,
    pub bg: Color,
    pub default_fg: Color,
    pub default_bg: Color,

    // Attributes
    pub flags: CellFlags,

    // Modes
    pub mode: Mode,
    pub autowrap: bool,
    pub cursor_visible: bool,
    pub reverse_video: bool,
    pub origin_mode: bool,  // Cursor relative to scroll region

    // Scroll region
    pub scroll_top: i32,
    pub scroll_bottom: i32,

    // Saved cursor state
    saved_x: i32,
    saved_y: i32,
    saved_fg: Color,
    saved_bg: Color,
    saved_flags: CellFlags,

    // Pending wrap (cursor at end of line, next char wraps)
    pub pending_wrap: bool,

    // Tab stops (bit array, 1 bit per column, max 160 columns = 20 bytes)
    tab_stops: [u8; 20],
}

impl TerminalState {
    /// Create a new terminal state
    pub fn new(width: usize, height: usize, fg: Color, bg: Color) -> Self {
        let mut state = Self {
            cursor_x: 0,
            cursor_y: 0,
            width: width as i32,
            height: height as i32,
            fg,
            bg,
            default_fg: fg,
            default_bg: bg,
            flags: CellFlags::empty(),
            mode: Mode::Normal,
            autowrap: true,
            cursor_visible: true,
            reverse_video: false,
            origin_mode: false,
            scroll_top: 0,
            scroll_bottom: height as i32 - 1,
            saved_x: 0,
            saved_y: 0,
            saved_fg: fg,
            saved_bg: bg,
            saved_flags: CellFlags::empty(),
            pending_wrap: false,
            tab_stops: [0; 20],
        };

        // Set default tab stops every 8 columns
        state.reset_tab_stops();
        state
    }

    /// Reset terminal to initial state
    pub fn reset(&mut self) {
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.fg = self.default_fg;
        self.bg = self.default_bg;
        self.flags = CellFlags::empty();
        self.mode = Mode::Normal;
        self.autowrap = true;
        self.cursor_visible = true;
        self.reverse_video = false;
        self.origin_mode = false;
        self.scroll_top = 0;
        self.scroll_bottom = self.height - 1;
        self.pending_wrap = false;
        self.reset_tab_stops();
    }

    /// Resize terminal
    pub fn resize(&mut self, width: usize, height: usize) {
        self.width = width as i32;
        self.height = height as i32;
        self.scroll_bottom = self.height - 1;

        // Clamp cursor to new bounds
        self.cursor_x = self.cursor_x.min(self.width - 1).max(0);
        self.cursor_y = self.cursor_y.min(self.height - 1).max(0);
    }

    // === Cursor Movement ===

    fn clamp_cursor(&mut self) {
        self.cursor_x = self.cursor_x.clamp(0, self.width - 1);
        let top = if self.origin_mode { self.scroll_top } else { 0 };
        let bottom = if self.origin_mode { self.scroll_bottom } else { self.height - 1 };
        self.cursor_y = self.cursor_y.clamp(top, bottom);
    }

    pub fn cursor_up(&mut self, n: i32) {
        self.pending_wrap = false;
        self.cursor_y -= n;
        self.clamp_cursor();
    }

    pub fn cursor_down(&mut self, n: i32) {
        self.pending_wrap = false;
        self.cursor_y += n;
        self.clamp_cursor();
    }

    pub fn cursor_left(&mut self, n: i32) {
        self.pending_wrap = false;
        self.cursor_x -= n;
        self.clamp_cursor();
    }

    pub fn cursor_right(&mut self, n: i32) {
        self.pending_wrap = false;
        self.cursor_x += n;
        self.clamp_cursor();
    }

    pub fn cursor_to(&mut self, x: i32, y: i32) {
        self.pending_wrap = false;
        self.cursor_x = x;
        self.cursor_y = if self.origin_mode { y + self.scroll_top } else { y };
        self.clamp_cursor();
    }

    pub fn cursor_to_col(&mut self, x: i32) {
        self.pending_wrap = false;
        self.cursor_x = x;
        self.clamp_cursor();
    }

    pub fn carriage_return(&mut self) {
        self.pending_wrap = false;
        self.cursor_x = 0;
    }

    /// Handle newline - may trigger scroll
    pub fn newline(&mut self) -> bool {
        self.pending_wrap = false;
        self.cursor_y += 1;

        if self.cursor_y > self.scroll_bottom {
            self.cursor_y = self.scroll_bottom;
            return true; // Signal that scroll is needed
        }
        false
    }

    /// Handle advancing cursor after character (may wrap/scroll)
    pub fn advance_cursor(&mut self) -> bool {
        if self.pending_wrap {
            self.pending_wrap = false;
            self.cursor_x = 0;
            return self.newline();
        }

        self.cursor_x += 1;
        if self.cursor_x >= self.width {
            if self.autowrap {
                self.pending_wrap = true;
                self.cursor_x = self.width - 1;
            } else {
                self.cursor_x = self.width - 1;
            }
        }
        false
    }

    // === Cursor Save/Restore ===

    pub fn save_cursor(&mut self) {
        self.saved_x = self.cursor_x;
        self.saved_y = self.cursor_y;
        self.saved_fg = self.fg;
        self.saved_bg = self.bg;
        self.saved_flags = self.flags;
    }

    pub fn restore_cursor(&mut self) {
        self.cursor_x = self.saved_x;
        self.cursor_y = self.saved_y;
        self.fg = self.saved_fg;
        self.bg = self.saved_bg;
        self.flags = self.saved_flags;
        self.pending_wrap = false;
        self.clamp_cursor();
    }

    // === Scroll Region ===

    pub fn set_scroll_region(&mut self, top: i32, bottom: i32) {
        let top = top.clamp(0, self.height - 1);
        let bottom = bottom.clamp(top, self.height - 1);
        self.scroll_top = top;
        self.scroll_bottom = bottom;

        // Move cursor to home position
        if self.origin_mode {
            self.cursor_x = 0;
            self.cursor_y = top;
        } else {
            self.cursor_x = 0;
            self.cursor_y = 0;
        }
    }

    // === Tab Stops ===

    fn reset_tab_stops(&mut self) {
        self.tab_stops = [0; 20];
        // Set tab stop every 8 columns
        for col in (0..160).step_by(8) {
            self.set_tab_stop(col);
        }
    }

    pub fn set_tab_stop(&mut self, col: usize) {
        if col < 160 {
            self.tab_stops[col / 8] |= 1 << (col % 8);
        }
    }

    pub fn clear_tab_stop(&mut self, col: usize) {
        if col < 160 {
            self.tab_stops[col / 8] &= !(1 << (col % 8));
        }
    }

    pub fn clear_all_tab_stops(&mut self) {
        self.tab_stops = [0; 20];
    }

    pub fn is_tab_stop(&self, col: usize) -> bool {
        if col < 160 {
            (self.tab_stops[col / 8] & (1 << (col % 8))) != 0
        } else {
            false
        }
    }

    pub fn next_tab_stop(&self) -> i32 {
        for col in (self.cursor_x + 1) as usize..self.width as usize {
            if self.is_tab_stop(col) {
                return col as i32;
            }
        }
        self.width - 1
    }

    pub fn tab(&mut self) {
        self.pending_wrap = false;
        self.cursor_x = self.next_tab_stop();
    }

    // === Attributes ===

    pub fn reset_attributes(&mut self) {
        self.fg = self.default_fg;
        self.bg = self.default_bg;
        self.flags = CellFlags::empty();
        self.reverse_video = false;
    }

    pub fn set_bold(&mut self, on: bool) {
        if on {
            self.flags |= CellFlags::BOLD;
        } else {
            self.flags &= !CellFlags::BOLD;
        }
    }

    pub fn set_italic(&mut self, on: bool) {
        if on {
            self.flags |= CellFlags::ITALIC;
        } else {
            self.flags &= !CellFlags::ITALIC;
        }
    }

    pub fn set_underline(&mut self, on: bool) {
        if on {
            self.flags |= CellFlags::UNDERLINE;
        } else {
            self.flags &= !CellFlags::UNDERLINE;
        }
    }

    pub fn set_blink(&mut self, on: bool) {
        if on {
            self.flags |= CellFlags::BLINK;
        } else {
            self.flags &= !CellFlags::BLINK;
        }
    }

    pub fn set_reverse(&mut self, on: bool) {
        self.reverse_video = on;
        if on {
            self.flags |= CellFlags::REVERSE;
        } else {
            self.flags &= !CellFlags::REVERSE;
        }
    }

    pub fn set_strikethrough(&mut self, on: bool) {
        if on {
            self.flags |= CellFlags::STRIKETHROUGH;
        } else {
            self.flags &= !CellFlags::STRIKETHROUGH;
        }
    }
}
