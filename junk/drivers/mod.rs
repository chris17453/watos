//! Kernel Driver Manager
//!
//! Provides a unified interface for loading and managing device drivers.
//! Uses the watos-driver-framework crate for driver abstractions.

use alloc::boxed::Box;
use spin::Mutex;

// Re-export driver framework types
pub use watos_driver_framework::{
    Driver, DriverError, DriverInfo, DriverState,
    net::{NetworkDevice, MacAddress, LinkStatus, LinkSpeed, NetCapabilities},
    block::{BlockDevice, BlockGeometry},
};

// Re-export concrete drivers
pub use watos_driver_e1000::E1000Driver;

/// Global network device (singleton for now)
static NETWORK_DEVICE: Mutex<Option<Box<dyn NetworkDeviceWrapper>>> = Mutex::new(None);

/// Wrapper trait to allow storing NetworkDevice as trait object
/// This is needed because NetworkDevice has Driver as a supertrait
pub trait NetworkDeviceWrapper: Send + Sync {
    fn mac_address(&self) -> MacAddress;
    fn link_status(&self) -> LinkStatus;
    fn link_speed(&self) -> LinkSpeed;
    fn capabilities(&self) -> NetCapabilities;
    fn send(&mut self, packet: &[u8]) -> Result<(), DriverError>;
    fn receive(&mut self, buffer: &mut [u8]) -> Result<usize, DriverError>;
    fn has_packet(&self) -> bool;
    fn set_promiscuous(&mut self, enabled: bool) -> Result<(), DriverError>;
    fn state(&self) -> DriverState;
}

impl<T: NetworkDevice + Send + Sync> NetworkDeviceWrapper for T {
    fn mac_address(&self) -> MacAddress {
        <T as NetworkDevice>::mac_address(self)
    }
    fn link_status(&self) -> LinkStatus {
        <T as NetworkDevice>::link_status(self)
    }
    fn link_speed(&self) -> LinkSpeed {
        <T as NetworkDevice>::link_speed(self)
    }
    fn capabilities(&self) -> NetCapabilities {
        <T as NetworkDevice>::capabilities(self)
    }
    fn send(&mut self, packet: &[u8]) -> Result<(), DriverError> {
        <T as NetworkDevice>::send(self, packet)
    }
    fn receive(&mut self, buffer: &mut [u8]) -> Result<usize, DriverError> {
        <T as NetworkDevice>::receive(self, buffer)
    }
    fn has_packet(&self) -> bool {
        <T as NetworkDevice>::has_packet(self)
    }
    fn set_promiscuous(&mut self, enabled: bool) -> Result<(), DriverError> {
        <T as NetworkDevice>::set_promiscuous(self, enabled)
    }
    fn state(&self) -> DriverState {
        <T as Driver>::state(self)
    }
}

/// Probe and initialize network driver
pub fn init_network() -> Result<(), &'static str> {
    // Try to probe for e1000
    if let Some(mut driver) = E1000Driver::probe() {
        // Initialize the driver
        driver.init().map_err(|_| "Failed to initialize e1000")?;
        driver.start().map_err(|_| "Failed to start e1000")?;

        // Store globally
        let mut guard = NETWORK_DEVICE.lock();
        *guard = Some(Box::new(driver));

        Ok(())
    } else {
        Err("No network device found")
    }
}

/// Get access to the network device
pub fn with_network<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut dyn NetworkDeviceWrapper) -> R,
{
    let mut guard = NETWORK_DEVICE.lock();
    guard.as_mut().map(|dev| f(dev.as_mut()))
}

/// Check if network device is available
pub fn has_network() -> bool {
    NETWORK_DEVICE.lock().is_some()
}

/// Get network device MAC address
pub fn get_mac_address() -> Option<[u8; 6]> {
    with_network(|dev| dev.mac_address().0)
}

/// Get network link status
pub fn get_link_status() -> Option<LinkStatus> {
    with_network(|dev| dev.link_status())
}

/// Send a network packet
pub fn send_packet(packet: &[u8]) -> Result<(), DriverError> {
    with_network(|dev| dev.send(packet))
        .ok_or(DriverError::NotFound)?
}

/// Receive a network packet
pub fn receive_packet(buffer: &mut [u8]) -> Result<usize, DriverError> {
    with_network(|dev| dev.receive(buffer))
        .ok_or(DriverError::NotFound)?
}

/// Check if packet is available
pub fn has_packet() -> bool {
    with_network(|dev| dev.has_packet()).unwrap_or(false)
}
