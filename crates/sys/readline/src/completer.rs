//! Tab completion for readline
//!
//! Provides path completion, command completion, and extensible completion traits.

use alloc::string::String;
use alloc::vec::Vec;
use watos_syscall::numbers as syscall;

/// A single completion candidate
#[derive(Debug, Clone)]
pub struct Completion {
    /// The replacement text (what gets inserted)
    pub text: String,
    /// Display text (what's shown when listing completions)
    pub display: String,
    /// Optional suffix to add (e.g., '/' for directories, ' ' for commands)
    pub suffix: Option<char>,
}

impl Completion {
    /// Create a new completion
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let display = text.clone();
        Completion {
            text,
            display,
            suffix: None,
        }
    }

    /// Create a completion with a suffix
    pub fn with_suffix(text: impl Into<String>, suffix: char) -> Self {
        let text = text.into();
        let display = text.clone();
        Completion {
            text,
            display,
            suffix: Some(suffix),
        }
    }

    /// Set custom display text
    pub fn display(mut self, display: impl Into<String>) -> Self {
        self.display = display.into();
        self
    }
}

/// Trait for tab completion providers
pub trait Completer: Send + Sync {
    /// Get completions for the given line at the cursor position
    ///
    /// Returns a list of possible completions.
    fn complete(&self, line: &str, cursor: usize) -> Vec<Completion>;

    /// Get the word boundaries for the word being completed
    ///
    /// Returns (start, end) positions of the word at cursor.
    fn word_boundaries(&self, line: &str, cursor: usize) -> (usize, usize) {
        let bytes = line.as_bytes();
        let mut start = cursor;
        let mut end = cursor;

        // Find start of word
        while start > 0 && !bytes[start - 1].is_ascii_whitespace() {
            start -= 1;
        }

        // Find end of word
        while end < bytes.len() && !bytes[end].is_ascii_whitespace() {
            end += 1;
        }

        (start, end)
    }
}

/// Path completer for files and directories
pub struct PathCompleter;

impl PathCompleter {
    /// Create a new path completer
    pub fn new() -> Self {
        PathCompleter
    }

    /// Get current working directory
    fn get_cwd() -> String {
        let mut buf = [0u8; 256];
        let len = unsafe {
            let ret: u64;
            core::arch::asm!(
                "int 0x80",
                in("eax") syscall::SYS_GETCWD,
                in("rdi") buf.as_mut_ptr() as u64,
                in("rsi") buf.len() as u64,
                lateout("rax") ret,
                options(nostack)
            );
            ret as usize
        };

        if len > 0 && len < buf.len() {
            String::from_utf8_lossy(&buf[..len]).into_owned()
        } else {
            String::from("/")
        }
    }

    /// List directory contents
    fn list_directory(path: &str) -> Vec<(String, bool)> {
        let mut results = Vec::new();
        let mut buf = [0u8; 4096];

        let len = unsafe {
            let ret: u64;
            core::arch::asm!(
                "int 0x80",
                in("eax") syscall::SYS_READDIR,
                in("rdi") path.as_ptr() as u64,
                in("rsi") path.len() as u64,
                in("rdx") buf.as_mut_ptr() as u64,
                lateout("rax") ret,
                options(nostack)
            );
            ret as usize
        };

        if len > 0 && len < buf.len() {
            // Parse response: "TYPE\tNAME SIZE\n" format
            let data = &buf[..len];
            for line in data.split(|&b| b == b'\n') {
                if line.is_empty() {
                    continue;
                }
                // Format: "D\tname" or "F\tname SIZE"
                if line.len() >= 3 && line[1] == b'\t' {
                    let is_dir = line[0] == b'D';
                    // Find the name (up to space or end)
                    let name_start = 2;
                    let name_end = line[name_start..]
                        .iter()
                        .position(|&b| b == b' ')
                        .map(|p| name_start + p)
                        .unwrap_or(line.len());

                    if let Ok(name) = core::str::from_utf8(&line[name_start..name_end]) {
                        if !name.is_empty() && name != "." && name != ".." {
                            results.push((String::from(name), is_dir));
                        }
                    }
                }
            }
        }

        results
    }

    /// Complete a path
    fn complete_path(&self, partial: &str) -> Vec<Completion> {
        let mut completions = Vec::new();

        // Handle tilde expansion
        let expanded = if partial.starts_with('~') {
            // Get HOME directory
            let mut home_buf = [0u8; 128];
            let home_len = unsafe {
                let ret: u64;
                core::arch::asm!(
                    "int 0x80",
                    in("eax") syscall::SYS_GETENV,
                    in("rdi") b"HOME".as_ptr() as u64,
                    in("rsi") 4u64,
                    in("rdx") home_buf.as_mut_ptr() as u64,
                    in("r10") home_buf.len() as u64,
                    lateout("rax") ret,
                    options(nostack)
                );
                ret as usize
            };

            if home_len > 0 && home_len < home_buf.len() {
                let home = core::str::from_utf8(&home_buf[..home_len]).unwrap_or("/");
                let rest = &partial[1..];
                let mut expanded = String::from(home);
                if !rest.starts_with('/') && !rest.is_empty() {
                    expanded.push('/');
                }
                expanded.push_str(rest);
                expanded
            } else {
                String::from(partial)
            }
        } else {
            String::from(partial)
        };

        // Split into directory and filename parts
        let (dir, prefix) = if let Some(pos) = expanded.rfind('/') {
            (&expanded[..=pos], &expanded[pos + 1..])
        } else {
            // No slash - complete in current directory
            let cwd = Self::get_cwd();
            let entries = Self::list_directory(&cwd);
            for (name, is_dir) in entries {
                if name.starts_with(&expanded) {
                    let suffix = if is_dir { Some('/') } else { Some(' ') };
                    completions.push(Completion::with_suffix(name, suffix.unwrap_or(' ')));
                }
            }
            return completions;
        };

        // List the directory
        let entries = Self::list_directory(dir);
        for (name, is_dir) in entries {
            if name.starts_with(prefix) {
                let mut full_path = String::from(dir);
                full_path.push_str(&name);

                let suffix = if is_dir { Some('/') } else { Some(' ') };
                completions.push(Completion {
                    text: full_path,
                    display: name,
                    suffix,
                });
            }
        }

        completions
    }
}

impl Default for PathCompleter {
    fn default() -> Self {
        Self::new()
    }
}

impl Completer for PathCompleter {
    fn complete(&self, line: &str, cursor: usize) -> Vec<Completion> {
        let (start, _end) = self.word_boundaries(line, cursor);
        let word = &line[start..cursor];
        self.complete_path(word)
    }
}

/// Command completer for executables in PATH
pub struct CommandCompleter;

impl CommandCompleter {
    /// Create a new command completer
    pub fn new() -> Self {
        CommandCompleter
    }

    /// Get PATH directories
    fn get_path_dirs() -> Vec<String> {
        let mut buf = [0u8; 512];
        let len = unsafe {
            let ret: u64;
            core::arch::asm!(
                "int 0x80",
                in("eax") syscall::SYS_GETENV,
                in("rdi") b"PATH".as_ptr() as u64,
                in("rsi") 4u64,
                in("rdx") buf.as_mut_ptr() as u64,
                in("r10") buf.len() as u64,
                lateout("rax") ret,
                options(nostack)
            );
            ret as usize
        };

        let mut dirs = Vec::new();
        if len > 0 && len < buf.len() {
            if let Ok(path) = core::str::from_utf8(&buf[..len]) {
                for dir in path.split(':') {
                    if !dir.is_empty() {
                        dirs.push(String::from(dir));
                    }
                }
            }
        }

        // Default fallback
        if dirs.is_empty() {
            dirs.push(String::from("C:/apps/system"));
        }

        dirs
    }

    /// Complete a command name
    fn complete_command(&self, partial: &str) -> Vec<Completion> {
        let mut completions = Vec::new();
        let mut seen = Vec::new();

        for dir in Self::get_path_dirs() {
            let entries = PathCompleter::list_directory(&dir);
            for (name, _is_dir) in entries {
                // Skip directories, only complete executables
                if name.starts_with(partial) && !seen.contains(&name) {
                    seen.push(name.clone());
                    completions.push(Completion::with_suffix(name, ' '));
                }
            }
        }

        completions
    }
}

impl Default for CommandCompleter {
    fn default() -> Self {
        Self::new()
    }
}

impl Completer for CommandCompleter {
    fn complete(&self, line: &str, cursor: usize) -> Vec<Completion> {
        let (start, _end) = self.word_boundaries(line, cursor);
        let word = &line[start..cursor];
        self.complete_command(word)
    }
}

/// Shell completer that combines path and command completion intelligently
pub struct ShellCompleter;

impl ShellCompleter {
    /// Create a new shell completer
    pub fn new() -> Self {
        ShellCompleter
    }

    /// Check if the word looks like a path
    fn is_path_like(word: &str) -> bool {
        word.starts_with('/')
            || word.starts_with('.')
            || word.starts_with('~')
            || word.contains('/')
    }
}

impl Default for ShellCompleter {
    fn default() -> Self {
        Self::new()
    }
}

impl Completer for ShellCompleter {
    fn complete(&self, line: &str, cursor: usize) -> Vec<Completion> {
        let (start, _end) = self.word_boundaries(line, cursor);
        let word = &line[start..cursor];

        // If word looks like a path, use path completion
        if Self::is_path_like(word) {
            return PathCompleter::new().complete(line, cursor);
        }

        // If at the start of line or after certain characters, complete commands
        let before = &line[..start];
        let is_command_position = before.is_empty()
            || before.ends_with(' ')
            || before.ends_with('|')
            || before.ends_with('&')
            || before.ends_with(';');

        if is_command_position {
            // Complete both commands and paths
            let mut completions = CommandCompleter::new().complete(line, cursor);
            completions.extend(PathCompleter::new().complete(line, cursor));
            completions
        } else {
            // Complete paths for arguments
            PathCompleter::new().complete(line, cursor)
        }
    }
}
