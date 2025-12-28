//! Keyboard input handling
//!
//! Converts raw PS/2 scancodes to key events with modifier tracking.

use bitflags::bitflags;

bitflags! {
    /// Keyboard modifier flags
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct Modifiers: u8 {
        const SHIFT   = 0b0000_0001;
        const CTRL    = 0b0000_0010;
        const ALT     = 0b0000_0100;
        const CAPSLOCK = 0b0000_1000;
        const NUMLOCK  = 0b0001_0000;
    }
}

/// Key codes (virtual key representation)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    // Printable characters are stored directly
    Char(char),

    // Function keys
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,

    // Navigation
    Up, Down, Left, Right,
    Home, End, PageUp, PageDown,
    Insert, Delete,

    // Control keys
    Escape,
    Tab,
    Backspace,
    Enter,
    Space,

    // Modifiers (for tracking state, not usually sent as events)
    LeftShift, RightShift,
    LeftCtrl, RightCtrl,
    LeftAlt, RightAlt,
    CapsLock, NumLock, ScrollLock,

    // Keypad
    KpEnter,
    Kp0, Kp1, Kp2, Kp3, Kp4, Kp5, Kp6, Kp7, Kp8, Kp9,
    KpPlus, KpMinus, KpMultiply, KpDivide, KpDecimal,

    // Unknown/unmapped
    Unknown(u8),
}

/// A keyboard event
#[derive(Debug, Clone, Copy)]
pub struct KeyEvent {
    /// The key that was pressed or released
    pub key: KeyCode,
    /// Whether this is a press (true) or release (false)
    pub pressed: bool,
    /// Current modifier state
    pub modifiers: Modifiers,
}

/// Keyboard state machine for PS/2 scancode set 1
pub struct Keyboard {
    /// Current modifier state
    modifiers: Modifiers,
    /// Extended scancode flag (0xE0 prefix)
    extended: bool,
}

impl Keyboard {
    /// Create a new keyboard handler
    pub const fn new() -> Self {
        Self {
            modifiers: Modifiers::empty(),
            extended: false,
        }
    }

    /// Reset keyboard state (clears modifiers and extended flag)
    /// Call this after returning from a child process to prevent state corruption
    pub fn reset(&mut self) {
        self.modifiers = Modifiers::empty();
        self.extended = false;
    }

    /// Process a raw PS/2 scancode, returning a key event if applicable
    pub fn process_scancode(&mut self, scancode: u8) -> Option<KeyEvent> {
        // Handle extended prefix
        if scancode == 0xE0 {
            self.extended = true;
            return None;
        }

        // Check for key release (high bit set)
        let pressed = scancode & 0x80 == 0;
        let code = scancode & 0x7F;

        let key = if self.extended {
            self.extended = false;
            self.decode_extended(code)
        } else {
            self.decode_standard(code)
        };

        // Update modifier state
        self.update_modifiers(&key, pressed);

        // Don't emit events for pure modifier keys
        if self.is_modifier(&key) {
            return None;
        }

        Some(KeyEvent {
            key,
            pressed,
            modifiers: self.modifiers,
        })
    }

    /// Get current modifier state
    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    /// Decode standard (non-extended) scancode
    fn decode_standard(&self, code: u8) -> KeyCode {
        match code {
            0x01 => KeyCode::Escape,
            0x02 => KeyCode::Char('1'),
            0x03 => KeyCode::Char('2'),
            0x04 => KeyCode::Char('3'),
            0x05 => KeyCode::Char('4'),
            0x06 => KeyCode::Char('5'),
            0x07 => KeyCode::Char('6'),
            0x08 => KeyCode::Char('7'),
            0x09 => KeyCode::Char('8'),
            0x0A => KeyCode::Char('9'),
            0x0B => KeyCode::Char('0'),
            0x0C => KeyCode::Char('-'),
            0x0D => KeyCode::Char('='),
            0x0E => KeyCode::Backspace,
            0x0F => KeyCode::Tab,
            0x10 => KeyCode::Char('q'),
            0x11 => KeyCode::Char('w'),
            0x12 => KeyCode::Char('e'),
            0x13 => KeyCode::Char('r'),
            0x14 => KeyCode::Char('t'),
            0x15 => KeyCode::Char('y'),
            0x16 => KeyCode::Char('u'),
            0x17 => KeyCode::Char('i'),
            0x18 => KeyCode::Char('o'),
            0x19 => KeyCode::Char('p'),
            0x1A => KeyCode::Char('['),
            0x1B => KeyCode::Char(']'),
            0x1C => KeyCode::Enter,
            0x1D => KeyCode::LeftCtrl,
            0x1E => KeyCode::Char('a'),
            0x1F => KeyCode::Char('s'),
            0x20 => KeyCode::Char('d'),
            0x21 => KeyCode::Char('f'),
            0x22 => KeyCode::Char('g'),
            0x23 => KeyCode::Char('h'),
            0x24 => KeyCode::Char('j'),
            0x25 => KeyCode::Char('k'),
            0x26 => KeyCode::Char('l'),
            0x27 => KeyCode::Char(';'),
            0x28 => KeyCode::Char('\''),
            0x29 => KeyCode::Char('`'),
            0x2A => KeyCode::LeftShift,
            0x2B => KeyCode::Char('\\'),
            0x2C => KeyCode::Char('z'),
            0x2D => KeyCode::Char('x'),
            0x2E => KeyCode::Char('c'),
            0x2F => KeyCode::Char('v'),
            0x30 => KeyCode::Char('b'),
            0x31 => KeyCode::Char('n'),
            0x32 => KeyCode::Char('m'),
            0x33 => KeyCode::Char(','),
            0x34 => KeyCode::Char('.'),
            0x35 => KeyCode::Char('/'),
            0x36 => KeyCode::RightShift,
            0x37 => KeyCode::KpMultiply,
            0x38 => KeyCode::LeftAlt,
            0x39 => KeyCode::Space,
            0x3A => KeyCode::CapsLock,
            0x3B => KeyCode::F1,
            0x3C => KeyCode::F2,
            0x3D => KeyCode::F3,
            0x3E => KeyCode::F4,
            0x3F => KeyCode::F5,
            0x40 => KeyCode::F6,
            0x41 => KeyCode::F7,
            0x42 => KeyCode::F8,
            0x43 => KeyCode::F9,
            0x44 => KeyCode::F10,
            0x45 => KeyCode::NumLock,
            0x46 => KeyCode::ScrollLock,
            0x47 => KeyCode::Kp7,
            0x48 => KeyCode::Kp8,
            0x49 => KeyCode::Kp9,
            0x4A => KeyCode::KpMinus,
            0x4B => KeyCode::Kp4,
            0x4C => KeyCode::Kp5,
            0x4D => KeyCode::Kp6,
            0x4E => KeyCode::KpPlus,
            0x4F => KeyCode::Kp1,
            0x50 => KeyCode::Kp2,
            0x51 => KeyCode::Kp3,
            0x52 => KeyCode::Kp0,
            0x53 => KeyCode::KpDecimal,
            0x57 => KeyCode::F11,
            0x58 => KeyCode::F12,
            _ => KeyCode::Unknown(code),
        }
    }

    /// Decode extended (0xE0 prefixed) scancode
    fn decode_extended(&self, code: u8) -> KeyCode {
        match code {
            0x1C => KeyCode::KpEnter,
            0x1D => KeyCode::RightCtrl,
            0x35 => KeyCode::KpDivide,
            0x38 => KeyCode::RightAlt,
            0x47 => KeyCode::Home,
            0x48 => KeyCode::Up,
            0x49 => KeyCode::PageUp,
            0x4B => KeyCode::Left,
            0x4D => KeyCode::Right,
            0x4F => KeyCode::End,
            0x50 => KeyCode::Down,
            0x51 => KeyCode::PageDown,
            0x52 => KeyCode::Insert,
            0x53 => KeyCode::Delete,
            _ => KeyCode::Unknown(code | 0x80), // Mark as extended
        }
    }

    /// Update modifier state based on key event
    fn update_modifiers(&mut self, key: &KeyCode, pressed: bool) {
        let modifier = match key {
            KeyCode::LeftShift | KeyCode::RightShift => Some(Modifiers::SHIFT),
            KeyCode::LeftCtrl | KeyCode::RightCtrl => Some(Modifiers::CTRL),
            KeyCode::LeftAlt | KeyCode::RightAlt => Some(Modifiers::ALT),
            KeyCode::CapsLock if pressed => {
                // Toggle on press only
                self.modifiers ^= Modifiers::CAPSLOCK;
                return;
            }
            KeyCode::NumLock if pressed => {
                self.modifiers ^= Modifiers::NUMLOCK;
                return;
            }
            _ => None,
        };

        if let Some(m) = modifier {
            if pressed {
                self.modifiers |= m;
            } else {
                self.modifiers &= !m;
            }
        }
    }

    /// Check if a key is a pure modifier
    fn is_modifier(&self, key: &KeyCode) -> bool {
        matches!(
            key,
            KeyCode::LeftShift
                | KeyCode::RightShift
                | KeyCode::LeftCtrl
                | KeyCode::RightCtrl
                | KeyCode::LeftAlt
                | KeyCode::RightAlt
                | KeyCode::CapsLock
                | KeyCode::NumLock
                | KeyCode::ScrollLock
        )
    }

    /// Convert a key event to a character (if printable)
    pub fn to_char(&self, event: &KeyEvent) -> Option<char> {
        if !event.pressed {
            return None;
        }

        let shift = event.modifiers.contains(Modifiers::SHIFT);
        let caps = event.modifiers.contains(Modifiers::CAPSLOCK);
        let ctrl = event.modifiers.contains(Modifiers::CTRL);

        match event.key {
            KeyCode::Char(c) => {
                if ctrl {
                    // Ctrl+letter produces control codes
                    if c.is_ascii_lowercase() {
                        return Some((c as u8 - b'a' + 1) as char);
                    }
                    return None;
                }

                let uppercase = shift ^ caps;

                if c.is_ascii_alphabetic() {
                    Some(if uppercase {
                        c.to_ascii_uppercase()
                    } else {
                        c.to_ascii_lowercase()
                    })
                } else if shift {
                    // Shift + symbol
                    Some(match c {
                        '1' => '!',
                        '2' => '@',
                        '3' => '#',
                        '4' => '$',
                        '5' => '%',
                        '6' => '^',
                        '7' => '&',
                        '8' => '*',
                        '9' => '(',
                        '0' => ')',
                        '-' => '_',
                        '=' => '+',
                        '[' => '{',
                        ']' => '}',
                        '\\' => '|',
                        ';' => ':',
                        '\'' => '"',
                        ',' => '<',
                        '.' => '>',
                        '/' => '?',
                        '`' => '~',
                        _ => c,
                    })
                } else {
                    Some(c)
                }
            }
            KeyCode::Space => Some(' '),
            KeyCode::Enter | KeyCode::KpEnter => Some('\n'),
            KeyCode::Tab => Some('\t'),
            KeyCode::Backspace => Some('\x08'),
            _ => None,
        }
    }

    /// Convert a key event to an escape sequence for terminal
    pub fn to_escape_sequence(&self, event: &KeyEvent) -> Option<&'static [u8]> {
        if !event.pressed {
            return None;
        }

        match event.key {
            KeyCode::Up => Some(b"\x1B[A"),
            KeyCode::Down => Some(b"\x1B[B"),
            KeyCode::Right => Some(b"\x1B[C"),
            KeyCode::Left => Some(b"\x1B[D"),
            KeyCode::Home => Some(b"\x1B[H"),
            KeyCode::End => Some(b"\x1B[F"),
            KeyCode::Insert => Some(b"\x1B[2~"),
            KeyCode::Delete => Some(b"\x1B[3~"),
            KeyCode::PageUp => Some(b"\x1B[5~"),
            KeyCode::PageDown => Some(b"\x1B[6~"),
            KeyCode::F1 => Some(b"\x1BOP"),
            KeyCode::F2 => Some(b"\x1BOQ"),
            KeyCode::F3 => Some(b"\x1BOR"),
            KeyCode::F4 => Some(b"\x1BOS"),
            KeyCode::F5 => Some(b"\x1B[15~"),
            KeyCode::F6 => Some(b"\x1B[17~"),
            KeyCode::F7 => Some(b"\x1B[18~"),
            KeyCode::F8 => Some(b"\x1B[19~"),
            KeyCode::F9 => Some(b"\x1B[20~"),
            KeyCode::F10 => Some(b"\x1B[21~"),
            KeyCode::F11 => Some(b"\x1B[23~"),
            KeyCode::F12 => Some(b"\x1B[24~"),
            KeyCode::Escape => Some(b"\x1B"),
            _ => None,
        }
    }
}

impl Default for Keyboard {
    fn default() -> Self {
        Self::new()
    }
}
