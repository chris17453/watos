//! WATOS Driver Framework
//!
//! Provides abstractions for device drivers:
//! - Driver trait for lifecycle management
//! - Device type traits (BlockDevice, NetworkDevice, CharDevice)
//! - Driver registry for discovery and enumeration

#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use alloc::string::String;

pub mod block;
pub mod net;
pub mod char;
pub mod bus;

/// Driver lifecycle states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverState {
    /// Driver loaded but not initialized
    Loaded,
    /// Driver initialized and ready
    Ready,
    /// Driver is active/running
    Active,
    /// Driver suspended (power saving)
    Suspended,
    /// Driver error state
    Error,
}

/// Common driver information
#[derive(Debug, Clone)]
pub struct DriverInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub author: &'static str,
    pub description: &'static str,
}

/// Base trait for all drivers
pub trait Driver: Send + Sync {
    /// Get driver information
    fn info(&self) -> DriverInfo;

    /// Get current driver state
    fn state(&self) -> DriverState;

    /// Initialize the driver
    fn init(&mut self) -> Result<(), DriverError>;

    /// Start/activate the driver
    fn start(&mut self) -> Result<(), DriverError>;

    /// Stop the driver
    fn stop(&mut self) -> Result<(), DriverError>;

    /// Suspend the driver (power management)
    fn suspend(&mut self) -> Result<(), DriverError> {
        Ok(()) // Default: no-op
    }

    /// Resume the driver (power management)
    fn resume(&mut self) -> Result<(), DriverError> {
        Ok(()) // Default: no-op
    }
}

/// Driver errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverError {
    /// Device not found
    NotFound,
    /// Device busy
    Busy,
    /// Invalid operation for current state
    InvalidState,
    /// Hardware error
    HardwareError,
    /// Timeout waiting for device
    Timeout,
    /// Resource allocation failed
    ResourceError,
    /// Not supported by this driver
    NotSupported,
    /// I/O error
    IoError,
    /// Generic error with code
    Other(u32),
}

/// Driver registry for managing loaded drivers
pub struct DriverRegistry {
    drivers: Vec<(String, Box<dyn Driver>)>,
}

impl DriverRegistry {
    pub const fn new() -> Self {
        DriverRegistry {
            drivers: Vec::new(),
        }
    }

    /// Register a driver
    pub fn register(&mut self, name: String, driver: Box<dyn Driver>) {
        self.drivers.push((name, driver));
    }

    /// Find a driver index by name
    pub fn find_index(&self, name: &str) -> Option<usize> {
        self.drivers
            .iter()
            .position(|(n, _)| n == name)
    }

    /// Get driver by index
    pub fn get(&self, index: usize) -> Option<&dyn Driver> {
        self.drivers.get(index).map(|(_, d)| d.as_ref())
    }

    /// Get driver by index (mutable)
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Box<dyn Driver>> {
        self.drivers.get_mut(index).map(|(_, d)| d)
    }

    /// Get all registered driver names
    pub fn list(&self) -> Vec<&str> {
        self.drivers.iter().map(|(n, _)| n.as_str()).collect()
    }

    /// Initialize all registered drivers
    pub fn init_all(&mut self) -> Result<(), DriverError> {
        for (_, driver) in &mut self.drivers {
            driver.init()?;
        }
        Ok(())
    }
}
