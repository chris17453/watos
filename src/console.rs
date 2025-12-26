// Virtual Console System for WATOS
// Allows multiple DOS sessions with independent screen buffers

extern crate alloc;
use alloc::vec::Vec;
use alloc::string::String;

pub const CONSOLE_COLS: usize = 80;
pub const CONSOLE_ROWS: usize = 25;
pub const CONSOLE_SIZE: usize = CONSOLE_COLS * CONSOLE_ROWS;

// A single character cell with character and attributes
#[derive(Clone, Copy)]
pub struct Cell {
    pub ch: u8,
    pub fg: u8,  // Foreground color (0-15)
    pub bg: u8,  // Background color (0-7)
}

impl Default for Cell {
    fn default() -> Self {
        Cell { ch: b' ', fg: 7, bg: 1 } // Light gray on blue (DOS shell style)
    }
}

// Virtual console with its own screen buffer
pub struct Console {
    pub id: u8,
    pub name: String,
    pub buffer: [Cell; CONSOLE_SIZE],
    pub cursor_x: u8,
    pub cursor_y: u8,
    pub cursor_visible: bool,
    pub task_id: Option<u32>,  // Associated DOS task
    pub parent_id: u8,         // Parent console (to return to on exit)
    pub current_fg: u8,        // Current foreground color
    pub current_bg: u8,        // Current background color
}

impl Console {
    pub fn new(id: u8, name: &str) -> Self {
        Console {
            id,
            name: String::from(name),
            buffer: [Cell::default(); CONSOLE_SIZE],
            cursor_x: 0,
            cursor_y: 0,
            cursor_visible: true,
            task_id: None,
            parent_id: 0, // Default parent is console 0
            current_fg: 7,  // Light gray
            current_bg: 1,  // Blue (DOS shell style)
        }
    }

    // Set current text colors
    pub fn set_colors(&mut self, fg: u8, bg: u8) {
        self.current_fg = fg & 0x0F;
        self.current_bg = bg & 0x07;
    }

    // Clear the console
    pub fn clear(&mut self) {
        let clear_cell = Cell { ch: b' ', fg: self.current_fg, bg: self.current_bg };
        for cell in self.buffer.iter_mut() {
            *cell = clear_cell;
        }
        self.cursor_x = 0;
        self.cursor_y = 0;
    }

    // Scroll up one line (simple version)
    pub fn scroll_up_simple(&mut self) {
        // Move lines up
        for row in 1..CONSOLE_ROWS {
            for col in 0..CONSOLE_COLS {
                self.buffer[(row - 1) * CONSOLE_COLS + col] = self.buffer[row * CONSOLE_COLS + col];
            }
        }
        // Clear bottom line with current colors
        let clear_cell = Cell { ch: b' ', fg: self.current_fg, bg: self.current_bg };
        for col in 0..CONSOLE_COLS {
            self.buffer[(CONSOLE_ROWS - 1) * CONSOLE_COLS + col] = clear_cell;
        }
    }

    // Scroll up window (INT 10h AH=06h compatible)
    pub fn scroll_up(&mut self, lines: u8, attr: u8, top: u8, left: u8, bottom: u8, right: u8) {
        let lines = if lines == 0 {
            // 0 means clear entire window
            (bottom - top + 1) as usize
        } else {
            lines as usize
        };
        let top = (top as usize).min(CONSOLE_ROWS - 1);
        let bottom = (bottom as usize).min(CONSOLE_ROWS - 1);
        let left = (left as usize).min(CONSOLE_COLS - 1);
        let right = (right as usize).min(CONSOLE_COLS - 1);

        let fg = attr & 0x0F;
        let bg = (attr >> 4) & 0x07;
        let clear_cell = Cell { ch: b' ', fg, bg };

        // Scroll the window up by 'lines' rows
        for _ in 0..lines {
            for row in top..bottom {
                for col in left..=right {
                    self.buffer[row * CONSOLE_COLS + col] = self.buffer[(row + 1) * CONSOLE_COLS + col];
                }
            }
            // Clear bottom row of window
            for col in left..=right {
                self.buffer[bottom * CONSOLE_COLS + col] = clear_cell;
            }
        }
    }

    // Scroll down window (INT 10h AH=07h compatible)
    pub fn scroll_down(&mut self, lines: u8, attr: u8, top: u8, left: u8, bottom: u8, right: u8) {
        let lines = if lines == 0 {
            (bottom - top + 1) as usize
        } else {
            lines as usize
        };
        let top = (top as usize).min(CONSOLE_ROWS - 1);
        let bottom = (bottom as usize).min(CONSOLE_ROWS - 1);
        let left = (left as usize).min(CONSOLE_COLS - 1);
        let right = (right as usize).min(CONSOLE_COLS - 1);

        let fg = attr & 0x0F;
        let bg = (attr >> 4) & 0x07;
        let clear_cell = Cell { ch: b' ', fg, bg };

        // Scroll the window down by 'lines' rows
        for _ in 0..lines {
            for row in ((top + 1)..=bottom).rev() {
                for col in left..=right {
                    self.buffer[row * CONSOLE_COLS + col] = self.buffer[(row - 1) * CONSOLE_COLS + col];
                }
            }
            // Clear top row of window
            for col in left..=right {
                self.buffer[top * CONSOLE_COLS + col] = clear_cell;
            }
        }
    }

    // Write a character at cursor position
    pub fn putchar(&mut self, ch: u8) {
        match ch {
            b'\n' => {
                self.cursor_x = 0;
                self.cursor_y += 1;
                if self.cursor_y >= CONSOLE_ROWS as u8 {
                    self.scroll_up_simple();
                    self.cursor_y = (CONSOLE_ROWS - 1) as u8;
                }
            }
            b'\r' => {
                self.cursor_x = 0;
            }
            0x08 => { // Backspace
                if self.cursor_x > 0 {
                    self.cursor_x -= 1;
                    let idx = self.cursor_y as usize * CONSOLE_COLS + self.cursor_x as usize;
                    self.buffer[idx] = Cell::default();
                }
            }
            _ => {
                let idx = self.cursor_y as usize * CONSOLE_COLS + self.cursor_x as usize;
                if idx < CONSOLE_SIZE {
                    self.buffer[idx].ch = ch;
                    self.buffer[idx].fg = self.current_fg;
                    self.buffer[idx].bg = self.current_bg;
                }
                self.cursor_x += 1;
                if self.cursor_x >= CONSOLE_COLS as u8 {
                    self.cursor_x = 0;
                    self.cursor_y += 1;
                    if self.cursor_y >= CONSOLE_ROWS as u8 {
                        self.scroll_up_simple();
                        self.cursor_y = (CONSOLE_ROWS - 1) as u8;
                    }
                }
            }
        }
    }

    // Write a string
    pub fn print(&mut self, s: &[u8]) {
        for &ch in s {
            self.putchar(ch);
        }
    }

    // Set cursor position
    pub fn set_cursor(&mut self, x: u8, y: u8) {
        self.cursor_x = x.min((CONSOLE_COLS - 1) as u8);
        self.cursor_y = y.min((CONSOLE_ROWS - 1) as u8);
    }

    // Get character at position
    pub fn get_char(&self, x: u8, y: u8) -> u8 {
        let idx = y as usize * CONSOLE_COLS + x as usize;
        if idx < CONSOLE_SIZE {
            self.buffer[idx].ch
        } else {
            b' '
        }
    }
}

// Console manager - tracks all consoles and active console
pub struct ConsoleManager {
    consoles: Vec<Console>,
    active_idx: usize,
}

impl ConsoleManager {
    pub fn new() -> Self {
        let mut mgr = ConsoleManager {
            consoles: Vec::new(),
            active_idx: 0,
        };
        // Create console 0 - the main shell
        mgr.consoles.push(Console::new(0, "DOS64 Shell"));
        mgr
    }

    // Create a new console for a DOS task, returns console ID
    // Parent is the currently active console
    pub fn create_console(&mut self, name: &str, task_id: u32) -> u8 {
        let id = self.consoles.len() as u8;
        let parent = self.active_id();
        let mut console = Console::new(id, name);
        console.task_id = Some(task_id);
        console.parent_id = parent;
        self.consoles.push(console);
        id
    }

    // Remove a console and return to parent (when task terminates)
    // Returns the parent console ID to switch to
    pub fn remove_console(&mut self, id: u8) -> u8 {
        if id == 0 { return 0; } // Never remove console 0
        let parent_id = if let Some(con) = self.consoles.iter().find(|c| c.id == id) {
            con.parent_id
        } else {
            0
        };

        if let Some(pos) = self.consoles.iter().position(|c| c.id == id) {
            self.consoles.remove(pos);
            // Switch to parent console
            self.switch_to(parent_id);
        }
        parent_id
    }

    // Get active console
    pub fn active(&mut self) -> &mut Console {
        &mut self.consoles[self.active_idx]
    }

    // Get console by ID
    pub fn get(&mut self, id: u8) -> Option<&mut Console> {
        self.consoles.iter_mut().find(|c| c.id == id)
    }

    // Get console by task ID
    pub fn get_by_task(&mut self, task_id: u32) -> Option<&mut Console> {
        self.consoles.iter_mut().find(|c| c.task_id == Some(task_id))
    }

    // Switch to next console (Ctrl+Tab)
    pub fn next(&mut self) -> u8 {
        if self.consoles.len() > 1 {
            self.active_idx = (self.active_idx + 1) % self.consoles.len();
        }
        self.consoles[self.active_idx].id
    }

    // Switch to previous console (Shift+Ctrl+Tab)
    pub fn prev(&mut self) -> u8 {
        if self.consoles.len() > 1 {
            if self.active_idx == 0 {
                self.active_idx = self.consoles.len() - 1;
            } else {
                self.active_idx -= 1;
            }
        }
        self.consoles[self.active_idx].id
    }

    // Switch to specific console
    pub fn switch_to(&mut self, id: u8) -> bool {
        if let Some(pos) = self.consoles.iter().position(|c| c.id == id) {
            self.active_idx = pos;
            true
        } else {
            false
        }
    }

    // Get active console ID
    pub fn active_id(&self) -> u8 {
        self.consoles[self.active_idx].id
    }

    // Get number of consoles
    pub fn count(&self) -> usize {
        self.consoles.len()
    }

    // Check if a task's console is active
    pub fn is_task_active(&self, task_id: u32) -> bool {
        self.consoles[self.active_idx].task_id == Some(task_id)
    }
}

// Global console manager
static mut CONSOLE_MANAGER: Option<ConsoleManager> = None;

pub fn init() {
    unsafe {
        CONSOLE_MANAGER = Some(ConsoleManager::new());
    }
}

pub fn manager() -> &'static mut ConsoleManager {
    unsafe {
        CONSOLE_MANAGER.as_mut().expect("Console manager not initialized")
    }
}

// Convenience functions for active console
pub fn print(s: &[u8]) {
    manager().active().print(s);
}

pub fn putchar(ch: u8) {
    manager().active().putchar(ch);
}

pub fn clear() {
    manager().active().clear();
}
