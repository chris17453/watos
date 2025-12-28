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

extern crate alloc;
use alloc::vec::Vec;

// Re-export all trait modules
pub mod block;
pub mod nic;
pub mod input;
pub mod video;
pub mod audio;
mod debug;
pub mod bus;

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
    /// Invalid state for this operation
    InvalidState,
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

/// Driver lifecycle state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverState {
    /// Driver is loaded but not initialized
    Loaded,
    /// Driver is initialized and ready to start
    Ready,
    /// Driver is active and operational
    Active,
    /// Driver is stopped
    Stopped,
    /// Driver encountered an error
    Error,
}

/// Static driver information
#[derive(Debug, Clone, Copy)]
pub struct DriverInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub author: &'static str,
    pub description: &'static str,
}

/// Base trait for all drivers
pub trait Driver {
    /// Get driver information
    fn info(&self) -> DriverInfo;

    /// Get current driver state
    fn state(&self) -> DriverState;

    /// Initialize the driver
    fn init(&mut self) -> Result<(), DriverError>;

    /// Start the driver (after init)
    fn start(&mut self) -> Result<(), DriverError>;

    /// Stop the driver
    fn stop(&mut self) -> Result<(), DriverError>;
}
