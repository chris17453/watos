//! Emacs editing mode for readline
//!
//! Implements standard Emacs key bindings for line editing.

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
    /// User pressed Ctrl-D on empty line
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
}

/// Emacs editing mode state
pub struct EmacsMode {
    /// Kill ring (for Ctrl-K, Ctrl-U, Ctrl-W, Ctrl-Y)
    kill_ring: String,
    /// Last command was a yank (for yank-pop)
    last_was_yank: bool,
    /// Argument prefix (Ctrl-U, Alt-digit)
    arg_prefix: Option<i32>,
}

impl EmacsMode {
    /// Create a new Emacs mode
    pub fn new() -> Self {
        EmacsMode {
            kill_ring: String::new(),
            last_was_yank: false,
            arg_prefix: None,
        }
    }

    /// Handle a key press
    pub fn handle_key(&mut self, key: Key, buffer: &mut EditBuffer) -> Action {
        self.last_was_yank = false;

        match key {
            // === Enter/Return ===
            Key::Enter => Action::Done,

            // === Interrupt/EOF ===
            Key::Ctrl('c') => Action::Interrupt,
            Key::Ctrl('d') => {
                if buffer.is_empty() {
                    Action::Eof
                } else {
                    buffer.delete_char();
                    Action::Continue
                }
            }

            // === Character insertion ===
            Key::Char(ch) => {
                buffer.insert(ch);
                Action::Continue
            }

            // === Deletion ===
            Key::Backspace | Key::Ctrl('h') => {
                buffer.backspace();
                Action::Continue
            }
            Key::Delete => {
                buffer.delete_char();
                Action::Continue
            }
            Key::Ctrl('k') => {
                // Kill to end of line
                let killed = buffer.delete_to_end();
                if !killed.is_empty() {
                    self.kill_ring = killed;
                }
                Action::Continue
            }
            Key::Ctrl('u') => {
                // Kill to start of line
                let killed = buffer.delete_to_start();
                if !killed.is_empty() {
                    self.kill_ring = killed;
                }
                Action::Continue
            }
            Key::Ctrl('w') => {
                // Kill previous word
                let killed = buffer.delete_word_back();
                if !killed.is_empty() {
                    self.kill_ring = killed;
                }
                Action::Continue
            }
            Key::Alt('d') => {
                // Kill next word
                let killed = buffer.delete_word_forward();
                if !killed.is_empty() {
                    self.kill_ring = killed;
                }
                Action::Continue
            }

            // === Yank ===
            Key::Ctrl('y') => {
                buffer.insert_str(&self.kill_ring);
                self.last_was_yank = true;
                Action::Continue
            }

            // === Movement ===
            Key::Left | Key::Ctrl('b') => {
                buffer.move_left();
                Action::Continue
            }
            Key::Right | Key::Ctrl('f') => {
                buffer.move_right();
                Action::Continue
            }
            Key::Home | Key::Ctrl('a') => {
                buffer.move_home();
                Action::Continue
            }
            Key::End | Key::Ctrl('e') => {
                buffer.move_end();
                Action::Continue
            }
            Key::Alt('b') => {
                buffer.move_word_left();
                Action::Continue
            }
            Key::Alt('f') => {
                buffer.move_word_right();
                Action::Continue
            }

            // === History ===
            Key::Up | Key::Ctrl('p') => Action::HistoryPrev,
            Key::Down | Key::Ctrl('n') => Action::HistoryNext,
            Key::Ctrl('r') => Action::HistorySearch,

            // === Tab completion ===
            Key::Tab => Action::Complete,

            // === Misc ===
            Key::Ctrl('l') => {
                // Clear screen
                Action::Refresh
            }
            Key::Ctrl('t') => {
                // Transpose characters
                buffer.transpose_chars();
                Action::Continue
            }
            Key::Escape => {
                // Just ignore standalone escape
                Action::Continue
            }

            // === Unknown ===
            _ => Action::Continue,
        }
    }

    /// Get the kill ring contents
    pub fn kill_ring(&self) -> &str {
        &self.kill_ring
    }

    /// Clear state for new line
    pub fn reset(&mut self) {
        self.arg_prefix = None;
        self.last_was_yank = false;
    }
}

impl Default for EmacsMode {
    fn default() -> Self {
        Self::new()
    }
}
