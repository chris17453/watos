//! Network subsystem for DOS64
//!
//! Provides e1000 driver and TCP/IP stack integration

pub mod pci;
pub mod e1000;
pub mod stack;

pub use e1000::E1000;
pub use stack::{NetworkStack, PingResult, parse_ipv4};
