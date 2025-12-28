//! Console Manager - virtual console support
//!
//! Manages multiple virtual terminals (like Linux tty1-tty12)
//! with Alt+Fn switching.

use crate::cell::Cell;
use crate::framebuffer::Framebuffer;
use crate::keyboard::{Keyboard, KeyEvent, KeyCode, Modifiers};
use crate::renderer::Renderer;
use crate::terminal::Terminal;

/// Maximum number of virtual consoles
pub const MAX_CONSOLES: usize = 1;

/// Console manager
pub struct ConsoleManager {
    /// Virtual terminals
    terminals: [Option<Terminal>; MAX_CONSOLES],
    /// Currently active console (0-11)
    active: usize,
    /// Keyboard handler
    keyboard: Keyboard,
    /// Renderer
    renderer: Renderer,
    /// Terminal dimensions
    cols: usize,
    rows: usize,
    /// Number of initialized consoles
    console_count: usize,
}

/// Helper constant for array initialization
const NONE_TERMINAL: Option<Terminal> = None;

impl ConsoleManager {
    /// Create a new console manager with given dimensions
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            terminals: [NONE_TERMINAL; MAX_CONSOLES],
            active: 0,
            keyboard: Keyboard::new(),
            renderer: Renderer::new(),
            cols,
            rows,
            console_count: 0,
        }
    }

    /// Initialize n consoles (call after construction)
    pub fn init_consoles(&mut self, count: usize) {
        let count = count.min(MAX_CONSOLES);
        for i in 0..count {
            self.terminals[i] = Some(Terminal::new(self.cols, self.rows));
        }
        self.console_count = count;
    }

    /// Get the active terminal
    pub fn active_terminal(&self) -> Option<&Terminal> {
        self.terminals[self.active].as_ref()
    }

    /// Get the active terminal mutably
    pub fn active_terminal_mut(&mut self) -> Option<&mut Terminal> {
        self.terminals[self.active].as_mut()
    }

    /// Get a specific terminal
    pub fn terminal(&self, index: usize) -> Option<&Terminal> {
        if index < MAX_CONSOLES {
            self.terminals[index].as_ref()
        } else {
            None
        }
    }

    /// Get a specific terminal mutably
    pub fn terminal_mut(&mut self, index: usize) -> Option<&mut Terminal> {
        if index < MAX_CONSOLES {
            self.terminals[index].as_mut()
        } else {
            None
        }
    }

    /// Switch to a different console
    pub fn switch_to(&mut self, index: usize) {
        if index < self.console_count && self.terminals[index].is_some() {
            self.active = index;
            // Mark the new console for full redraw
            if let Some(term) = &mut self.terminals[index] {
                term.grid.mark_all_dirty();
            }
        }
    }

    /// Get current console index
    pub fn active_index(&self) -> usize {
        self.active
    }

    /// Process a keyboard scancode
    /// Returns true if the key was handled (console switch), false otherwise
    pub fn process_scancode(&mut self, scancode: u8) -> Option<KeyEvent> {
        let event = self.keyboard.process_scancode(scancode)?;

        // Check for console switching (Alt+Fn)
        if event.pressed && event.modifiers.contains(Modifiers::ALT) {
            let console_idx = match event.key {
                KeyCode::F1 => Some(0),
                KeyCode::F2 => Some(1),
                KeyCode::F3 => Some(2),
                KeyCode::F4 => Some(3),
                KeyCode::F5 => Some(4),
                KeyCode::F6 => Some(5),
                KeyCode::F7 => Some(6),
                KeyCode::F8 => Some(7),
                KeyCode::F9 => Some(8),
                KeyCode::F10 => Some(9),
                KeyCode::F11 => Some(10),
                KeyCode::F12 => Some(11),
                _ => None,
            };

            if let Some(idx) = console_idx {
                self.switch_to(idx);
                return None; // Consumed the event
            }
        }

        Some(event)
    }

    /// Write data to the active console
    pub fn write(&mut self, data: &[u8]) {
        if let Some(term) = self.active_terminal_mut() {
            term.process_bytes(data);
        }
    }

    /// Write a string to the active console
    pub fn write_str(&mut self, s: &str) {
        self.write(s.as_bytes());
    }

    /// Render the active console to framebuffer
    pub fn render<F: Framebuffer>(&mut self, fb: &mut F) {
        if let Some(term) = &mut self.terminals[self.active] {
            let cursor = if term.cursor_visible() {
                Some(term.cursor())
            } else {
                None
            };
            self.renderer.render_dirty(fb, &mut term.grid, cursor);
        }
    }

    /// Force full redraw
    pub fn invalidate(&mut self) {
        if let Some(term) = self.active_terminal_mut() {
            term.grid.mark_all_dirty();
        }
    }

    /// Update cursor blink, returns true if visibility changed
    pub fn tick(&mut self) -> bool {
        self.renderer.tick_cursor()
    }

    /// Redraw just the cursor cell (for efficient cursor blinking)
    pub fn render_cursor<F: Framebuffer>(&mut self, fb: &mut F) {
        if let Some(term) = &self.terminals[self.active] {
            let (col, row) = term.cursor();
            let grid = &term.grid;
            self.renderer.render_cursor_cell(fb, col, row, grid);
        }
    }

    /// Resize all consoles
    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.cols = cols;
        self.rows = rows;
        for terminal in self.terminals.iter_mut().flatten() {
            terminal.resize(cols, rows);
        }
    }

    /// Get keyboard handler reference (for converting events to chars)
    pub fn keyboard(&self) -> &Keyboard {
        &self.keyboard
    }

    /// Get mutable keyboard handler reference (for resetting state after child process)
    pub fn keyboard_mut(&mut self) -> &mut Keyboard {
        &mut self.keyboard
    }

    /// Get dimensions
    pub fn size(&self) -> (usize, usize) {
        (self.cols, self.rows)
    }

    /// Get a row's content as chars for testing
    /// Callback receives (row_index, &[Cell]) for each row
    pub fn for_each_row<F: FnMut(usize, &[Cell])>(&self, mut f: F) {
        if let Some(term) = self.active_terminal() {
            for row in 0..self.rows {
                if let Some(cells) = term.grid.row(row) {
                    f(row, cells);
                }
            }
        }
    }
}

impl Default for ConsoleManager {
    fn default() -> Self {
        Self::new(80, 25)
    }
}
