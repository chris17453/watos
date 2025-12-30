//! Edit buffer for line manipulation
//!
//! Provides efficient character-level editing operations for readline.

use alloc::string::String;
use alloc::vec::Vec;

/// Buffer for editing a line of text
///
/// Supports efficient insertion, deletion, and cursor movement.
/// Uses a gap buffer internally for performance.
#[derive(Debug, Clone)]
pub struct EditBuffer {
    /// Characters in the buffer
    chars: Vec<char>,
    /// Current cursor position (0 to len inclusive)
    cursor: usize,
    /// Optional mark position for region operations
    mark: Option<usize>,
}

impl EditBuffer {
    /// Create a new empty buffer
    pub fn new() -> Self {
        EditBuffer {
            chars: Vec::new(),
            cursor: 0,
            mark: None,
        }
    }

    /// Create a buffer with initial content
    pub fn with_content(s: &str) -> Self {
        let chars: Vec<char> = s.chars().collect();
        let len = chars.len();
        EditBuffer {
            chars,
            cursor: len,
            mark: None,
        }
    }

    /// Get the current content as a String
    pub fn as_str(&self) -> String {
        self.chars.iter().collect()
    }

    /// Alias for as_str (used by editor)
    pub fn content(&self) -> String {
        self.as_str()
    }

    /// Set the buffer content (replaces everything)
    pub fn set_content(&mut self, s: &str) {
        self.chars = s.chars().collect();
        self.cursor = self.chars.len();
        self.mark = None;
    }

    /// Get the buffer length in characters
    pub fn len(&self) -> usize {
        self.chars.len()
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    /// Get the cursor position
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Set the cursor position (clamped to valid range)
    pub fn set_cursor(&mut self, pos: usize) {
        self.cursor = pos.min(self.chars.len());
    }

    /// Get character at position (if valid)
    pub fn char_at(&self, pos: usize) -> Option<char> {
        self.chars.get(pos).copied()
    }

    /// Get the character under the cursor (if any)
    pub fn current_char(&self) -> Option<char> {
        self.char_at(self.cursor)
    }

    /// Get the character before the cursor (if any)
    pub fn prev_char(&self) -> Option<char> {
        if self.cursor > 0 {
            self.char_at(self.cursor - 1)
        } else {
            None
        }
    }

    // ========== Insertion ==========

    /// Insert a character at the cursor position
    pub fn insert(&mut self, ch: char) {
        self.chars.insert(self.cursor, ch);
        self.cursor += 1;
    }

    /// Insert a string at the cursor position
    pub fn insert_str(&mut self, s: &str) {
        for ch in s.chars() {
            self.insert(ch);
        }
    }

    // ========== Deletion ==========

    /// Delete the character at the cursor (like Del key)
    /// Returns the deleted character if any
    pub fn delete_char(&mut self) -> Option<char> {
        if self.cursor < self.chars.len() {
            Some(self.chars.remove(self.cursor))
        } else {
            None
        }
    }

    /// Delete the character before the cursor (like Backspace)
    /// Returns the deleted character if any
    pub fn backspace(&mut self) -> Option<char> {
        if self.cursor > 0 {
            self.cursor -= 1;
            Some(self.chars.remove(self.cursor))
        } else {
            None
        }
    }

    /// Delete from cursor to end of line (Ctrl-K in emacs)
    /// Returns the deleted text
    pub fn delete_to_end(&mut self) -> String {
        let deleted: String = self.chars[self.cursor..].iter().collect();
        self.chars.truncate(self.cursor);
        deleted
    }

    /// Delete from start to cursor (Ctrl-U in emacs)
    /// Returns the deleted text
    pub fn delete_to_start(&mut self) -> String {
        let deleted: String = self.chars[..self.cursor].iter().collect();
        self.chars = self.chars[self.cursor..].to_vec();
        self.cursor = 0;
        deleted
    }

    /// Delete the previous word (Ctrl-W in emacs)
    /// Returns the deleted text
    pub fn delete_word_back(&mut self) -> String {
        if self.cursor == 0 {
            return String::new();
        }

        // Skip trailing whitespace
        let mut end = self.cursor;
        while end > 0 && self.chars[end - 1].is_whitespace() {
            end -= 1;
        }

        // Find word start
        let mut start = end;
        while start > 0 && !self.chars[start - 1].is_whitespace() {
            start -= 1;
        }

        // Delete the range
        let deleted: String = self.chars[start..self.cursor].iter().collect();
        self.chars.drain(start..self.cursor);
        self.cursor = start;
        deleted
    }

    /// Delete the next word (Alt-D in emacs)
    /// Returns the deleted text
    pub fn delete_word_forward(&mut self) -> String {
        if self.cursor >= self.chars.len() {
            return String::new();
        }

        let start = self.cursor;

        // Skip leading whitespace
        let mut end = start;
        while end < self.chars.len() && self.chars[end].is_whitespace() {
            end += 1;
        }

        // Skip word characters
        while end < self.chars.len() && !self.chars[end].is_whitespace() {
            end += 1;
        }

        // Delete the range
        let deleted: String = self.chars[start..end].iter().collect();
        self.chars.drain(start..end);
        deleted
    }

    // ========== Movement ==========

    /// Move cursor left one character
    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    /// Move cursor right one character
    pub fn move_right(&mut self) {
        if self.cursor < self.chars.len() {
            self.cursor += 1;
        }
    }

    /// Move cursor to start of line
    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    /// Move cursor to end of line
    pub fn move_end(&mut self) {
        self.cursor = self.chars.len();
    }

    /// Move cursor to previous word start
    pub fn move_word_left(&mut self) {
        if self.cursor == 0 {
            return;
        }

        // Skip whitespace before cursor
        while self.cursor > 0 && self.chars[self.cursor - 1].is_whitespace() {
            self.cursor -= 1;
        }

        // Skip word characters
        while self.cursor > 0 && !self.chars[self.cursor - 1].is_whitespace() {
            self.cursor -= 1;
        }
    }

    /// Move cursor to next word end
    pub fn move_word_right(&mut self) {
        let len = self.chars.len();

        // Skip current word
        while self.cursor < len && !self.chars[self.cursor].is_whitespace() {
            self.cursor += 1;
        }

        // Skip whitespace
        while self.cursor < len && self.chars[self.cursor].is_whitespace() {
            self.cursor += 1;
        }
    }

    // ========== Vi-specific operations ==========

    /// Move to end of current word (vi 'e' command)
    pub fn move_word_end(&mut self) {
        let len = self.chars.len();
        if self.cursor >= len {
            return;
        }

        // Move at least one position if not at end
        if self.cursor < len {
            self.cursor += 1;
        }

        // Skip whitespace
        while self.cursor < len && self.chars[self.cursor].is_whitespace() {
            self.cursor += 1;
        }

        // Move to end of word
        while self.cursor < len && !self.chars[self.cursor].is_whitespace() {
            self.cursor += 1;
        }

        // Back up one to be on last char of word
        if self.cursor > 0 && self.cursor <= len {
            self.cursor -= 1;
        }
    }

    /// Find next occurrence of character (vi 'f' command)
    pub fn find_char_forward(&mut self, ch: char) -> bool {
        for i in (self.cursor + 1)..self.chars.len() {
            if self.chars[i] == ch {
                self.cursor = i;
                return true;
            }
        }
        false
    }

    /// Find previous occurrence of character (vi 'F' command)
    pub fn find_char_backward(&mut self, ch: char) -> bool {
        for i in (0..self.cursor).rev() {
            if self.chars[i] == ch {
                self.cursor = i;
                return true;
            }
        }
        false
    }

    // ========== Mark operations ==========

    /// Set the mark at current cursor position
    pub fn set_mark(&mut self) {
        self.mark = Some(self.cursor);
    }

    /// Clear the mark
    pub fn clear_mark(&mut self) {
        self.mark = None;
    }

    /// Get the mark position
    pub fn mark(&self) -> Option<usize> {
        self.mark
    }

    /// Exchange cursor and mark (Ctrl-X Ctrl-X in emacs)
    pub fn exchange_point_and_mark(&mut self) {
        if let Some(mark) = self.mark {
            let old_cursor = self.cursor;
            self.cursor = mark;
            self.mark = Some(old_cursor);
        }
    }

    // ========== Buffer operations ==========

    /// Clear the entire buffer
    pub fn clear(&mut self) {
        self.chars.clear();
        self.cursor = 0;
        self.mark = None;
    }

    /// Replace the entire buffer content
    pub fn replace(&mut self, content: &str) {
        self.chars = content.chars().collect();
        self.cursor = self.chars.len();
        self.mark = None;
    }

    /// Transpose characters before cursor (Ctrl-T in emacs)
    pub fn transpose_chars(&mut self) {
        if self.cursor >= 2 {
            self.chars.swap(self.cursor - 1, self.cursor - 2);
        } else if self.cursor == 1 && self.chars.len() >= 2 {
            self.chars.swap(0, 1);
            self.cursor = 2.min(self.chars.len());
        }
    }

    /// Toggle case of character at cursor (vi ~ command)
    pub fn toggle_case(&mut self) {
        if self.cursor < self.chars.len() {
            let ch = self.chars[self.cursor];
            if ch.is_ascii_lowercase() {
                self.chars[self.cursor] = ch.to_ascii_uppercase();
            } else if ch.is_ascii_uppercase() {
                self.chars[self.cursor] = ch.to_ascii_lowercase();
            }
        }
    }

    /// Simple undo (restores to empty - proper undo would need history)
    /// For now, this is a no-op placeholder
    pub fn undo(&mut self) {
        // A real implementation would track edit history
        // For now, we just beep (handled by caller)
    }

    /// Get text from cursor to position (for vi motions)
    pub fn text_range(&self, start: usize, end: usize) -> String {
        let s = start.min(self.chars.len());
        let e = end.min(self.chars.len());
        if s <= e {
            self.chars[s..e].iter().collect()
        } else {
            self.chars[e..s].iter().collect()
        }
    }

    /// Delete range and return deleted text
    pub fn delete_range(&mut self, start: usize, end: usize) -> String {
        let s = start.min(self.chars.len());
        let e = end.min(self.chars.len());
        if s <= e {
            let deleted: String = self.chars[s..e].iter().collect();
            self.chars.drain(s..e);
            self.cursor = s;
            deleted
        } else {
            let deleted: String = self.chars[e..s].iter().collect();
            self.chars.drain(e..s);
            self.cursor = e;
            deleted
        }
    }
}

impl Default for EditBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert() {
        let mut buf = EditBuffer::new();
        buf.insert('h');
        buf.insert('i');
        assert_eq!(buf.as_str(), "hi");
        assert_eq!(buf.cursor(), 2);
    }

    #[test]
    fn test_backspace() {
        let mut buf = EditBuffer::with_content("hello");
        assert_eq!(buf.backspace(), Some('o'));
        assert_eq!(buf.as_str(), "hell");
    }

    #[test]
    fn test_delete_word_back() {
        let mut buf = EditBuffer::with_content("hello world");
        let deleted = buf.delete_word_back();
        assert_eq!(deleted, "world");
        assert_eq!(buf.as_str(), "hello ");
    }

    #[test]
    fn test_movement() {
        let mut buf = EditBuffer::with_content("hello");
        buf.move_home();
        assert_eq!(buf.cursor(), 0);
        buf.move_end();
        assert_eq!(buf.cursor(), 5);
        buf.move_left();
        assert_eq!(buf.cursor(), 4);
    }
}
