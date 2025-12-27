//! Disk I/O subsystem
//!
//! AHCI driver, WFS and FAT filesystem support, drive management

pub mod ahci;
pub mod wfs;
pub mod fat;
pub mod drives;
pub mod partition;
pub mod vfs;

pub use ahci::AhciController;
pub use wfs::Wfs;
pub use drives::{drive_manager, init as init_drives};
pub use vfs::FsError;
