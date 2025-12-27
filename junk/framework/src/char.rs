//! Character device driver abstraction
//!
//! For serial ports, keyboards, etc.

use crate::{Driver, DriverError};

/// Character device trait
pub trait CharDevice: Driver {
    /// Read bytes from the device
    ///
    /// # Returns
    /// Number of bytes read, or 0 if no data available
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, DriverError>;

    /// Write bytes to the device
    ///
    /// # Returns
    /// Number of bytes written
    fn write(&mut self, buffer: &[u8]) -> Result<usize, DriverError>;

    /// Check if data is available to read
    fn has_data(&self) -> bool;

    /// Check if device can accept more data
    fn can_write(&self) -> bool;

    /// Flush output buffer
    fn flush(&mut self) -> Result<(), DriverError> {
        Ok(())
    }
}

/// Input device trait (keyboard, mouse)
pub trait InputDevice: Driver {
    /// Get next input event
    fn poll_event(&mut self) -> Option<InputEvent>;

    /// Check if events are available
    fn has_events(&self) -> bool;
}

/// Input event types
#[derive(Debug, Clone, Copy)]
pub enum InputEvent {
    /// Key press (scancode, pressed)
    Key { scancode: u8, pressed: bool },
    /// Mouse movement (dx, dy)
    MouseMove { dx: i16, dy: i16 },
    /// Mouse button (button, pressed)
    MouseButton { button: u8, pressed: bool },
    /// Mouse scroll (delta)
    MouseScroll { delta: i8 },
}

/// Character device identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharDeviceId(pub u32);
