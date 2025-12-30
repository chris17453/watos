//! Command history management for readline
//!
//! Provides history storage, navigation, and search functionality.

use alloc::collections::VecDeque;
use alloc::string::String;

/// Command history storage and navigation
pub struct History {
    /// Stored history entries (oldest first)
    entries: VecDeque<String>,
    /// Maximum number of entries
    max_size: usize,
    /// Current navigation position (0 = most recent, len = past oldest)
    position: usize,
    /// Saved current line when navigating
    current_edit: String,
    /// Whether we're currently navigating
    navigating: bool,
}

impl History {
    /// Create a new history with specified maximum size
    pub fn new(max_size: usize) -> Self {
        History {
            entries: VecDeque::with_capacity(max_size.min(1024)),
            max_size,
            position: 0,
            current_edit: String::new(),
            navigating: false,
        }
    }

    /// Add a line to history
    ///
    /// Skips empty lines and duplicates of the most recent entry.
    pub fn add(&mut self, line: &str) {
        // Skip empty lines
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return;
        }

        // Skip if duplicate of most recent
        if let Some(last) = self.entries.back() {
            if last == trimmed {
                return;
            }
        }

        // Remove oldest if at capacity
        if self.entries.len() >= self.max_size {
            self.entries.pop_front();
        }

        self.entries.push_back(String::from(trimmed));
        self.reset_position();
    }

    /// Get number of entries in history
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if history is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear all history entries
    pub fn clear(&mut self) {
        self.entries.clear();
        self.reset_position();
    }

    /// Reset navigation position to current (newest)
    pub fn reset_position(&mut self) {
        self.position = 0;
        self.navigating = false;
        self.current_edit.clear();
    }

    /// Start navigation from current line
    pub fn start_navigation(&mut self, current: &str) {
        if !self.navigating {
            self.current_edit = String::from(current);
            self.navigating = true;
            self.position = 0;
        }
    }

    /// Navigate to previous (older) entry
    ///
    /// Returns the previous entry if available.
    pub fn prev(&mut self, current: &str) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }

        self.start_navigation(current);

        if self.position < self.entries.len() {
            self.position += 1;
            // Position 1 = most recent, position N = oldest
            let idx = self.entries.len() - self.position;
            Some(&self.entries[idx])
        } else {
            None
        }
    }

    /// Navigate to next (newer) entry
    ///
    /// Returns the next entry, or the original current line if at newest.
    pub fn next(&mut self) -> Option<&str> {
        if !self.navigating {
            return None;
        }

        if self.position > 0 {
            self.position -= 1;
            if self.position == 0 {
                // Back to current edit
                self.navigating = false;
                Some(&self.current_edit)
            } else {
                let idx = self.entries.len() - self.position;
                Some(&self.entries[idx])
            }
        } else {
            None
        }
    }

    /// Search backward for a line containing the pattern
    ///
    /// Starts from current position and searches toward older entries.
    pub fn search_backward(&mut self, pattern: &str, current: &str) -> Option<&str> {
        if self.entries.is_empty() || pattern.is_empty() {
            return None;
        }

        self.start_navigation(current);

        // Start searching from current position
        let start_pos = self.position;

        for offset in 1..=self.entries.len() {
            let check_pos = start_pos + offset;
            if check_pos > self.entries.len() {
                break;
            }
            let idx = self.entries.len() - check_pos;
            if self.entries[idx].contains(pattern) {
                self.position = check_pos;
                return Some(&self.entries[idx]);
            }
        }

        None
    }

    /// Search forward for a line containing the pattern
    pub fn search_forward(&mut self, pattern: &str) -> Option<&str> {
        if !self.navigating || self.position == 0 || pattern.is_empty() {
            return None;
        }

        for new_pos in (1..self.position).rev() {
            let idx = self.entries.len() - new_pos;
            if self.entries[idx].contains(pattern) {
                self.position = new_pos;
                return Some(&self.entries[idx]);
            }
        }

        // Check if current edit matches
        if self.current_edit.contains(pattern) {
            self.position = 0;
            self.navigating = false;
            return Some(&self.current_edit);
        }

        None
    }

    /// Get entry at specific index (0 = newest)
    pub fn get(&self, index: usize) -> Option<&str> {
        if index < self.entries.len() {
            Some(&self.entries[self.entries.len() - 1 - index])
        } else {
            None
        }
    }

    /// Get the current navigation position
    pub fn position(&self) -> usize {
        self.position
    }

    /// Check if currently navigating history
    pub fn is_navigating(&self) -> bool {
        self.navigating
    }

    /// Get the saved current edit (line before navigating)
    pub fn current_edit(&self) -> &str {
        &self.current_edit
    }

    /// Iterator over entries (newest first)
    pub fn iter(&self) -> impl Iterator<Item = &String> {
        self.entries.iter().rev()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_navigate() {
        let mut hist = History::new(10);
        hist.add("first");
        hist.add("second");
        hist.add("third");

        assert_eq!(hist.len(), 3);

        // Navigate back
        assert_eq!(hist.prev("current"), Some("third"));
        assert_eq!(hist.prev("current"), Some("second"));
        assert_eq!(hist.prev("current"), Some("first"));
        assert_eq!(hist.prev("current"), None);

        // Navigate forward
        assert_eq!(hist.next(), Some("second"));
        assert_eq!(hist.next(), Some("third"));
        assert_eq!(hist.next(), Some("current"));
        assert_eq!(hist.next(), None);
    }

    #[test]
    fn test_skip_duplicates() {
        let mut hist = History::new(10);
        hist.add("same");
        hist.add("same");
        hist.add("same");

        assert_eq!(hist.len(), 1);
    }

    #[test]
    fn test_search() {
        let mut hist = History::new(10);
        hist.add("echo hello");
        hist.add("ls -la");
        hist.add("echo world");
        hist.add("cat file");

        assert_eq!(hist.search_backward("echo", ""), Some("echo world"));
        assert_eq!(hist.search_backward("echo", ""), Some("echo hello"));
    }
}
