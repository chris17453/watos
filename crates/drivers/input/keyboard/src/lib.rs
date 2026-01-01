//! WATOS Keyboard Driver
//!
//! Professional keyboard driver with support for:
//! - Multiple keyboard layouts (US, UK, DE, FR, etc.)
//! - Code page support (CP437, CP850, CP1252, UTF-8)
//! - Dynamic keymap/codepage loading from files
//! - Full modifier key support (Shift, Ctrl, Alt, AltGr, Caps Lock)
//! - Proper key state tracking

#![no_std]

use spin::Mutex;

pub mod layouts;
pub mod codepage;

pub use layouts::*;
pub use codepage::*;

/// Keyboard modifier state
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyboardState {
    /// Left shift pressed
    pub left_shift: bool,
    /// Right shift pressed
    pub right_shift: bool,
    /// Left control pressed
    pub left_ctrl: bool,
    /// Right control pressed (extended scancode)
    pub right_ctrl: bool,
    /// Left alt pressed
    pub left_alt: bool,
    /// Right alt (AltGr) pressed (extended scancode)
    pub right_alt: bool,
    /// Caps lock enabled
    pub caps_lock: bool,
    /// Num lock enabled
    pub num_lock: bool,
    /// Scroll lock enabled
    pub scroll_lock: bool,
    /// Extended scancode (E0 prefix) received
    pub extended: bool,
}

impl KeyboardState {
    pub const fn new() -> Self {
        Self {
            left_shift: false,
            right_shift: false,
            left_ctrl: false,
            right_ctrl: false,
            left_alt: false,
            right_alt: false,
            caps_lock: false,
            num_lock: false,
            scroll_lock: false,
            extended: false,
        }
    }

    pub fn shift(&self) -> bool {
        self.left_shift || self.right_shift
    }

    pub fn ctrl(&self) -> bool {
        self.left_ctrl || self.right_ctrl
    }

    pub fn alt(&self) -> bool {
        self.left_alt || self.right_alt
    }

    pub fn altgr(&self) -> bool {
        self.right_alt
    }
}

/// Scancode set 1 key codes
pub mod scancodes {
    pub const ESCAPE: u8 = 0x01;
    pub const BACKSPACE: u8 = 0x0E;
    pub const TAB: u8 = 0x0F;
    pub const ENTER: u8 = 0x1C;
    pub const LEFT_CTRL: u8 = 0x1D;
    pub const LEFT_SHIFT: u8 = 0x2A;
    pub const RIGHT_SHIFT: u8 = 0x36;
    pub const LEFT_ALT: u8 = 0x38;
    pub const SPACE: u8 = 0x39;
    pub const CAPS_LOCK: u8 = 0x3A;
    pub const NUM_LOCK: u8 = 0x45;
    pub const SCROLL_LOCK: u8 = 0x46;

    // Extended scancodes (E0 prefix)
    pub const EXTENDED: u8 = 0xE0;
    pub const RIGHT_CTRL: u8 = 0x1D;  // When preceded by E0
    pub const RIGHT_ALT: u8 = 0x38;   // When preceded by E0 (AltGr)

    pub const RELEASE: u8 = 0x80;
}

/// Dynamic keyboard layout storage
/// Maps scancodes (0-255) to Unicode codepoints (chars)
struct DynamicKeymap {
    /// Whether a custom keymap is loaded
    loaded: bool,
    /// Layout name (max 32 chars)
    name: [u8; 32],
    name_len: usize,
    /// Normal (unshifted) map: scancode -> char (0 = no mapping)
    normal: [u32; 256],
    /// Shifted map: scancode -> char
    shift: [u32; 256],
    /// AltGr map: scancode -> char
    altgr: [u32; 256],
}

impl DynamicKeymap {
    const fn new() -> Self {
        Self {
            loaded: false,
            name: [0; 32],
            name_len: 0,
            normal: [0; 256],
            shift: [0; 256],
            altgr: [0; 256],
        }
    }
}

/// Dynamic code page storage
/// Maps byte values (0-255) to Unicode codepoints
struct DynamicCodepage {
    /// Whether a custom codepage is loaded
    loaded: bool,
    /// Code page ID
    id: u16,
    /// Name (max 32 chars)
    name: [u8; 32],
    name_len: usize,
    /// Byte to unicode map
    to_unicode: [u32; 256],
}

impl DynamicCodepage {
    const fn new() -> Self {
        Self {
            loaded: false,
            id: 437,
            name: [0; 32],
            name_len: 0,
            to_unicode: [0; 256],
        }
    }
}

/// Keyboard driver configuration
pub struct KeyboardDriver {
    state: Mutex<KeyboardState>,
    layout: Mutex<KeyboardLayout_>,
    codepage: Mutex<CodePage_>,
    dynamic_keymap: Mutex<DynamicKeymap>,
    dynamic_codepage: Mutex<DynamicCodepage>,
}

/// Built-in keyboard layout enum
#[derive(Clone, Copy)]
enum KeyboardLayout_ {
    US,
    UK,
    DE,
    FR,
    Custom, // Use dynamic keymap
}

/// Built-in code page enum
#[derive(Clone, Copy)]
enum CodePage_ {
    CP437,
    CP850,
    CP1252,
    UTF8,
    Custom, // Use dynamic codepage
}

impl KeyboardDriver {
    pub const fn new() -> Self {
        Self {
            state: Mutex::new(KeyboardState::new()),
            layout: Mutex::new(KeyboardLayout_::US),
            codepage: Mutex::new(CodePage_::CP437),
            dynamic_keymap: Mutex::new(DynamicKeymap::new()),
            dynamic_codepage: Mutex::new(DynamicCodepage::new()),
        }
    }

    /// Set the keyboard layout by name
    pub fn set_layout(&self, layout: &str) {
        let mut l = self.layout.lock();
        *l = match layout {
            "UK" | "uk" => KeyboardLayout_::UK,
            "DE" | "de" => KeyboardLayout_::DE,
            "FR" | "fr" => KeyboardLayout_::FR,
            "CUSTOM" | "custom" => KeyboardLayout_::Custom,
            _ => KeyboardLayout_::US,
        };
    }

    /// Set the code page by ID
    pub fn set_codepage(&self, codepage: u16) {
        let mut cp = self.codepage.lock();
        *cp = match codepage {
            850 => CodePage_::CP850,
            1252 => CodePage_::CP1252,
            65001 => CodePage_::UTF8,
            0 => CodePage_::Custom, // Special value for custom
            _ => CodePage_::CP437,  // Default to CP437
        };
    }

    /// Get current layout name
    pub fn get_layout_name(&self) -> &'static str {
        match *self.layout.lock() {
            KeyboardLayout_::US => "US",
            KeyboardLayout_::UK => "UK",
            KeyboardLayout_::DE => "DE",
            KeyboardLayout_::FR => "FR",
            KeyboardLayout_::Custom => "CUSTOM",
        }
    }

    /// Get current code page ID
    pub fn get_codepage_id(&self) -> u16 {
        match *self.codepage.lock() {
            CodePage_::CP437 => 437,
            CodePage_::CP850 => 850,
            CodePage_::CP1252 => 1252,
            CodePage_::UTF8 => 65001,
            CodePage_::Custom => self.dynamic_codepage.lock().id,
        }
    }

    /// Load a dynamic keymap from binary data
    /// Format: KMAP (4 bytes) + version (1) + name_len (1) + name +
    ///         normal_map (256 bytes) + shift_map (256 bytes) + altgr_map (256 bytes)
    pub fn load_keymap(&self, data: &[u8]) -> Result<(), &'static str> {
        if data.len() < 6 {
            return Err("Data too short");
        }

        // Verify magic
        if &data[0..4] != b"KMAP" {
            return Err("Invalid magic");
        }

        // Check version
        if data[4] != 1 {
            return Err("Unsupported version");
        }

        let name_len = data[5] as usize;
        if name_len > 32 {
            return Err("Name too long");
        }

        let expected_len = 6 + name_len + 768;
        if data.len() < expected_len {
            return Err("Data too short for maps");
        }

        let mut keymap = self.dynamic_keymap.lock();

        // Copy name
        keymap.name_len = name_len;
        for i in 0..name_len {
            keymap.name[i] = data[6 + i];
        }

        // Extract the three 256-byte maps
        let offset = 6 + name_len;

        // Parse maps - each byte in the file represents a character
        // 0 = no mapping, otherwise treat as Latin-1/ASCII code
        for i in 0..256 {
            let normal_byte = data[offset + i];
            let shift_byte = data[offset + 256 + i];
            let altgr_byte = data[offset + 512 + i];

            // Map bytes to unicode codepoints (simple 1:1 for Latin-1)
            keymap.normal[i] = if normal_byte == 0 { 0 } else { normal_byte as u32 };
            keymap.shift[i] = if shift_byte == 0 { 0 } else { shift_byte as u32 };
            keymap.altgr[i] = if altgr_byte == 0 { 0 } else { altgr_byte as u32 };
        }

        keymap.loaded = true;

        // Switch to custom layout
        *self.layout.lock() = KeyboardLayout_::Custom;

        Ok(())
    }

    /// Load a dynamic codepage from binary data
    /// Format: CPAG (4 bytes) + version (1) + id (2 LE) + name_len (1) + name +
    ///         byte_to_unicode (256 * 4 bytes UTF-32 LE)
    pub fn load_codepage(&self, data: &[u8]) -> Result<(), &'static str> {
        if data.len() < 8 {
            return Err("Data too short");
        }

        // Verify magic
        if &data[0..4] != b"CPAG" {
            return Err("Invalid magic");
        }

        // Check version
        if data[4] != 1 {
            return Err("Unsupported version");
        }

        // Extract code page ID (little-endian)
        let cp_id = u16::from_le_bytes([data[5], data[6]]);
        let name_len = data[7] as usize;

        if name_len > 32 {
            return Err("Name too long");
        }

        let expected_len = 8 + name_len + 1024;
        if data.len() < expected_len {
            return Err("Data too short for character map");
        }

        let mut codepage = self.dynamic_codepage.lock();

        codepage.id = cp_id;
        codepage.name_len = name_len;

        // Copy name
        for i in 0..name_len {
            codepage.name[i] = data[8 + i];
        }

        // Extract the 256 * 4 byte character map (UTF-32 LE)
        let offset = 8 + name_len;
        for i in 0..256 {
            let base = offset + i * 4;
            let codepoint = u32::from_le_bytes([
                data[base],
                data[base + 1],
                data[base + 2],
                data[base + 3],
            ]);
            codepage.to_unicode[i] = codepoint;
        }

        codepage.loaded = true;

        // Switch to custom codepage
        *self.codepage.lock() = CodePage_::Custom;

        Ok(())
    }

    /// Process a raw PS/2 scancode
    pub fn process_scancode(&self, scancode: u8) -> Option<u8> {
        let mut state = self.state.lock();

        // Handle extended scancode prefix (E0)
        if scancode == scancodes::EXTENDED {
            state.extended = true;
            return None;
        }

        let is_release = (scancode & scancodes::RELEASE) != 0;
        let key = scancode & !scancodes::RELEASE;

        // Handle extended keys (right ctrl, right alt/AltGr)
        if state.extended {
            state.extended = false;
            match key {
                scancodes::RIGHT_CTRL => {
                    state.right_ctrl = !is_release;
                    return None;
                }
                scancodes::RIGHT_ALT => {
                    state.right_alt = !is_release;  // AltGr
                    return None;
                }
                _ => {}
            }
        }

        // Handle modifier keys
        match key {
            scancodes::LEFT_SHIFT => {
                state.left_shift = !is_release;
                return None;
            }
            scancodes::RIGHT_SHIFT => {
                state.right_shift = !is_release;
                return None;
            }
            scancodes::LEFT_CTRL => {
                state.left_ctrl = !is_release;
                return None;
            }
            scancodes::LEFT_ALT => {
                state.left_alt = !is_release;
                return None;
            }
            scancodes::CAPS_LOCK => {
                if !is_release {
                    state.caps_lock = !state.caps_lock;
                }
                return None;
            }
            scancodes::NUM_LOCK => {
                if !is_release {
                    state.num_lock = !state.num_lock;
                }
                return None;
            }
            scancodes::SCROLL_LOCK => {
                if !is_release {
                    state.scroll_lock = !state.scroll_lock;
                }
                return None;
            }
            _ => {}
        }

        // Only process key presses (not releases)
        if is_release {
            return None;
        }

        // Convert scancode to character using current layout
        let layout = *self.layout.lock();
        let ch = if state.altgr() {
            // Try AltGr first (for layouts that support it)
            self.get_char_altgr(&layout, key)
                .or_else(|| self.get_char(&layout, key, state.shift()))
        } else {
            self.get_char(&layout, key, state.shift())
        };

        // Apply caps lock for letters
        let ch = ch.and_then(|c| {
            if state.caps_lock && c.is_alphabetic() {
                if state.shift() {
                    // Shift + Caps = invert case
                    if c.is_uppercase() {
                        Some(c.to_lowercase().next().unwrap_or(c))
                    } else {
                        Some(c.to_uppercase().next().unwrap_or(c))
                    }
                } else {
                    // Just caps = uppercase
                    Some(c.to_uppercase().next().unwrap_or(c))
                }
            } else {
                Some(c)
            }
        });

        // Convert character to byte using current code page
        ch.and_then(|c| {
            let codepage = *self.codepage.lock();
            self.char_to_byte(&codepage, c)
        })
    }

    fn get_char(&self, layout: &KeyboardLayout_, scancode: u8, shift: bool) -> Option<char> {
        match layout {
            KeyboardLayout_::US => {
                let layout = LayoutUS;
                if shift {
                    layout.scancode_to_char_shift(scancode)
                } else {
                    layout.scancode_to_char(scancode)
                }
            }
            KeyboardLayout_::UK => {
                let layout = LayoutUK;
                if shift {
                    layout.scancode_to_char_shift(scancode)
                } else {
                    layout.scancode_to_char(scancode)
                }
            }
            KeyboardLayout_::DE => {
                let layout = LayoutDE;
                if shift {
                    layout.scancode_to_char_shift(scancode)
                } else {
                    layout.scancode_to_char(scancode)
                }
            }
            KeyboardLayout_::FR => {
                let layout = LayoutFR;
                if shift {
                    layout.scancode_to_char_shift(scancode)
                } else {
                    layout.scancode_to_char(scancode)
                }
            }
            KeyboardLayout_::Custom => {
                let keymap = self.dynamic_keymap.lock();
                if !keymap.loaded {
                    return None;
                }
                let codepoint = if shift {
                    keymap.shift[scancode as usize]
                } else {
                    keymap.normal[scancode as usize]
                };
                if codepoint == 0 {
                    None
                } else {
                    char::from_u32(codepoint)
                }
            }
        }
    }

    fn get_char_altgr(&self, layout: &KeyboardLayout_, scancode: u8) -> Option<char> {
        match layout {
            KeyboardLayout_::DE => LayoutDE.scancode_to_char_altgr(scancode),
            KeyboardLayout_::FR => LayoutFR.scancode_to_char_altgr(scancode),
            KeyboardLayout_::Custom => {
                let keymap = self.dynamic_keymap.lock();
                if !keymap.loaded {
                    return None;
                }
                let codepoint = keymap.altgr[scancode as usize];
                if codepoint == 0 {
                    None
                } else {
                    char::from_u32(codepoint)
                }
            }
            _ => None,  // US and UK don't typically use AltGr
        }
    }

    fn char_to_byte(&self, codepage: &CodePage_, ch: char) -> Option<u8> {
        match codepage {
            CodePage_::CP437 => CodePage437.from_unicode(ch),
            CodePage_::CP850 => CodePage850.from_unicode(ch),
            CodePage_::CP1252 => CodePage1252.from_unicode(ch),
            CodePage_::UTF8 => CodePageUTF8.from_unicode(ch),
            CodePage_::Custom => {
                // For custom codepage, we need reverse lookup
                // Simple implementation: search for the character
                let cp = self.dynamic_codepage.lock();
                if !cp.loaded {
                    return None;
                }
                let target = ch as u32;
                for i in 0..256 {
                    if cp.to_unicode[i] == target {
                        return Some(i as u8);
                    }
                }
                // ASCII fallback
                if target <= 127 {
                    Some(target as u8)
                } else {
                    None
                }
            }
        }
    }

    /// Get the current keyboard state
    pub fn get_state(&self) -> KeyboardState {
        *self.state.lock()
    }

    /// Reset keyboard state
    pub fn reset_state(&self) {
        *self.state.lock() = KeyboardState::new();
    }
}

/// Global keyboard driver instance
static KEYBOARD: KeyboardDriver = KeyboardDriver::new();

/// Process a scancode and return ASCII character (or 0 if none)
pub fn process_scancode(scancode: u8) -> u8 {
    KEYBOARD.process_scancode(scancode).unwrap_or(0)
}

/// Set the keyboard layout
pub fn set_layout(layout: &str) {
    KEYBOARD.set_layout(layout);
}

/// Set the code page
pub fn set_codepage(codepage: u16) {
    KEYBOARD.set_codepage(codepage);
}

/// Get current layout name
pub fn get_layout() -> &'static str {
    KEYBOARD.get_layout_name()
}

/// Get current code page ID
pub fn get_codepage() -> u16 {
    KEYBOARD.get_codepage_id()
}

/// Get keyboard state
pub fn get_state() -> KeyboardState {
    KEYBOARD.get_state()
}

/// Reset keyboard state
pub fn reset_state() {
    KEYBOARD.reset_state()
}

/// Load keymap from binary data
/// Format: KMAP (4 bytes) + version (1) + name_len (1) + name + normal_map (256) + shift_map (256) + altgr_map (256)
pub fn load_keymap(data: &[u8]) -> Result<(), &'static str> {
    KEYBOARD.load_keymap(data)
}

/// Load codepage from binary data
/// Format: CPAG (4 bytes) + version (1) + id (2 le) + name_len (1) + name + byte_to_unicode_map (256 * 4 bytes UTF-32 LE)
pub fn load_codepage(data: &[u8]) -> Result<(), &'static str> {
    KEYBOARD.load_codepage(data)
}
