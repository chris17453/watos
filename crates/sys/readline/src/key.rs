//! Key event abstraction for readline
//!
//! Provides a unified key representation combining ASCII characters
//! and special keys (arrows, function keys, etc.)

use watos_syscall::numbers as syscall;

/// Key event representation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Regular printable character
    Char(char),
    /// Control + character (Ctrl-A through Ctrl-Z)
    Ctrl(char),
    /// Alt/Meta + character
    Alt(char),
    /// Up arrow
    Up,
    /// Down arrow
    Down,
    /// Left arrow
    Left,
    /// Right arrow
    Right,
    /// Home key
    Home,
    /// End key
    End,
    /// Delete key
    Delete,
    /// Backspace key
    Backspace,
    /// Tab key
    Tab,
    /// Enter/Return key
    Enter,
    /// Escape key
    Escape,
    /// Page Up
    PageUp,
    /// Page Down
    PageDown,
    /// Insert key
    Insert,
    /// Unknown/unhandled key
    Unknown(u8),
}

/// Key reader using syscalls
pub struct KeyReader;

impl KeyReader {
    /// Read a single key event (blocking)
    ///
    /// Combines SYS_GETKEY for ASCII and handles escape sequences
    /// for special keys.
    pub fn read_key() -> Key {
        loop {
            let ch = Self::read_char();
            if ch == 0 {
                // No key available, spin briefly
                for _ in 0..1000 {
                    core::hint::spin_loop();
                }
                continue;
            }

            // Handle escape sequences
            if ch == 0x1B {
                // Escape - could be standalone or start of sequence
                let ch2 = Self::read_char_timeout();
                if ch2 == 0 {
                    // Standalone escape
                    return Key::Escape;
                }

                if ch2 == b'[' {
                    // CSI sequence (arrow keys, etc.)
                    return Self::parse_csi_sequence();
                } else if ch2 == b'O' {
                    // SS3 sequence (some terminals use this for arrows)
                    return Self::parse_ss3_sequence();
                } else {
                    // Alt + character
                    return Key::Alt(ch2 as char);
                }
            }

            // Control characters
            if ch < 32 {
                return match ch {
                    0x00 => Key::Ctrl('@'),  // Ctrl-Space or Ctrl-@
                    0x08 => Key::Backspace,  // Ctrl-H (backspace)
                    0x09 => Key::Tab,        // Ctrl-I (tab)
                    0x0A | 0x0D => Key::Enter, // Ctrl-J (LF) or Ctrl-M (CR)
                    0x1B => Key::Escape,     // Already handled above
                    0x01..=0x1A => Key::Ctrl((b'a' + ch - 1) as char),
                    _ => Key::Ctrl((b'@' + ch) as char),
                };
            }

            // DEL character
            if ch == 0x7F {
                return Key::Backspace;
            }

            // Regular printable character
            return Key::Char(ch as char);
        }
    }

    /// Read a character with timeout (for escape sequence parsing)
    fn read_char_timeout() -> u8 {
        // Brief delay to see if more characters are coming
        for _ in 0..5000 {
            let ch = Self::read_char();
            if ch != 0 {
                return ch;
            }
            core::hint::spin_loop();
        }
        0
    }

    /// Read a single character using syscall
    fn read_char() -> u8 {
        unsafe {
            let ret: u64;
            core::arch::asm!(
                "int 0x80",
                in("eax") syscall::SYS_GETKEY,
                lateout("rax") ret,
                options(nostack)
            );
            ret as u8
        }
    }

    /// Parse CSI (Control Sequence Introducer) escape sequence
    /// Format: ESC [ <params> <final>
    fn parse_csi_sequence() -> Key {
        let mut params = [0u8; 8];
        let mut param_count = 0;

        loop {
            let ch = Self::read_char_timeout();
            if ch == 0 {
                return Key::Unknown(0);
            }

            // Collect parameter bytes (0-9 and ;)
            if ch.is_ascii_digit() || ch == b';' {
                if param_count < params.len() {
                    params[param_count] = ch;
                    param_count += 1;
                }
                continue;
            }

            // Final byte determines the key
            return match ch {
                b'A' => Key::Up,
                b'B' => Key::Down,
                b'C' => Key::Right,
                b'D' => Key::Left,
                b'H' => Key::Home,
                b'F' => Key::End,
                b'~' => {
                    // Parse the number parameter
                    let n = Self::parse_number(&params[..param_count]);
                    match n {
                        1 => Key::Home,
                        2 => Key::Insert,
                        3 => Key::Delete,
                        4 => Key::End,
                        5 => Key::PageUp,
                        6 => Key::PageDown,
                        7 => Key::Home,
                        8 => Key::End,
                        _ => Key::Unknown(ch),
                    }
                }
                _ => Key::Unknown(ch),
            };
        }
    }

    /// Parse SS3 escape sequence
    /// Format: ESC O <final>
    fn parse_ss3_sequence() -> Key {
        let ch = Self::read_char_timeout();
        match ch {
            b'A' => Key::Up,
            b'B' => Key::Down,
            b'C' => Key::Right,
            b'D' => Key::Left,
            b'H' => Key::Home,
            b'F' => Key::End,
            _ => Key::Unknown(ch),
        }
    }

    /// Parse a decimal number from parameter bytes
    fn parse_number(bytes: &[u8]) -> u32 {
        let mut n = 0u32;
        for &b in bytes {
            if b.is_ascii_digit() {
                n = n.saturating_mul(10).saturating_add((b - b'0') as u32);
            } else if b == b';' {
                break;
            }
        }
        n
    }
}

impl Key {
    /// Check if this is a printable character
    pub fn is_printable(&self) -> bool {
        matches!(self, Key::Char(c) if c.is_ascii_graphic() || *c == ' ')
    }

    /// Get the character if this is a Char or Ctrl key
    pub fn char(&self) -> Option<char> {
        match self {
            Key::Char(c) => Some(*c),
            Key::Ctrl(c) => Some(*c),
            Key::Alt(c) => Some(*c),
            _ => None,
        }
    }

    /// Check if this is a Ctrl key
    pub fn is_ctrl(&self) -> bool {
        matches!(self, Key::Ctrl(_))
    }

    /// Check if this is an Alt key
    pub fn is_alt(&self) -> bool {
        matches!(self, Key::Alt(_))
    }
}
