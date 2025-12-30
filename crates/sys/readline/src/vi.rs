//! Vi editing mode for readline
//!
//! Implements vi-style modal editing with command and insert modes.

use crate::buffer::EditBuffer;
use crate::key::Key;
use alloc::string::String;

/// Action to take after handling a key
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Continue editing
    Continue,
    /// Line is complete, return it
    Done,
    /// User pressed Ctrl-C
    Interrupt,
    /// User pressed Ctrl-D on empty line (or in command mode)
    Eof,
    /// Request history previous
    HistoryPrev,
    /// Request history next
    HistoryNext,
    /// Request history search
    HistorySearch,
    /// Request tab completion
    Complete,
    /// Refresh the display
    Refresh,
    /// Mode changed (for status display)
    ModeChanged,
}

/// Vi editing state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViState {
    /// Insert mode - characters are inserted
    Insert,
    /// Command mode - keys are commands
    Command,
    /// Replace mode - characters overwrite
    Replace,
}

/// Vi editing mode
pub struct ViMode {
    /// Current editing state
    state: ViState,
    /// Repeat count for commands (e.g., 5x deletes 5 chars)
    repeat_count: Option<usize>,
    /// Yank buffer (for y, d, p commands)
    yank_buffer: String,
    /// Last command for repeat (.)
    last_command: Option<Key>,
    /// Pending operator (d, c, y)
    pending_operator: Option<char>,
}

impl ViMode {
    /// Create a new Vi mode (starts in insert mode like bash)
    pub fn new() -> Self {
        ViMode {
            state: ViState::Insert,
            repeat_count: None,
            yank_buffer: String::new(),
            last_command: None,
            pending_operator: None,
        }
    }

    /// Get current state
    pub fn state(&self) -> ViState {
        self.state
    }

    /// Check if in insert mode
    pub fn is_insert(&self) -> bool {
        self.state == ViState::Insert
    }

    /// Check if in command mode
    pub fn is_command(&self) -> bool {
        self.state == ViState::Command
    }

    /// Handle a key press
    pub fn handle_key(&mut self, key: Key, buffer: &mut EditBuffer) -> Action {
        match self.state {
            ViState::Insert => self.handle_insert_mode(key, buffer),
            ViState::Command => self.handle_command_mode(key, buffer),
            ViState::Replace => self.handle_replace_mode(key, buffer),
        }
    }

    /// Handle key in insert mode
    fn handle_insert_mode(&mut self, key: Key, buffer: &mut EditBuffer) -> Action {
        match key {
            // Exit insert mode
            Key::Escape => {
                self.state = ViState::Command;
                // Move cursor back one (vi behavior)
                if buffer.cursor() > 0 {
                    buffer.move_left();
                }
                Action::ModeChanged
            }

            // Line complete
            Key::Enter => Action::Done,

            // Interrupt
            Key::Ctrl('c') => Action::Interrupt,

            // EOF on empty line
            Key::Ctrl('d') => {
                if buffer.is_empty() {
                    Action::Eof
                } else {
                    buffer.delete_char();
                    Action::Continue
                }
            }

            // Character insertion
            Key::Char(ch) => {
                buffer.insert(ch);
                Action::Continue
            }

            // Deletion
            Key::Backspace | Key::Ctrl('h') => {
                buffer.backspace();
                Action::Continue
            }
            Key::Delete => {
                buffer.delete_char();
                Action::Continue
            }

            // Word deletion
            Key::Ctrl('w') => {
                buffer.delete_word_back();
                Action::Continue
            }

            // Kill to start of line
            Key::Ctrl('u') => {
                buffer.delete_to_start();
                Action::Continue
            }

            // Movement (some work in insert mode)
            Key::Left => {
                buffer.move_left();
                Action::Continue
            }
            Key::Right => {
                buffer.move_right();
                Action::Continue
            }
            Key::Home => {
                buffer.move_home();
                Action::Continue
            }
            Key::End => {
                buffer.move_end();
                Action::Continue
            }

            // History
            Key::Up => Action::HistoryPrev,
            Key::Down => Action::HistoryNext,

            // Tab completion
            Key::Tab => Action::Complete,

            // Clear screen
            Key::Ctrl('l') => Action::Refresh,

            _ => Action::Continue,
        }
    }

    /// Handle key in command mode
    fn handle_command_mode(&mut self, key: Key, buffer: &mut EditBuffer) -> Action {
        // Handle digit for repeat count (except 0 which moves to start)
        if let Key::Char(ch) = key {
            if ch.is_ascii_digit() && ch != '0' {
                let digit = ch as usize - '0' as usize;
                self.repeat_count = Some(self.repeat_count.unwrap_or(0) * 10 + digit);
                return Action::Continue;
            }
        }

        let count = self.repeat_count.take().unwrap_or(1);

        // Handle pending operator
        if let Some(op) = self.pending_operator {
            return self.handle_operator_motion(op, key, count, buffer);
        }

        match key {
            // === Mode switching ===
            Key::Char('i') => {
                self.state = ViState::Insert;
                self.last_command = Some(key);
                Action::ModeChanged
            }
            Key::Char('I') => {
                buffer.move_home();
                self.state = ViState::Insert;
                self.last_command = Some(key);
                Action::ModeChanged
            }
            Key::Char('a') => {
                if !buffer.is_empty() {
                    buffer.move_right();
                }
                self.state = ViState::Insert;
                self.last_command = Some(key);
                Action::ModeChanged
            }
            Key::Char('A') => {
                buffer.move_end();
                self.state = ViState::Insert;
                self.last_command = Some(key);
                Action::ModeChanged
            }
            Key::Char('R') => {
                self.state = ViState::Replace;
                self.last_command = Some(key);
                Action::ModeChanged
            }

            // === Movement ===
            Key::Char('h') | Key::Left | Key::Backspace => {
                for _ in 0..count {
                    buffer.move_left();
                }
                Action::Continue
            }
            Key::Char('l') | Key::Right | Key::Char(' ') => {
                for _ in 0..count {
                    buffer.move_right();
                }
                Action::Continue
            }
            Key::Char('0') | Key::Home => {
                buffer.move_home();
                Action::Continue
            }
            Key::Char('$') | Key::End => {
                buffer.move_end();
                // In vi, cursor sits on last char, not after
                if buffer.cursor() > 0 && !buffer.is_empty() {
                    buffer.move_left();
                }
                Action::Continue
            }
            Key::Char('^') => {
                // Move to first non-blank
                buffer.move_home();
                let content = buffer.content();
                let chars: alloc::vec::Vec<char> = content.chars().collect();
                while buffer.cursor() < chars.len() {
                    if !chars[buffer.cursor()].is_whitespace() {
                        break;
                    }
                    buffer.move_right();
                }
                Action::Continue
            }
            Key::Char('w') => {
                for _ in 0..count {
                    buffer.move_word_right();
                }
                Action::Continue
            }
            Key::Char('b') => {
                for _ in 0..count {
                    buffer.move_word_left();
                }
                Action::Continue
            }
            Key::Char('e') => {
                // Move to end of word
                for _ in 0..count {
                    buffer.move_word_right();
                    if buffer.cursor() > 0 {
                        buffer.move_left();
                    }
                }
                Action::Continue
            }

            // === Deletion ===
            Key::Char('x') | Key::Delete => {
                let mut deleted = String::new();
                for _ in 0..count {
                    if let Some(ch) = buffer.delete_char() {
                        deleted.push(ch);
                    }
                }
                if !deleted.is_empty() {
                    self.yank_buffer = deleted;
                }
                self.last_command = Some(key);
                Action::Continue
            }
            Key::Char('X') => {
                let mut deleted = String::new();
                for _ in 0..count {
                    if let Some(ch) = buffer.backspace() {
                        deleted.insert(0, ch);
                    }
                }
                if !deleted.is_empty() {
                    self.yank_buffer = deleted;
                }
                self.last_command = Some(key);
                Action::Continue
            }
            Key::Char('D') => {
                // Delete to end of line
                let deleted = buffer.delete_to_end();
                if !deleted.is_empty() {
                    self.yank_buffer = deleted;
                }
                self.last_command = Some(key);
                Action::Continue
            }
            Key::Char('C') => {
                // Change to end of line
                let deleted = buffer.delete_to_end();
                if !deleted.is_empty() {
                    self.yank_buffer = deleted;
                }
                self.state = ViState::Insert;
                self.last_command = Some(key);
                Action::ModeChanged
            }
            Key::Char('S') | Key::Char('c') if key == Key::Char('S') => {
                // Substitute entire line
                self.yank_buffer = buffer.content();
                buffer.clear();
                self.state = ViState::Insert;
                self.last_command = Some(key);
                Action::ModeChanged
            }
            Key::Char('s') => {
                // Substitute character
                let mut deleted = String::new();
                for _ in 0..count {
                    if let Some(ch) = buffer.delete_char() {
                        deleted.push(ch);
                    }
                }
                if !deleted.is_empty() {
                    self.yank_buffer = deleted;
                }
                self.state = ViState::Insert;
                self.last_command = Some(key);
                Action::ModeChanged
            }

            // === Operators (start pending mode) ===
            Key::Char('d') => {
                self.pending_operator = Some('d');
                Action::Continue
            }
            Key::Char('c') => {
                self.pending_operator = Some('c');
                Action::Continue
            }
            Key::Char('y') => {
                self.pending_operator = Some('y');
                Action::Continue
            }

            // === Yank/Put ===
            Key::Char('Y') => {
                // Yank whole line
                self.yank_buffer = buffer.content();
                Action::Continue
            }
            Key::Char('p') => {
                // Put after cursor
                if !self.yank_buffer.is_empty() {
                    if !buffer.is_empty() {
                        buffer.move_right();
                    }
                    for _ in 0..count {
                        buffer.insert_str(&self.yank_buffer);
                    }
                }
                self.last_command = Some(key);
                Action::Continue
            }
            Key::Char('P') => {
                // Put before cursor
                for _ in 0..count {
                    buffer.insert_str(&self.yank_buffer);
                }
                self.last_command = Some(key);
                Action::Continue
            }

            // === Undo/Redo ===
            Key::Char('u') => {
                buffer.undo();
                Action::Continue
            }
            Key::Ctrl('r') => {
                // In vi, Ctrl-R is redo; but in readline, it's reverse search
                // We'll use it for history search here
                Action::HistorySearch
            }

            // === Repeat ===
            Key::Char('.') => {
                if let Some(last) = self.last_command {
                    // Re-execute last command
                    return self.handle_command_mode(last, buffer);
                }
                Action::Continue
            }

            // === History ===
            Key::Char('j') | Key::Down => Action::HistoryNext,
            Key::Char('k') | Key::Up => Action::HistoryPrev,
            Key::Char('/') => Action::HistorySearch,

            // === Line operations ===
            Key::Enter => Action::Done,
            Key::Ctrl('c') => Action::Interrupt,
            Key::Ctrl('d') => {
                if buffer.is_empty() {
                    Action::Eof
                } else {
                    Action::Continue
                }
            }
            Key::Ctrl('l') => Action::Refresh,

            // === Misc ===
            Key::Char('r') => {
                // Replace single character - would need next char
                // For now, just enter replace mode briefly
                self.state = ViState::Replace;
                Action::Continue
            }
            Key::Char('~') => {
                // Toggle case
                for _ in 0..count {
                    buffer.toggle_case();
                    buffer.move_right();
                }
                self.last_command = Some(key);
                Action::Continue
            }

            _ => Action::Continue,
        }
    }

    /// Handle operator + motion (d, c, y followed by motion)
    fn handle_operator_motion(
        &mut self,
        op: char,
        motion: Key,
        count: usize,
        buffer: &mut EditBuffer,
    ) -> Action {
        self.pending_operator = None;

        // Double operator means whole line (dd, cc, yy)
        if let Key::Char(ch) = motion {
            if ch == op {
                let content = buffer.content();
                match op {
                    'd' => {
                        self.yank_buffer = content;
                        buffer.clear();
                        self.last_command = Some(Key::Char('d'));
                    }
                    'c' => {
                        self.yank_buffer = content;
                        buffer.clear();
                        self.state = ViState::Insert;
                        self.last_command = Some(Key::Char('c'));
                        return Action::ModeChanged;
                    }
                    'y' => {
                        self.yank_buffer = content;
                    }
                    _ => {}
                }
                return Action::Continue;
            }
        }

        // Get start position
        let start = buffer.cursor();

        // Execute motion to find end position
        match motion {
            Key::Char('w') => {
                for _ in 0..count {
                    buffer.move_word_right();
                }
            }
            Key::Char('b') => {
                for _ in 0..count {
                    buffer.move_word_left();
                }
            }
            Key::Char('e') => {
                for _ in 0..count {
                    buffer.move_word_right();
                }
            }
            Key::Char('$') | Key::End => {
                buffer.move_end();
            }
            Key::Char('0') | Key::Home => {
                buffer.move_home();
            }
            Key::Char('h') | Key::Left => {
                for _ in 0..count {
                    buffer.move_left();
                }
            }
            Key::Char('l') | Key::Right => {
                for _ in 0..count {
                    buffer.move_right();
                }
            }
            _ => {
                return Action::Continue;
            }
        }

        let end = buffer.cursor();

        // Calculate range
        let (from, to) = if start < end {
            (start, end)
        } else {
            (end, start)
        };

        // Extract the text in range
        let content = buffer.content();
        let chars: alloc::vec::Vec<char> = content.chars().collect();
        let extracted: String = chars[from..to].iter().collect();

        // Move cursor to start of range
        while buffer.cursor() > from {
            buffer.move_left();
        }

        match op {
            'd' => {
                // Delete range
                self.yank_buffer = extracted;
                for _ in from..to {
                    buffer.delete_char();
                }
                self.last_command = Some(Key::Char('d'));
            }
            'c' => {
                // Change range (delete and enter insert mode)
                self.yank_buffer = extracted;
                for _ in from..to {
                    buffer.delete_char();
                }
                self.state = ViState::Insert;
                self.last_command = Some(Key::Char('c'));
                return Action::ModeChanged;
            }
            'y' => {
                // Yank range (restore cursor position)
                self.yank_buffer = extracted;
                // Move cursor back to original position
                while buffer.cursor() < start && buffer.cursor() < buffer.len() {
                    buffer.move_right();
                }
            }
            _ => {}
        }

        Action::Continue
    }

    /// Handle key in replace mode
    fn handle_replace_mode(&mut self, key: Key, buffer: &mut EditBuffer) -> Action {
        match key {
            Key::Escape => {
                self.state = ViState::Command;
                Action::ModeChanged
            }
            Key::Char(ch) => {
                // Replace character at cursor
                buffer.delete_char();
                buffer.insert(ch);
                // After single replace from 'r' command, go back to command mode
                // For 'R' (continuous replace), stay in replace mode
                // We'll stay in replace mode here; user can press Escape
                Action::Continue
            }
            Key::Backspace => {
                buffer.move_left();
                Action::Continue
            }
            Key::Enter => Action::Done,
            _ => Action::Continue,
        }
    }

    /// Get the yank buffer contents
    pub fn yank_buffer(&self) -> &str {
        &self.yank_buffer
    }

    /// Reset state for new line
    pub fn reset(&mut self) {
        self.state = ViState::Insert; // Start in insert mode
        self.repeat_count = None;
        self.pending_operator = None;
    }

    /// Force command mode (for when user does `set -o vi`)
    pub fn enter_command_mode(&mut self) {
        self.state = ViState::Command;
    }
}

impl Default for ViMode {
    fn default() -> Self {
        Self::new()
    }
}
