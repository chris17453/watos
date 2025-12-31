//! Keyboard layout definitions
//!
//! Provides keyboard layout support for different regions (US, UK, DE, FR, etc.)

/// Keyboard layout trait
pub trait KeyboardLayout {
    /// Get the layout name (e.g., "US", "UK", "DE")
    fn name(&self) -> &'static str;

    /// Convert scancode to character (unshifted)
    fn scancode_to_char(&self, scancode: u8) -> Option<char>;

    /// Convert scancode to character (shifted)
    fn scancode_to_char_shift(&self, scancode: u8) -> Option<char>;

    /// Convert scancode to character (with AltGr for layouts that support it)
    fn scancode_to_char_altgr(&self, scancode: u8) -> Option<char> {
        // Default: no AltGr support
        None
    }
}

/// US QWERTY keyboard layout
pub struct LayoutUS;

impl KeyboardLayout for LayoutUS {
    fn name(&self) -> &'static str {
        "US"
    }

    fn scancode_to_char(&self, scancode: u8) -> Option<char> {
        let ch = match scancode {
            // Letters
            0x1E => 'a', 0x30 => 'b', 0x2E => 'c', 0x20 => 'd',
            0x12 => 'e', 0x21 => 'f', 0x22 => 'g', 0x23 => 'h',
            0x17 => 'i', 0x24 => 'j', 0x25 => 'k', 0x26 => 'l',
            0x32 => 'm', 0x31 => 'n', 0x18 => 'o', 0x19 => 'p',
            0x10 => 'q', 0x13 => 'r', 0x1F => 's', 0x14 => 't',
            0x16 => 'u', 0x2F => 'v', 0x11 => 'w', 0x2D => 'x',
            0x15 => 'y', 0x2C => 'z',

            // Numbers
            0x02 => '1', 0x03 => '2', 0x04 => '3', 0x05 => '4',
            0x06 => '5', 0x07 => '6', 0x08 => '7', 0x09 => '8',
            0x0A => '9', 0x0B => '0',

            // Symbols
            0x0C => '-', 0x0D => '=',
            0x1A => '[', 0x1B => ']',
            0x27 => ';', 0x28 => '\'',
            0x29 => '`', 0x2B => '\\',
            0x33 => ',', 0x34 => '.', 0x35 => '/',

            // Special
            0x39 => ' ',   // Space
            0x1C => '\n',  // Enter
            0x0E => '\x08', // Backspace
            0x0F => '\t',  // Tab

            _ => return None,
        };
        Some(ch)
    }

    fn scancode_to_char_shift(&self, scancode: u8) -> Option<char> {
        let ch = match scancode {
            // Letters (uppercase)
            0x1E => 'A', 0x30 => 'B', 0x2E => 'C', 0x20 => 'D',
            0x12 => 'E', 0x21 => 'F', 0x22 => 'G', 0x23 => 'H',
            0x17 => 'I', 0x24 => 'J', 0x25 => 'K', 0x26 => 'L',
            0x32 => 'M', 0x31 => 'N', 0x18 => 'O', 0x19 => 'P',
            0x10 => 'Q', 0x13 => 'R', 0x1F => 'S', 0x14 => 'T',
            0x16 => 'U', 0x2F => 'V', 0x11 => 'W', 0x2D => 'X',
            0x15 => 'Y', 0x2C => 'Z',

            // Shifted numbers
            0x02 => '!', 0x03 => '@', 0x04 => '#', 0x05 => '$',
            0x06 => '%', 0x07 => '^', 0x08 => '&', 0x09 => '*',
            0x0A => '(', 0x0B => ')',

            // Shifted symbols
            0x0C => '_', 0x0D => '+',
            0x1A => '{', 0x1B => '}',
            0x27 => ':', 0x28 => '"',
            0x29 => '~', 0x2B => '|',
            0x33 => '<', 0x34 => '>', 0x35 => '?',

            // Special (same as unshifted)
            0x39 => ' ',
            0x1C => '\n',
            0x0E => '\x08',
            0x0F => '\t',

            _ => return None,
        };
        Some(ch)
    }
}

/// UK QWERTY keyboard layout
pub struct LayoutUK;

impl KeyboardLayout for LayoutUK {
    fn name(&self) -> &'static str {
        "UK"
    }

    fn scancode_to_char(&self, scancode: u8) -> Option<char> {
        // UK layout is mostly same as US, with some differences
        let ch = match scancode {
            0x28 => '\'',  // Quote key (US: ')
            0x02 => '1', 0x03 => '2', 0x04 => '3', 0x05 => '4',
            0x06 => '5', 0x07 => '6', 0x08 => '7', 0x09 => '8',
            0x0A => '9', 0x0B => '0',
            0x29 => '`',   // Backtick
            0x56 => '\\',  // Extra key next to left shift on UK keyboards
            _ => return LayoutUS.scancode_to_char(scancode),
        };
        Some(ch)
    }

    fn scancode_to_char_shift(&self, scancode: u8) -> Option<char> {
        let ch = match scancode {
            0x28 => '@',   // Shift+quote = @
            0x02 => '!', 0x03 => '"', 0x04 => '£', 0x05 => '$',  // UK: Shift+3 = £
            0x06 => '%', 0x07 => '^', 0x08 => '&', 0x09 => '*',
            0x0A => '(', 0x0B => ')',
            0x29 => '¬',   // Shift+backtick
            0x56 => '|',   // Shift+extra key
            _ => return LayoutUS.scancode_to_char_shift(scancode),
        };
        Some(ch)
    }
}

/// German QWERTZ keyboard layout
pub struct LayoutDE;

impl KeyboardLayout for LayoutDE {
    fn name(&self) -> &'static str {
        "DE"
    }

    fn scancode_to_char(&self, scancode: u8) -> Option<char> {
        let ch = match scancode {
            // Y and Z are swapped (QWERTZ not QWERTY)
            0x15 => 'z', 0x2C => 'y',

            // Numbers
            0x02 => '1', 0x03 => '2', 0x04 => '3', 0x05 => '4',
            0x06 => '5', 0x07 => '6', 0x08 => '7', 0x09 => '8',
            0x0A => '9', 0x0B => '0',

            // German special characters
            0x0C => 'ß',   // Sharp S
            0x1A => 'ü',   // U-umlaut
            0x1B => '+',
            0x27 => 'ö',   // O-umlaut
            0x28 => 'ä',   // A-umlaut
            0x29 => '^',
            0x2B => '#',
            0x56 => '<',   // Extra key

            _ => return LayoutUS.scancode_to_char(scancode),
        };
        Some(ch)
    }

    fn scancode_to_char_shift(&self, scancode: u8) -> Option<char> {
        let ch = match scancode {
            // Y and Z are swapped
            0x15 => 'Z', 0x2C => 'Y',

            // Shifted numbers
            0x02 => '!', 0x03 => '"', 0x04 => '§', 0x05 => '$',
            0x06 => '%', 0x07 => '&', 0x08 => '/', 0x09 => '(',
            0x0A => ')', 0x0B => '=',

            // Shifted special characters
            0x0C => '?',
            0x1A => 'Ü',
            0x1B => '*',
            0x27 => 'Ö',
            0x28 => 'Ä',
            0x29 => '°',
            0x2B => '\'',
            0x56 => '>',

            _ => return LayoutUS.scancode_to_char_shift(scancode),
        };
        Some(ch)
    }

    fn scancode_to_char_altgr(&self, scancode: u8) -> Option<char> {
        // AltGr combinations for German layout
        let ch = match scancode {
            0x03 => '²',   // AltGr+2
            0x04 => '³',   // AltGr+3
            0x08 => '{',   // AltGr+7
            0x09 => '[',   // AltGr+8
            0x0A => ']',   // AltGr+9
            0x0B => '}',   // AltGr+0
            0x0C => '\\',  // AltGr+ß
            0x10 => '@',   // AltGr+Q
            0x12 => '€',   // AltGr+E (Euro symbol)
            0x2B => '~',   // AltGr+#
            0x56 => '|',   // AltGr+<
            _ => return None,
        };
        Some(ch)
    }
}

/// French AZERTY keyboard layout
pub struct LayoutFR;

impl KeyboardLayout for LayoutFR {
    fn name(&self) -> &'static str {
        "FR"
    }

    fn scancode_to_char(&self, scancode: u8) -> Option<char> {
        let ch = match scancode {
            // AZERTY layout (not QWERTY)
            0x10 => 'a', 0x2C => 'w', 0x20 => 'q',
            0x1E => 'q', 0x11 => 'z', 0x32 => ',',

            // Numbers require shift in French layout, unshifted gives symbols
            0x02 => '&', 0x03 => 'é', 0x04 => '"', 0x05 => '\'',
            0x06 => '(', 0x07 => '-', 0x08 => 'è', 0x09 => '_',
            0x0A => 'ç', 0x0B => 'à',

            0x0C => ')', 0x0D => '=',
            0x1A => '^', 0x1B => '$',
            0x27 => 'm', 0x28 => 'ù',
            0x2B => '*', 0x29 => '²',
            0x56 => '<',

            _ => return LayoutUS.scancode_to_char(scancode),
        };
        Some(ch)
    }

    fn scancode_to_char_shift(&self, scancode: u8) -> Option<char> {
        let ch = match scancode {
            // AZERTY uppercase
            0x10 => 'A', 0x2C => 'W', 0x20 => 'Q',
            0x1E => 'Q', 0x11 => 'Z', 0x32 => '?',

            // Shifted = numbers
            0x02 => '1', 0x03 => '2', 0x04 => '3', 0x05 => '4',
            0x06 => '5', 0x07 => '6', 0x08 => '7', 0x09 => '8',
            0x0A => '9', 0x0B => '0',

            0x0C => '°', 0x0D => '+',
            0x27 => 'M', 0x28 => '%',
            0x2B => 'µ', 0x56 => '>',

            _ => return LayoutUS.scancode_to_char_shift(scancode),
        };
        Some(ch)
    }

    fn scancode_to_char_altgr(&self, scancode: u8) -> Option<char> {
        let ch = match scancode {
            0x03 => '~',   // AltGr+é
            0x04 => '#',   // AltGr+"
            0x05 => '{',   // AltGr+'
            0x06 => '[',   // AltGr+(
            0x07 => '|',   // AltGr+-
            0x08 => '`',   // AltGr+è
            0x09 => '\\',  // AltGr+_
            0x0A => '^',   // AltGr+ç
            0x0B => '@',   // AltGr+à
            0x0C => ']',   // AltGr+)
            0x0D => '}',   // AltGr+=
            0x12 => '€',   // AltGr+E (Euro)
            _ => return None,
        };
        Some(ch)
    }
}
