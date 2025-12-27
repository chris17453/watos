//! Terminal - combines parser, grid, and state
//!
//! This is the main terminal emulator that processes input bytes
//! and updates the screen buffer.

use crate::cell::Cell;
use crate::color::{Color, color_256, ANSI_COLORS};
use crate::grid::Grid;
use crate::parser::{Parser, Event, csi_param};
use crate::state::TerminalState;

/// Terminal emulator
pub struct Terminal {
    /// Character grid
    pub grid: Grid,
    /// Terminal state (cursor, colors, modes)
    pub state: TerminalState,
    /// ANSI parser
    parser: Parser,
}

impl Terminal {
    /// Create a new terminal with given dimensions
    pub fn new(cols: usize, rows: usize) -> Self {
        let fg = Color::WHITE;
        let bg = Color::BLACK;

        Self {
            grid: Grid::new(cols, rows, fg, bg),
            state: TerminalState::new(cols, rows, fg, bg),
            parser: Parser::new(),
        }
    }

    /// Resize the terminal
    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.grid.resize(cols, rows);
        self.state.resize(cols, rows);
    }

    /// Get current dimensions
    pub fn size(&self) -> (usize, usize) {
        (self.grid.cols(), self.grid.rows())
    }

    /// Get cursor position
    pub fn cursor(&self) -> (usize, usize) {
        (self.state.cursor_x as usize, self.state.cursor_y as usize)
    }

    /// Check if cursor is visible
    pub fn cursor_visible(&self) -> bool {
        self.state.cursor_visible
    }

    /// Process a single byte of input
    pub fn process_byte(&mut self, byte: u8) {
        if let Some(event) = self.parser.advance(byte) {
            self.handle_event(event);
        }
    }

    /// Process a slice of bytes
    pub fn process_bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.process_byte(byte);
        }
    }

    /// Process a string
    pub fn write_str(&mut self, s: &str) {
        self.process_bytes(s.as_bytes());
    }

    /// Handle a parser event
    fn handle_event(&mut self, event: Event) {
        match event {
            Event::Print(ch) => self.print_char(ch),
            Event::Execute(byte) => self.execute(byte),
            Event::Csi { params, param_count, intermediate: _, final_byte, private } => {
                self.handle_csi(&params, param_count, final_byte, private);
            }
            Event::EscDispatch(byte) => self.handle_esc(byte),
            Event::Osc { command: _ } => {
                // OSC sequences (title, etc.) - ignore for now
            }
            Event::Charset { slot: _, charset: _ } => {
                // Charset selection - ignore for now
            }
        }
    }

    /// Print a character at current cursor position
    fn print_char(&mut self, ch: char) {
        let x = self.state.cursor_x as usize;
        let y = self.state.cursor_y as usize;

        // Create cell with current attributes
        let cell = Cell::with_flags(
            ch,
            self.state.fg,
            self.state.bg,
            self.state.flags,
        );

        self.grid.set(x, y, cell);

        // Advance cursor (may scroll)
        if self.state.advance_cursor() {
            self.scroll_up(1);
        }
    }

    /// Execute a control character
    fn execute(&mut self, byte: u8) {
        match byte {
            0x07 => {
                // BEL - bell (ignore or beep)
            }
            0x08 => {
                // BS - backspace
                self.state.cursor_left(1);
            }
            0x09 => {
                // HT - horizontal tab
                self.state.tab();
            }
            0x0A | 0x0B | 0x0C => {
                // LF, VT, FF - line feed (with implicit CR for Unix compatibility)
                self.state.carriage_return();
                if self.state.newline() {
                    self.scroll_up(1);
                }
            }
            0x0D => {
                // CR - carriage return
                self.state.carriage_return();
            }
            0x0E => {
                // SO - shift out (switch to G1 charset)
            }
            0x0F => {
                // SI - shift in (switch to G0 charset)
            }
            _ => {}
        }
    }

    /// Handle CSI sequence
    fn handle_csi(&mut self, params: &[i32; 16], count: usize, cmd: u8, private: bool) {
        if private {
            self.handle_csi_private(params, count, cmd);
            return;
        }

        match cmd {
            // Cursor movement
            b'A' => {
                // CUU - cursor up
                let n = csi_param(params, count, 0, 1);
                self.state.cursor_up(n);
            }
            b'B' => {
                // CUD - cursor down
                let n = csi_param(params, count, 0, 1);
                self.state.cursor_down(n);
            }
            b'C' => {
                // CUF - cursor forward (right)
                let n = csi_param(params, count, 0, 1);
                self.state.cursor_right(n);
            }
            b'D' => {
                // CUB - cursor back (left)
                let n = csi_param(params, count, 0, 1);
                self.state.cursor_left(n);
            }
            b'E' => {
                // CNL - cursor next line
                let n = csi_param(params, count, 0, 1);
                self.state.cursor_down(n);
                self.state.carriage_return();
            }
            b'F' => {
                // CPL - cursor previous line
                let n = csi_param(params, count, 0, 1);
                self.state.cursor_up(n);
                self.state.carriage_return();
            }
            b'G' | b'`' => {
                // CHA - cursor horizontal absolute
                let col = csi_param(params, count, 0, 1) - 1;
                self.state.cursor_to_col(col);
            }
            b'H' | b'f' => {
                // CUP - cursor position
                let row = csi_param(params, count, 0, 1) - 1;
                let col = csi_param(params, count, 1, 1) - 1;
                self.state.cursor_to(col, row);
            }
            b'd' => {
                // VPA - vertical position absolute
                let row = csi_param(params, count, 0, 1) - 1;
                self.state.cursor_to(self.state.cursor_x, row);
            }

            // Erase
            b'J' => {
                // ED - erase in display
                let mode = csi_param(params, count, 0, 0);
                self.erase_in_display(mode);
            }
            b'K' => {
                // EL - erase in line
                let mode = csi_param(params, count, 0, 0);
                self.erase_in_line(mode);
            }

            // Insert/Delete
            b'L' => {
                // IL - insert lines
                let n = csi_param(params, count, 0, 1) as usize;
                self.insert_lines(n);
            }
            b'M' => {
                // DL - delete lines
                let n = csi_param(params, count, 0, 1) as usize;
                self.delete_lines(n);
            }
            b'P' => {
                // DCH - delete characters
                let n = csi_param(params, count, 0, 1) as usize;
                self.delete_chars(n);
            }
            b'@' => {
                // ICH - insert characters
                let n = csi_param(params, count, 0, 1) as usize;
                self.insert_chars(n);
            }

            // Scroll
            b'S' => {
                // SU - scroll up
                let n = csi_param(params, count, 0, 1) as usize;
                self.scroll_up(n);
            }
            b'T' => {
                // SD - scroll down
                let n = csi_param(params, count, 0, 1) as usize;
                self.scroll_down(n);
            }

            // SGR - Select Graphic Rendition
            b'm' => {
                self.handle_sgr(params, count);
            }

            // Scroll region
            b'r' => {
                // DECSTBM - set top and bottom margins
                let top = csi_param(params, count, 0, 1) - 1;
                let bottom = csi_param(params, count, 1, self.state.height) - 1;
                self.state.set_scroll_region(top, bottom);
            }

            // Tab stops
            b'g' => {
                // TBC - tab clear
                let mode = csi_param(params, count, 0, 0);
                match mode {
                    0 => self.state.clear_tab_stop(self.state.cursor_x as usize),
                    3 => self.state.clear_all_tab_stops(),
                    _ => {}
                }
            }

            // Cursor save/restore
            b's' => {
                // SCP - save cursor position
                self.state.save_cursor();
            }
            b'u' => {
                // RCP - restore cursor position
                self.state.restore_cursor();
            }

            // Device status
            b'n' => {
                // DSR - device status report (ignore)
            }
            b'c' => {
                // DA - device attributes (ignore)
            }

            _ => {
                // Unknown CSI sequence
            }
        }
    }

    /// Handle private CSI sequences (ESC [ ?)
    fn handle_csi_private(&mut self, params: &[i32; 16], count: usize, cmd: u8) {
        let mode = csi_param(params, count, 0, 0);

        match cmd {
            b'h' => {
                // DECSET - set mode
                match mode {
                    1 => self.state.mode = crate::state::Mode::ApplicationCursor,
                    7 => self.state.autowrap = true,
                    25 => self.state.cursor_visible = true,
                    _ => {}
                }
            }
            b'l' => {
                // DECRST - reset mode
                match mode {
                    1 => self.state.mode = crate::state::Mode::Normal,
                    7 => self.state.autowrap = false,
                    25 => self.state.cursor_visible = false,
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// Handle ESC single-byte commands
    fn handle_esc(&mut self, byte: u8) {
        match byte {
            b'7' => self.state.save_cursor(),
            b'8' => self.state.restore_cursor(),
            b'c' => self.reset(),
            b'D' => {
                // IND - index (move down, scroll if at bottom)
                if self.state.newline() {
                    self.scroll_up(1);
                }
            }
            b'E' => {
                // NEL - next line
                self.state.carriage_return();
                if self.state.newline() {
                    self.scroll_up(1);
                }
            }
            b'H' => {
                // HTS - horizontal tab set
                self.state.set_tab_stop(self.state.cursor_x as usize);
            }
            b'M' => {
                // RI - reverse index (move up, scroll if at top)
                if self.state.cursor_y <= self.state.scroll_top {
                    self.scroll_down(1);
                } else {
                    self.state.cursor_up(1);
                }
            }
            _ => {}
        }
    }

    /// Handle SGR (colors and attributes)
    fn handle_sgr(&mut self, params: &[i32; 16], count: usize) {
        if count == 0 {
            self.state.reset_attributes();
            return;
        }

        let mut i = 0;
        while i < count {
            let p = params[i];
            match p {
                0 => self.state.reset_attributes(),
                1 => self.state.set_bold(true),
                3 => self.state.set_italic(true),
                4 => self.state.set_underline(true),
                5 | 6 => self.state.set_blink(true),
                7 => self.state.set_reverse(true),
                9 => self.state.set_strikethrough(true),
                21 | 22 => self.state.set_bold(false),
                23 => self.state.set_italic(false),
                24 => self.state.set_underline(false),
                25 => self.state.set_blink(false),
                27 => self.state.set_reverse(false),
                29 => self.state.set_strikethrough(false),

                // Standard foreground colors
                30..=37 => {
                    self.state.fg = ANSI_COLORS[(p - 30) as usize];
                }
                38 => {
                    // Extended foreground color
                    if i + 1 < count {
                        match params[i + 1] {
                            5 if i + 2 < count => {
                                // 256-color mode
                                self.state.fg = color_256(params[i + 2] as u8);
                                i += 2;
                            }
                            2 if i + 4 < count => {
                                // True color mode
                                self.state.fg = Color::rgb(
                                    params[i + 2] as u8,
                                    params[i + 3] as u8,
                                    params[i + 4] as u8,
                                );
                                i += 4;
                            }
                            _ => {}
                        }
                    }
                }
                39 => {
                    // Default foreground
                    self.state.fg = self.state.default_fg;
                }

                // Standard background colors
                40..=47 => {
                    self.state.bg = ANSI_COLORS[(p - 40) as usize];
                }
                48 => {
                    // Extended background color
                    if i + 1 < count {
                        match params[i + 1] {
                            5 if i + 2 < count => {
                                // 256-color mode
                                self.state.bg = color_256(params[i + 2] as u8);
                                i += 2;
                            }
                            2 if i + 4 < count => {
                                // True color mode
                                self.state.bg = Color::rgb(
                                    params[i + 2] as u8,
                                    params[i + 3] as u8,
                                    params[i + 4] as u8,
                                );
                                i += 4;
                            }
                            _ => {}
                        }
                    }
                }
                49 => {
                    // Default background
                    self.state.bg = self.state.default_bg;
                }

                // Bright foreground colors
                90..=97 => {
                    self.state.fg = ANSI_COLORS[(p - 90 + 8) as usize];
                }
                // Bright background colors
                100..=107 => {
                    self.state.bg = ANSI_COLORS[(p - 100 + 8) as usize];
                }

                _ => {}
            }
            i += 1;
        }
    }

    /// Erase in display
    fn erase_in_display(&mut self, mode: i32) {
        let cy = self.state.cursor_y as usize;
        let cx = self.state.cursor_x as usize;

        match mode {
            0 => {
                // From cursor to end
                self.grid.clear_row_from(cy, cx);
                for row in (cy + 1)..self.grid.rows() {
                    self.grid.clear_row(row);
                }
            }
            1 => {
                // From start to cursor
                for row in 0..cy {
                    self.grid.clear_row(row);
                }
                self.grid.clear_row_to(cy, cx);
            }
            2 | 3 => {
                // Entire screen
                self.grid.clear();
            }
            _ => {}
        }
    }

    /// Erase in line
    fn erase_in_line(&mut self, mode: i32) {
        let cy = self.state.cursor_y as usize;
        let cx = self.state.cursor_x as usize;

        match mode {
            0 => self.grid.clear_row_from(cy, cx),
            1 => self.grid.clear_row_to(cy, cx),
            2 => self.grid.clear_row(cy),
            _ => {}
        }
    }

    /// Scroll up n lines in scroll region
    fn scroll_up(&mut self, n: usize) {
        let top = self.state.scroll_top as usize;
        let bottom = (self.state.scroll_bottom + 1) as usize;
        self.grid.scroll_up(top, bottom, n);
    }

    /// Scroll down n lines in scroll region
    fn scroll_down(&mut self, n: usize) {
        let top = self.state.scroll_top as usize;
        let bottom = (self.state.scroll_bottom + 1) as usize;
        self.grid.scroll_down(top, bottom, n);
    }

    /// Insert n blank lines at cursor position
    fn insert_lines(&mut self, n: usize) {
        let top = self.state.cursor_y as usize;
        let bottom = (self.state.scroll_bottom + 1) as usize;
        self.grid.scroll_down(top, bottom, n);
    }

    /// Delete n lines at cursor position
    fn delete_lines(&mut self, n: usize) {
        let top = self.state.cursor_y as usize;
        let bottom = (self.state.scroll_bottom + 1) as usize;
        self.grid.scroll_up(top, bottom, n);
    }

    /// Insert n blank characters at cursor
    fn insert_chars(&mut self, _n: usize) {
        // TODO: Implement character insertion
    }

    /// Delete n characters at cursor
    fn delete_chars(&mut self, _n: usize) {
        // TODO: Implement character deletion
    }

    /// Reset terminal to initial state
    pub fn reset(&mut self) {
        self.state.reset();
        self.grid.clear();
        self.parser.reset();
    }

    /// Clear the screen
    pub fn clear(&mut self) {
        self.grid.clear();
        self.state.cursor_to(0, 0);
    }
}
