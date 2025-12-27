//! Debug macros for driver subsystems
//!
//! These macros compile to nothing when debug features are disabled.

/// Debug print for storage subsystem
#[macro_export]
#[cfg(feature = "debug-storage")]
macro_rules! debug_storage {
    ($($arg:tt)*) => {
        // Will use serial_write when integrated
        // For now, this is a placeholder that can be hooked up
        $crate::_debug_print("[STORAGE] ", format_args!($($arg)*))
    };
}

#[macro_export]
#[cfg(not(feature = "debug-storage"))]
macro_rules! debug_storage {
    ($($arg:tt)*) => {};
}

/// Debug print for network subsystem
#[macro_export]
#[cfg(feature = "debug-network")]
macro_rules! debug_network {
    ($($arg:tt)*) => {
        $crate::_debug_print("[NETWORK] ", format_args!($($arg)*))
    };
}

#[macro_export]
#[cfg(not(feature = "debug-network"))]
macro_rules! debug_network {
    ($($arg:tt)*) => {};
}

/// Debug print for input subsystem
#[macro_export]
#[cfg(feature = "debug-input")]
macro_rules! debug_input {
    ($($arg:tt)*) => {
        $crate::_debug_print("[INPUT] ", format_args!($($arg)*))
    };
}

#[macro_export]
#[cfg(not(feature = "debug-input"))]
macro_rules! debug_input {
    ($($arg:tt)*) => {};
}

/// Debug print for video subsystem
#[macro_export]
#[cfg(feature = "debug-video")]
macro_rules! debug_video {
    ($($arg:tt)*) => {
        $crate::_debug_print("[VIDEO] ", format_args!($($arg)*))
    };
}

#[macro_export]
#[cfg(not(feature = "debug-video"))]
macro_rules! debug_video {
    ($($arg:tt)*) => {};
}

/// Debug print for audio subsystem
#[macro_export]
#[cfg(feature = "debug-audio")]
macro_rules! debug_audio {
    ($($arg:tt)*) => {
        $crate::_debug_print("[AUDIO] ", format_args!($($arg)*))
    };
}

#[macro_export]
#[cfg(not(feature = "debug-audio"))]
macro_rules! debug_audio {
    ($($arg:tt)*) => {};
}

/// Debug print for bus enumeration
#[macro_export]
#[cfg(feature = "debug-bus")]
macro_rules! debug_bus {
    ($($arg:tt)*) => {
        $crate::_debug_print("[BUS] ", format_args!($($arg)*))
    };
}

#[macro_export]
#[cfg(not(feature = "debug-bus"))]
macro_rules! debug_bus {
    ($($arg:tt)*) => {};
}

/// Debug output function - can be replaced with actual serial output
#[doc(hidden)]
#[cfg(any(
    feature = "debug-storage",
    feature = "debug-network",
    feature = "debug-input",
    feature = "debug-video",
    feature = "debug-audio",
    feature = "debug-bus"
))]
pub fn _debug_print(_prefix: &str, _args: core::fmt::Arguments) {
    // This will be hooked up to serial output
    // For now it's a no-op that can be connected later
}
