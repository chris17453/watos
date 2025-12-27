//! Input Device Trait
//!
//! Implemented by input drivers (PS/2, USB HID, etc.)
//! Used by the console/input subsystem

use crate::DriverResult;

/// Keyboard scancode (raw hardware code)
pub type Scancode = u8;

/// Input event types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    /// Key pressed (scancode)
    KeyDown(Scancode),
    /// Key released (scancode)
    KeyUp(Scancode),
    /// Mouse moved (dx, dy)
    MouseMove(i16, i16),
    /// Mouse button pressed (button number)
    MouseDown(u8),
    /// Mouse button released (button number)
    MouseUp(u8),
    /// Mouse scroll (delta)
    MouseScroll(i8),
}

/// Input device trait
pub trait InputDevice: Send + Sync {
    /// Poll for input events (non-blocking)
    ///
    /// # Returns
    /// * `Ok(Some(event))` - Event available
    /// * `Ok(None)` - No event available
    /// * `Err(...)` - Error occurred
    fn poll_event(&self) -> DriverResult<Option<InputEvent>>;

    /// Check if device has pending events
    fn has_events(&self) -> bool;

    /// Get device information
    fn info(&self) -> InputDeviceInfo;

    /// Set keyboard LEDs (if applicable)
    fn set_leds(&self, _caps: bool, _num: bool, _scroll: bool) -> DriverResult<()> {
        Ok(()) // Default: ignore
    }
}

/// Information about an input device
#[derive(Debug, Clone)]
pub struct InputDeviceInfo {
    /// Device name
    pub name: &'static str,
    /// Device type
    pub device_type: InputDeviceType,
}

/// Type of input device
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputDeviceType {
    Keyboard,
    Mouse,
    Touchpad,
    Gamepad,
    Other,
}

/// Common scancodes (Set 1)
pub mod scancodes {
    pub const ESC: u8 = 0x01;
    pub const BACKSPACE: u8 = 0x0E;
    pub const TAB: u8 = 0x0F;
    pub const ENTER: u8 = 0x1C;
    pub const LCTRL: u8 = 0x1D;
    pub const LSHIFT: u8 = 0x2A;
    pub const RSHIFT: u8 = 0x36;
    pub const LALT: u8 = 0x38;
    pub const SPACE: u8 = 0x39;
    pub const CAPS: u8 = 0x3A;
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
    pub const F11: u8 = 0x57;
    pub const F12: u8 = 0x58;
    pub const UP: u8 = 0x48;
    pub const DOWN: u8 = 0x50;
    pub const LEFT: u8 = 0x4B;
    pub const RIGHT: u8 = 0x4D;
    pub const HOME: u8 = 0x47;
    pub const END: u8 = 0x4F;
    pub const PGUP: u8 = 0x49;
    pub const PGDN: u8 = 0x51;
    pub const INSERT: u8 = 0x52;
    pub const DELETE: u8 = 0x53;
}
