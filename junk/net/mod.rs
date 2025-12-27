//! Network subsystem for WATOS
//!
//! Provides TCP/IP stack integration using kernel driver module.

pub use watos_network::{NetworkStack, NetworkDriver, PingResult, parse_ipv4, NetConfig};

// Re-export driver types for convenience
pub use crate::drivers::{LinkStatus, get_link_status, get_mac_address, has_network};

/// Kernel network driver adapter
pub struct KernelNetDriver;

impl NetworkDriver for KernelNetDriver {
    fn get_mac_address(&self) -> Option<[u8; 6]> {
        crate::drivers::get_mac_address()
    }

    fn send_packet(&self, data: &[u8]) -> Result<(), ()> {
        crate::drivers::send_packet(data).map_err(|_| ())
    }

    fn receive_packet(&self, buffer: &mut [u8]) -> Option<usize> {
        crate::drivers::receive_packet(buffer).ok()
    }

    fn get_ticks(&self) -> u64 {
        crate::interrupts::get_ticks()
    }

    fn halt(&self) {
        crate::interrupts::halt();
    }

    fn enable_timer(&self) {
        crate::interrupts::enable_timer();
    }

    fn disable_timer(&self) {
        crate::interrupts::disable_timer();
    }
}

/// Create a network stack with the kernel driver
pub fn create_network_stack() -> Option<NetworkStack<KernelNetDriver>> {
    NetworkStack::new(KernelNetDriver)
}
