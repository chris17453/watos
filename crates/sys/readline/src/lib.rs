//! WATOS Readline Library
//!
//! Provides full readline functionality for the WATOS shell including:
//! - Line editing with cursor movement
//! - Command history with up/down navigation
//! - Tab completion for files/directories and commands
//! - Vi and Emacs editing modes
//!
//! # Example
//!
//! ```rust,ignore
//! use watos_readline::{Readline, EditMode};
//!
//! let mut rl = Readline::new();
//! rl.set_mode(EditMode::Emacs);
//!
//! loop {
//!     match rl.readline("$ ") {
//!         Ok(line) => {
//!             // Process the line
//!             rl.add_history(&line);
//!         }
//!         Err(ReadlineError::Eof) => break,
//!         Err(ReadlineError::Interrupted) => continue,
//!         Err(_) => break,
//!     }
//! }
//! ```

#![no_std]

extern crate alloc;

mod buffer;
mod history;
mod key;
mod terminal;
mod completer;
mod editor;

#[cfg(feature = "emacs")]
mod emacs;

#[cfg(feature = "vi")]
mod vi;

pub use buffer::EditBuffer;
pub use history::History;
pub use key::{Key, KeyReader};
pub use terminal::Terminal;
pub use completer::{Completer, Completion, PathCompleter, CommandCompleter, ShellCompleter};
pub use editor::LineEditor;

#[cfg(feature = "emacs")]
pub use emacs::EmacsMode;

#[cfg(feature = "vi")]
pub use vi::{ViMode, ViState};

use alloc::boxed::Box;
use alloc::string::String;

/// Readline error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadlineError {
    /// User pressed Ctrl-C
    Interrupted,
    /// User pressed Ctrl-D on empty line
    Eof,
    /// I/O error
    IoError,
}

/// Editing mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditMode {
    /// Emacs-style editing (default)
    Emacs,
    /// Vi-style editing with command/insert modes
    Vi,
}

impl Default for EditMode {
    fn default() -> Self {
        EditMode::Emacs
    }
}

/// Main readline interface
pub struct Readline {
    /// Line editor
    editor: LineEditor,
    /// Command history
    history: History,
    /// Current editing mode
    mode: EditMode,
}

impl Readline {
    /// Create a new Readline instance with default settings
    pub fn new() -> Self {
        Readline {
            editor: LineEditor::new(),
            history: History::new(1000),
            mode: EditMode::default(),
        }
    }

    /// Create a new Readline instance with specified history size
    pub fn with_history_size(size: usize) -> Self {
        Readline {
            editor: LineEditor::new(),
            history: History::new(size),
            mode: EditMode::default(),
        }
    }

    /// Set the editing mode (Emacs or Vi)
    pub fn set_mode(&mut self, mode: EditMode) {
        self.mode = mode;
    }

    /// Get the current editing mode
    pub fn mode(&self) -> EditMode {
        self.mode
    }

    /// Add a completer for tab completion
    pub fn add_completer(&mut self, completer: Box<dyn Completer>) {
        self.editor.add_completer(completer);
    }

    /// Clear all completers
    pub fn clear_completers(&mut self) {
        self.editor.clear_completers();
    }

    /// Add a line to history
    pub fn add_history(&mut self, line: &str) {
        self.history.add(line);
    }

    /// Clear history
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// Get history length
    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    /// Read a line with full editing support
    ///
    /// Displays the prompt and allows the user to edit the line using
    /// the current editing mode. Returns the line when Enter is pressed.
    ///
    /// # Returns
    /// - `Ok(String)` - The edited line
    /// - `Err(ReadlineError::Interrupted)` - User pressed Ctrl-C
    /// - `Err(ReadlineError::Eof)` - User pressed Ctrl-D on empty line
    pub fn readline(&mut self, prompt: &str) -> Result<String, ReadlineError> {
        // Set mode on editor
        self.editor.set_mode(self.mode);

        // Use editor's readline which returns ReadResult
        match self.editor.readline(prompt, &mut self.history) {
            editor::ReadResult::Line(s) => Ok(s),
            editor::ReadResult::Interrupted => Err(ReadlineError::Interrupted),
            editor::ReadResult::Eof => Err(ReadlineError::Eof),
        }
    }
}

impl Default for Readline {
    fn default() -> Self {
        Self::new()
    }
}
