//! Hardware Driver Traits for WATOS
//!
//! This crate defines the trait interfaces that hardware drivers implement.
//! Subsystems (storage, network, etc.) use these traits to interact with
//! hardware without knowing the specific driver implementation.
//!
//! # Debug Features
//!
//! Enable debug output for specific subsystems at compile time:
//! ```toml
//! watos-driver-traits = { path = "...", features = ["debug-storage"] }
//! ```
//!
//! Available features:
//! - `debug-all`: Enable all debug output
//! - `debug-storage`: BlockDevice operations
//! - `debug-network`: NicDevice operations
//! - `debug-input`: InputDevice operations
//! - `debug-video`: VideoDevice operations
//! - `debug-audio`: AudioDevice operations
//! - `debug-bus`: Bus enumeration

#![no_std]

// Re-export all trait modules
mod block;
mod nic;
mod input;
mod video;
mod audio;
mod debug;

pub use block::*;
pub use nic::*;
pub use input::*;
pub use video::*;
pub use audio::*;
pub use debug::*;

/// Common error type for driver operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverError {
    /// Device not found or not responding
    DeviceNotFound,
    /// Operation timed out
    Timeout,
    /// Invalid parameter
    InvalidParameter,
    /// Device busy
    Busy,
    /// I/O error
    IoError,
    /// Not supported by this device
    NotSupported,
    /// Buffer too small
    BufferTooSmall,
    /// Device-specific error
    DeviceError(u32),
}

pub type DriverResult<T> = Result<T, DriverError>;
