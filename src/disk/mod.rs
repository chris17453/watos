//! Disk I/O subsystem
//!
//! AHCI driver, WFS and FAT filesystem support, drive management

pub mod ahci;
pub mod wfs;
pub mod fat;
pub mod drives;
pub mod partition;
pub mod vfs;

pub use ahci::{AhciController, DiskInfo};
pub use wfs::{Wfs, FileEntry, CheckResult, MountResult, FLAG_EXEC, FLAG_READONLY, FLAG_SYSTEM, FLAG_DIR, DEBUG_SB_SIZE, DEBUG_CALC_CRC, DEBUG_STORED_CRC};
pub use wfs::FsInfo as WfsFsInfo;  // Avoid conflict with vfs::FsInfo
pub use fat::{FatFs, FatDirEntry, FatType, FatInfo};
pub use drives::{DriveManager, DriveInfo, FsType, drive_manager, init as init_drives};
pub use partition::{PartitionTable, PartitionTableType, Partition, PartitionType};
pub use vfs::{FileSystem, FileType, FileAttr, DirEntry, FsInfo, FsError, FsResult, BoxedFs, create_vfs};
