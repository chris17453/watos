//! Network device driver abstraction
//!
//! For network interfaces: e1000, virtio-net, etc.

use crate::{Driver, DriverError};

/// MAC address (6 bytes)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MacAddress(pub [u8; 6]);

impl MacAddress {
    pub const BROADCAST: MacAddress = MacAddress([0xFF; 6]);
    pub const ZERO: MacAddress = MacAddress([0; 6]);

    pub fn as_bytes(&self) -> &[u8; 6] {
        &self.0
    }
}

/// Network link status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkStatus {
    Down,
    Up,
}

/// Network link speed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkSpeed {
    Unknown,
    Mbps10,
    Mbps100,
    Gbps1,
    Gbps10,
}

/// Network device capabilities
#[derive(Debug, Clone, Copy)]
pub struct NetCapabilities {
    /// Maximum transmission unit
    pub mtu: u16,
    /// Supports checksum offload
    pub checksum_offload: bool,
    /// Supports scatter-gather
    pub scatter_gather: bool,
    /// Supports promiscuous mode
    pub promiscuous: bool,
}

/// Network device trait
pub trait NetworkDevice: Driver {
    /// Get the MAC address
    fn mac_address(&self) -> MacAddress;

    /// Get link status
    fn link_status(&self) -> LinkStatus;

    /// Get link speed
    fn link_speed(&self) -> LinkSpeed;

    /// Get device capabilities
    fn capabilities(&self) -> NetCapabilities;

    /// Send a packet
    ///
    /// # Arguments
    /// * `packet` - Raw ethernet frame to send
    fn send(&mut self, packet: &[u8]) -> Result<(), DriverError>;

    /// Receive a packet
    ///
    /// # Arguments
    /// * `buffer` - Buffer to receive into
    ///
    /// # Returns
    /// Number of bytes received, or 0 if no packet available
    fn receive(&mut self, buffer: &mut [u8]) -> Result<usize, DriverError>;

    /// Check if a packet is available to receive
    fn has_packet(&self) -> bool;

    /// Set promiscuous mode
    fn set_promiscuous(&mut self, enabled: bool) -> Result<(), DriverError>;
}

/// Network device identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetDeviceId(pub u32);
