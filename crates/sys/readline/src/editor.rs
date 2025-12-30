//! Line editor core for readline
//!
//! Combines the edit buffer, key handling, and terminal output
//! into a cohesive line editing experience.

use crate::buffer::EditBuffer;
use crate::completer::{Completer, Completion};
use crate::history::History;
use crate::key::{Key, KeyReader};
use crate::terminal::Terminal;
use crate::{EditMode, ReadlineError};

#[cfg(feature = "emacs")]
use crate::emacs::{Action as EmacsAction, EmacsMode};

#[cfg(feature = "vi")]
use crate::vi::{Action as ViAction, ViMode, ViState};

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

/// Result of reading a line
#[derive(Debug)]
pub enum ReadResult {
    /// Successfully read a line
    Line(String),
    /// User pressed Ctrl-C
    Interrupted,
    /// User pressed Ctrl-D on empty line (EOF)
    Eof,
}

/// Main line editor
pub struct LineEditor {
    /// Edit buffer for current line
    buffer: EditBuffer,
    /// Current prompt string
    prompt: String,
    /// Prompt display width
    prompt_width: usize,
    /// Current editing mode
    mode: EditMode,
    /// Emacs mode handler
    #[cfg(feature = "emacs")]
    emacs: EmacsMode,
    /// Vi mode handler
    #[cfg(feature = "vi")]
    vi: ViMode,
    /// Tab completers
    completers: Vec<Box<dyn Completer>>,
    /// Current completion state
    completion_state: Option<CompletionState>,
}

/// State for tab completion cycling
struct CompletionState {
    /// Original text before completion started
    original: String,
    /// Original cursor position
    original_cursor: usize,
    /// List of completions
    completions: Vec<Completion>,
    /// Current completion index
    index: usize,
    /// Start position of word being completed
    word_start: usize,
}

impl LineEditor {
    /// Create a new line editor
    pub fn new() -> Self {
        LineEditor {
            buffer: EditBuffer::new(),
            prompt: String::new(),
            prompt_width: 0,
            mode: EditMode::default(),
            #[cfg(feature = "emacs")]
            emacs: EmacsMode::new(),
            #[cfg(feature = "vi")]
            vi: ViMode::new(),
            completers: Vec::new(),
            completion_state: None,
        }
    }

    /// Set the editing mode
    pub fn set_mode(&mut self, mode: EditMode) {
        self.mode = mode;
    }

    /// Get the current editing mode
    pub fn mode(&self) -> EditMode {
        self.mode
    }

    /// Add a completer
    pub fn add_completer(&mut self, completer: Box<dyn Completer>) {
        self.completers.push(completer);
    }

    /// Clear all completers
    pub fn clear_completers(&mut self) {
        self.completers.clear();
    }

    /// Edit a line (called from Readline with external mode)
    /// Note: Completers should be added via add_completer() before calling
    #[allow(dead_code)]
    pub fn edit_line(
        &mut self,
        prompt: &str,
        history: &mut History,
        mode: EditMode,
    ) -> Result<String, ReadlineError> {
        self.mode = mode;

        match self.readline(prompt, history) {
            ReadResult::Line(s) => Ok(s),
            ReadResult::Interrupted => Err(ReadlineError::Interrupted),
            ReadResult::Eof => Err(ReadlineError::Eof),
        }
    }

    /// Read a line with the given prompt
    pub fn readline(&mut self, prompt: &str, history: &mut History) -> ReadResult {
        // Reset state
        self.buffer.clear();
        self.prompt = String::from(prompt);
        self.prompt_width = prompt.chars().count();
        self.completion_state = None;

        #[cfg(feature = "emacs")]
        self.emacs.reset();
        #[cfg(feature = "vi")]
        self.vi.reset();

        history.reset_position();

        // Display prompt
        Terminal::write(prompt);

        // Main editing loop
        loop {
            let key = KeyReader::read_key();

            // Cancel completion on non-tab
            if key != Key::Tab {
                self.completion_state = None;
            }

            // Dispatch to mode handler
            let result = match self.mode {
                #[cfg(feature = "emacs")]
                EditMode::Emacs => self.handle_emacs_key(key, history),
                #[cfg(feature = "vi")]
                EditMode::Vi => self.handle_vi_key(key, history),
            };

            match result {
                LoopAction::Continue => {
                    self.refresh_line();
                }
                LoopAction::Done => {
                    Terminal::newline();
                    let line = self.buffer.content();
                    history.add(&line);
                    return ReadResult::Line(line);
                }
                LoopAction::Interrupt => {
                    Terminal::write("^C");
                    Terminal::newline();
                    return ReadResult::Interrupted;
                }
                LoopAction::Eof => {
                    return ReadResult::Eof;
                }
                LoopAction::Refresh => {
                    Terminal::clear_screen();
                    Terminal::write(&self.prompt);
                    self.refresh_line();
                }
                LoopAction::Complete => {
                    self.do_completion();
                    self.refresh_line();
                }
                LoopAction::HistoryPrev => {
                    if let Some(entry) = history.prev(&self.buffer.content()) {
                        self.buffer.set_content(entry);
                    }
                    self.refresh_line();
                }
                LoopAction::HistoryNext => {
                    if let Some(entry) = history.next() {
                        self.buffer.set_content(entry);
                    }
                    self.refresh_line();
                }
                LoopAction::HistorySearch => {
                    self.do_history_search(history);
                    self.refresh_line();
                }
                LoopAction::ModeChanged => {
                    self.refresh_line();
                }
            }
        }
    }

    /// Handle key in Emacs mode
    #[cfg(feature = "emacs")]
    fn handle_emacs_key(&mut self, key: Key, _history: &mut History) -> LoopAction {
        let action = self.emacs.handle_key(key, &mut self.buffer);
        match action {
            EmacsAction::Continue => LoopAction::Continue,
            EmacsAction::Done => LoopAction::Done,
            EmacsAction::Interrupt => LoopAction::Interrupt,
            EmacsAction::Eof => LoopAction::Eof,
            EmacsAction::HistoryPrev => LoopAction::HistoryPrev,
            EmacsAction::HistoryNext => LoopAction::HistoryNext,
            EmacsAction::HistorySearch => LoopAction::HistorySearch,
            EmacsAction::Complete => LoopAction::Complete,
            EmacsAction::Refresh => LoopAction::Refresh,
        }
    }

    /// Handle key in Vi mode
    #[cfg(feature = "vi")]
    fn handle_vi_key(&mut self, key: Key, _history: &mut History) -> LoopAction {
        let action = self.vi.handle_key(key, &mut self.buffer);
        match action {
            ViAction::Continue => LoopAction::Continue,
            ViAction::Done => LoopAction::Done,
            ViAction::Interrupt => LoopAction::Interrupt,
            ViAction::Eof => LoopAction::Eof,
            ViAction::HistoryPrev => LoopAction::HistoryPrev,
            ViAction::HistoryNext => LoopAction::HistoryNext,
            ViAction::HistorySearch => LoopAction::HistorySearch,
            ViAction::Complete => LoopAction::Complete,
            ViAction::Refresh => LoopAction::Refresh,
            ViAction::ModeChanged => LoopAction::ModeChanged,
        }
    }

    /// Refresh the displayed line
    fn refresh_line(&self) {
        // Move to start of line
        Terminal::carriage_return();

        // Redraw prompt
        Terminal::write(&self.prompt);

        // Draw mode indicator for vi
        #[cfg(feature = "vi")]
        if self.mode == EditMode::Vi {
            match self.vi.state() {
                ViState::Command => {
                    // Could show indicator like [CMD] but keep it simple
                }
                ViState::Insert | ViState::Replace => {}
            }
        }

        // Draw buffer content
        Terminal::write(&self.buffer.content());

        // Clear to end of line (remove any leftover characters)
        Terminal::clear_to_end();

        // Position cursor
        let cursor_col = self.prompt_width + self.buffer.cursor() + 1;
        Terminal::move_to_column(cursor_col);
    }

    /// Perform tab completion
    fn do_completion(&mut self) {
        if self.completers.is_empty() {
            Terminal::beep();
            return;
        }

        // Check if we're continuing a completion cycle
        if let Some(ref mut state) = self.completion_state {
            // Cycle to next completion
            state.index = (state.index + 1) % state.completions.len();
            let completion = &state.completions[state.index];

            // Restore original and apply new completion
            self.buffer.set_content(&state.original);
            self.buffer.set_cursor(state.word_start);

            // Delete the original word
            while self.buffer.cursor() < state.original_cursor {
                self.buffer.delete_char();
            }

            // Insert completion
            self.buffer.insert_str(&completion.text);
            if let Some(suffix) = completion.suffix {
                self.buffer.insert(suffix);
            }
            return;
        }

        // Start new completion
        let line = self.buffer.content();
        let cursor = self.buffer.cursor();

        // Collect completions from all completers
        let mut all_completions = Vec::new();
        for completer in &self.completers {
            let completions = completer.complete(&line, cursor);
            all_completions.extend(completions);
        }

        if all_completions.is_empty() {
            Terminal::beep();
            return;
        }

        // Find word boundaries
        let word_start = self
            .completers
            .first()
            .map(|c| c.word_boundaries(&line, cursor).0)
            .unwrap_or(cursor);

        if all_completions.len() == 1 {
            // Single completion - apply it directly
            let completion = &all_completions[0];

            // Delete the partial word
            while self.buffer.cursor() > word_start {
                self.buffer.backspace();
            }

            // Insert completion
            self.buffer.insert_str(&completion.text);
            if let Some(suffix) = completion.suffix {
                self.buffer.insert(suffix);
            }
        } else {
            // Multiple completions - find common prefix
            let common = Self::common_prefix(&all_completions);
            let word = &line[word_start..cursor];

            if common.len() > word.len() {
                // We can complete some more
                while self.buffer.cursor() > word_start {
                    self.buffer.backspace();
                }
                self.buffer.insert_str(&common);
            } else {
                // Show all completions
                Terminal::newline();
                self.show_completions(&all_completions);
                Terminal::write(&self.prompt);

                // Set up completion state for cycling
                self.completion_state = Some(CompletionState {
                    original: line,
                    original_cursor: cursor,
                    completions: all_completions,
                    index: 0,
                    word_start,
                });
            }
        }
    }

    /// Find common prefix of all completions
    fn common_prefix(completions: &[Completion]) -> String {
        if completions.is_empty() {
            return String::new();
        }

        let first = &completions[0].text;
        let mut prefix_len = first.len();

        for comp in &completions[1..] {
            let text = &comp.text;
            let common = first
                .chars()
                .zip(text.chars())
                .take_while(|(a, b)| a == b)
                .count();
            prefix_len = prefix_len.min(common);
        }

        first.chars().take(prefix_len).collect()
    }

    /// Show completion options
    fn show_completions(&self, completions: &[Completion]) {
        // Calculate column width based on longest completion
        let max_len = completions
            .iter()
            .map(|c| c.display.len())
            .max()
            .unwrap_or(10);
        let col_width = max_len + 2;

        // Assume ~80 character terminal width
        let cols = (78 / col_width).max(1);
        let mut col = 0;

        for completion in completions {
            Terminal::write(&completion.display);

            // Pad to column width
            let padding = col_width - completion.display.len();
            for _ in 0..padding {
                Terminal::write(" ");
            }

            col += 1;
            if col >= cols {
                Terminal::newline();
                col = 0;
            }
        }

        if col > 0 {
            Terminal::newline();
        }
    }

    /// Perform incremental history search (Ctrl-R)
    fn do_history_search(&mut self, history: &mut History) {
        let mut search_pattern = String::new();
        let original_buffer = self.buffer.content();

        // Show search prompt
        Terminal::carriage_return();
        Terminal::clear_to_end();
        Terminal::write("(reverse-i-search)`': ");

        loop {
            let key = KeyReader::read_key();

            match key {
                Key::Ctrl('r') => {
                    // Search for next match
                    if let Some(found) = history.search_backward(&search_pattern, &original_buffer)
                    {
                        self.buffer.set_content(found);
                    }
                }
                Key::Ctrl('s') => {
                    // Search forward
                    if let Some(found) = history.search_forward(&search_pattern) {
                        self.buffer.set_content(found);
                    }
                }
                Key::Ctrl('g') | Key::Escape => {
                    // Cancel search, restore original
                    self.buffer.set_content(&original_buffer);
                    history.reset_position();
                    break;
                }
                Key::Enter => {
                    // Accept current match
                    break;
                }
                Key::Backspace | Key::Ctrl('h') => {
                    // Remove last character from search pattern
                    search_pattern.pop();
                }
                Key::Char(ch) => {
                    // Add to search pattern
                    search_pattern.push(ch);
                    // Search with updated pattern
                    if let Some(found) = history.search_backward(&search_pattern, &original_buffer)
                    {
                        self.buffer.set_content(found);
                    } else {
                        Terminal::beep();
                    }
                }
                _ => {
                    // Accept current match and process key normally
                    break;
                }
            }

            // Update search display
            Terminal::carriage_return();
            Terminal::clear_to_end();
            Terminal::write("(reverse-i-search)`");
            Terminal::write(&search_pattern);
            Terminal::write("': ");
            Terminal::write(&self.buffer.content());
        }

        // Restore normal prompt display
        Terminal::carriage_return();
        Terminal::clear_to_end();
        Terminal::write(&self.prompt);
    }

    /// Get reference to the current buffer
    pub fn buffer(&self) -> &EditBuffer {
        &self.buffer
    }

    /// Get mutable reference to the current buffer
    pub fn buffer_mut(&mut self) -> &mut EditBuffer {
        &mut self.buffer
    }

    /// Get vi mode state (if in vi mode)
    #[cfg(feature = "vi")]
    pub fn vi_state(&self) -> Option<ViState> {
        if self.mode == EditMode::Vi {
            Some(self.vi.state())
        } else {
            None
        }
    }
}

impl Default for LineEditor {
    fn default() -> Self {
        Self::new()
    }
}

/// Internal action for main loop
enum LoopAction {
    Continue,
    Done,
    Interrupt,
    Eof,
    Refresh,
    Complete,
    HistoryPrev,
    HistoryNext,
    HistorySearch,
    ModeChanged,
}
