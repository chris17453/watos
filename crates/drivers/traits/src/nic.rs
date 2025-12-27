//! Network Interface Card Trait
//!
//! Implemented by network drivers (e1000, RTL8139, etc.)
//! Used by the network stack (TCP/IP)

use crate::DriverResult;

/// MAC address type
pub type MacAddress = [u8; 6];

/// Network interface card trait
pub trait NicDevice: Send + Sync {
    /// Get the MAC address of this interface
    fn mac_address(&self) -> MacAddress;

    /// Send a raw Ethernet frame
    ///
    /// # Arguments
    /// * `frame` - Complete Ethernet frame including header
    fn send_frame(&self, frame: &[u8]) -> DriverResult<()>;

    /// Receive a raw Ethernet frame (non-blocking)
    ///
    /// # Arguments
    /// * `buf` - Buffer to receive into
    ///
    /// # Returns
    /// * `Ok(Some(len))` - Frame received, len bytes written to buf
    /// * `Ok(None)` - No frame available
    /// * `Err(...)` - Error occurred
    fn receive_frame(&self, buf: &mut [u8]) -> DriverResult<Option<usize>>;

    /// Check if the link is up
    fn link_up(&self) -> bool;

    /// Get link speed in Mbps (10, 100, 1000, etc.)
    fn link_speed(&self) -> u32;

    /// Get device information
    fn info(&self) -> NicDeviceInfo;

    /// Enable/disable promiscuous mode
    fn set_promiscuous(&self, _enabled: bool) -> DriverResult<()> {
        Ok(()) // Default: ignore
    }
}

/// Information about a network interface
#[derive(Debug, Clone)]
pub struct NicDeviceInfo {
    /// Device name/model
    pub name: &'static str,
    /// MAC address
    pub mac: MacAddress,
    /// Maximum transmission unit
    pub mtu: usize,
    /// Link is up
    pub link_up: bool,
    /// Link speed in Mbps
    pub speed_mbps: u32,
}

/// Format MAC address as string
pub fn format_mac(mac: &MacAddress) -> [u8; 17] {
    let hex = b"0123456789ABCDEF";
    let mut buf = [0u8; 17];
    for i in 0..6 {
        buf[i * 3] = hex[(mac[i] >> 4) as usize];
        buf[i * 3 + 1] = hex[(mac[i] & 0xF) as usize];
        if i < 5 {
            buf[i * 3 + 2] = b':';
        }
    }
    buf
}
