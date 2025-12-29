//! WATOS Keyboard Driver
//!
//! Provides scancode-to-ASCII conversion and keyboard state management.
//! Supports US keyboard layout with modifier keys (Shift, Ctrl, Alt, Caps Lock).

#![no_std]

use spin::Mutex;

/// Keyboard modifier state
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyboardState {
    /// Left shift pressed
    pub left_shift: bool,
    /// Right shift pressed
    pub right_shift: bool,
    /// Left control pressed
    pub left_ctrl: bool,
    /// Right control pressed
    pub right_ctrl: bool,
    /// Left alt pressed
    pub left_alt: bool,
    /// Right alt pressed
    pub right_alt: bool,
    /// Caps lock enabled
    pub caps_lock: bool,
    /// Num lock enabled
    pub num_lock: bool,
    /// Scroll lock enabled
    pub scroll_lock: bool,
}

impl KeyboardState {
    /// Create a new keyboard state with default values
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
        }
    }

    /// Check if any shift key is pressed
    pub fn shift(&self) -> bool {
        self.left_shift || self.right_shift
    }

    /// Check if any ctrl key is pressed
    pub fn ctrl(&self) -> bool {
        self.left_ctrl || self.right_ctrl
    }

    /// Check if any alt key is pressed
    pub fn alt(&self) -> bool {
        self.left_alt || self.right_alt
    }
}

/// Scancode set 1 key codes
pub mod scancodes {
    pub const ESCAPE: u8 = 0x01;
    pub const KEY_1: u8 = 0x02;
    pub const KEY_2: u8 = 0x03;
    pub const KEY_3: u8 = 0x04;
    pub const KEY_4: u8 = 0x05;
    pub const KEY_5: u8 = 0x06;
    pub const KEY_6: u8 = 0x07;
    pub const KEY_7: u8 = 0x08;
    pub const KEY_8: u8 = 0x09;
    pub const KEY_9: u8 = 0x0A;
    pub const KEY_0: u8 = 0x0B;
    pub const MINUS: u8 = 0x0C;
    pub const EQUALS: u8 = 0x0D;
    pub const BACKSPACE: u8 = 0x0E;
    pub const TAB: u8 = 0x0F;
    pub const Q: u8 = 0x10;
    pub const W: u8 = 0x11;
    pub const E: u8 = 0x12;
    pub const R: u8 = 0x13;
    pub const T: u8 = 0x14;
    pub const Y: u8 = 0x15;
    pub const U: u8 = 0x16;
    pub const I: u8 = 0x17;
    pub const O: u8 = 0x18;
    pub const P: u8 = 0x19;
    pub const LEFT_BRACKET: u8 = 0x1A;
    pub const RIGHT_BRACKET: u8 = 0x1B;
    pub const ENTER: u8 = 0x1C;
    pub const LEFT_CTRL: u8 = 0x1D;
    pub const A: u8 = 0x1E;
    pub const S: u8 = 0x1F;
    pub const D: u8 = 0x20;
    pub const F: u8 = 0x21;
    pub const G: u8 = 0x22;
    pub const H: u8 = 0x23;
    pub const J: u8 = 0x24;
    pub const K: u8 = 0x25;
    pub const L: u8 = 0x26;
    pub const SEMICOLON: u8 = 0x27;
    pub const APOSTROPHE: u8 = 0x28;
    pub const BACKTICK: u8 = 0x29;
    pub const LEFT_SHIFT: u8 = 0x2A;
    pub const BACKSLASH: u8 = 0x2B;
    pub const Z: u8 = 0x2C;
    pub const X: u8 = 0x2D;
    pub const C: u8 = 0x2E;
    pub const V: u8 = 0x2F;
    pub const B: u8 = 0x30;
    pub const N: u8 = 0x31;
    pub const M: u8 = 0x32;
    pub const COMMA: u8 = 0x33;
    pub const PERIOD: u8 = 0x34;
    pub const SLASH: u8 = 0x35;
    pub const RIGHT_SHIFT: u8 = 0x36;
    pub const KEYPAD_STAR: u8 = 0x37;
    pub const LEFT_ALT: u8 = 0x38;
    pub const SPACE: u8 = 0x39;
    pub const CAPS_LOCK: u8 = 0x3A;
    pub const F1: u8 = 0x3B;
    pub const F2: u8 = 0x3C;
    pub const F3: u8 = 0x3D;
    pub const F4: u8 = 0x3E;
    pub const F5: u8 = 0x3F;
    pub const F6: u8 = 0x40;
    pub const F7: u8 = 0x41;
    pub const F8: u8 = 0x42;
    pub const F9: u8 = 0x43;
    pub const F10: u8 = 0x44;
    pub const NUM_LOCK: u8 = 0x45;
    pub const SCROLL_LOCK: u8 = 0x46;

    /// Release flag (OR'd with scancode)
    pub const RELEASE: u8 = 0x80;
}

/// US keyboard layout scancode to ASCII mapping
static SCANCODE_TO_ASCII: [u8; 128] = [
    0,    27,  b'1', b'2', b'3', b'4', b'5', b'6',     // 0x00-0x07
    b'7', b'8', b'9', b'0', b'-', b'=', 8,   b'\t',    // 0x08-0x0F (8=backspace)
    b'q', b'w', b'e', b'r', b't', b'y', b'u', b'i',    // 0x10-0x17
    b'o', b'p', b'[', b']', b'\n', 0,  b'a', b's',     // 0x18-0x1F (\n=enter, 0=ctrl)
    b'd', b'f', b'g', b'h', b'j', b'k', b'l', b';',    // 0x20-0x27
    b'\'', b'`', 0,  b'\\', b'z', b'x', b'c', b'v',    // 0x28-0x2F (0=shift)
    b'b', b'n', b'm', b',', b'.', b'/', 0,   b'*',     // 0x30-0x37 (0=rshift)
    0,   b' ', 0,   0,   0,   0,   0,   0,             // 0x38-0x3F (0=alt,caps,F1-F5)
    0,   0,   0,   0,   0,   0,   0,   b'7',           // 0x40-0x47 (F6-F10, numlock, scroll, kp7)
    b'8', b'9', b'-', b'4', b'5', b'6', b'+', b'1',    // 0x48-0x4F (kp8,9,-,4,5,6,+,1)
    b'2', b'3', b'0', b'.', 0,   0,   0,   0,           // 0x50-0x57 (kp2,3,0,.)
    0,   0,   0,   0,   0,   0,   0,   0,              // 0x58-0x5F
    0,   0,   0,   0,   0,   0,   0,   0,              // 0x60-0x67
    0,   0,   0,   0,   0,   0,   0,   0,              // 0x68-0x6F
    0,   0,   0,   0,   0,   0,   0,   0,              // 0x70-0x77
    0,   0,   0,   0,   0,   0,   0,   0,              // 0x78-0x7F
];

/// Shifted scancode to ASCII mapping (when shift is pressed)
static SCANCODE_TO_ASCII_SHIFT: [u8; 128] = [
    0,    27,  b'!', b'@', b'#', b'$', b'%', b'^',     // 0x00-0x07
    b'&', b'*', b'(', b')', b'_', b'+', 8,   b'\t',    // 0x08-0x0F
    b'Q', b'W', b'E', b'R', b'T', b'Y', b'U', b'I',    // 0x10-0x17
    b'O', b'P', b'{', b'}', b'\n', 0,  b'A', b'S',     // 0x18-0x1F
    b'D', b'F', b'G', b'H', b'J', b'K', b'L', b':',    // 0x20-0x27
    b'"', b'~', 0,  b'|', b'Z', b'X', b'C', b'V',      // 0x28-0x2F
    b'B', b'N', b'M', b'<', b'>', b'?', 0,   b'*',     // 0x30-0x37
    0,   b' ', 0,   0,   0,   0,   0,   0,             // 0x38-0x3F
    0,   0,   0,   0,   0,   0,   0,   b'7',           // 0x40-0x47
    b'8', b'9', b'-', b'4', b'5', b'6', b'+', b'1',    // 0x48-0x4F
    b'2', b'3', b'0', b'.', 0,   0,   0,   0,           // 0x50-0x57
    0,   0,   0,   0,   0,   0,   0,   0,              // 0x58-0x5F
    0,   0,   0,   0,   0,   0,   0,   0,              // 0x60-0x67
    0,   0,   0,   0,   0,   0,   0,   0,              // 0x68-0x6F
    0,   0,   0,   0,   0,   0,   0,   0,              // 0x70-0x77
    0,   0,   0,   0,   0,   0,   0,   0,              // 0x78-0x7F
];

/// Global keyboard state
static KEYBOARD_STATE: Mutex<KeyboardState> = Mutex::new(KeyboardState::new());

/// Process a scancode and update keyboard state
///
/// Returns Some(ascii) if the scancode represents a printable character,
/// or None for modifier keys and special keys.
pub fn process_scancode(scancode: u8) -> Option<u8> {
    let mut state = KEYBOARD_STATE.lock();
    let is_release = (scancode & scancodes::RELEASE) != 0;
    let key = scancode & !scancodes::RELEASE;

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

    // Only process key press events (not releases) for ASCII conversion
    if is_release {
        return None;
    }

    // Convert scancode to ASCII
    let ascii = if state.shift() {
        SCANCODE_TO_ASCII_SHIFT[key as usize]
    } else {
        SCANCODE_TO_ASCII[key as usize]
    };

    // Apply caps lock for letters
    if ascii != 0 {
        if state.caps_lock && ascii >= b'a' && ascii <= b'z' {
            // Caps lock inverts shift for letters
            if state.shift() {
                Some(ascii) // Shift + caps = lowercase
            } else {
                Some(ascii - 32) // Just caps = uppercase
            }
        } else if state.caps_lock && ascii >= b'A' && ascii <= b'Z' {
            // Caps lock inverts shift for letters
            if state.shift() {
                Some(ascii + 32) // Shift + caps = lowercase
            } else {
                Some(ascii) // Just caps = uppercase
            }
        } else {
            Some(ascii)
        }
    } else {
        None
    }
}

/// Get the current keyboard state
pub fn get_state() -> KeyboardState {
    *KEYBOARD_STATE.lock()
}

/// Reset keyboard state (useful for testing or initialization)
pub fn reset_state() {
    *KEYBOARD_STATE.lock() = KeyboardState::new();
}

/// Check if a key is currently pressed (for raw scancode queries)
pub fn is_key_pressed(scancode: u8) -> bool {
    let state = KEYBOARD_STATE.lock();
    match scancode {
        scancodes::LEFT_SHIFT => state.left_shift,
        scancodes::RIGHT_SHIFT => state.right_shift,
        scancodes::LEFT_CTRL => state.left_ctrl,
        scancodes::LEFT_ALT => state.left_alt,
        _ => false,
    }
}
