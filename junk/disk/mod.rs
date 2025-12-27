//! Disk I/O subsystem
//!
//! Uses watos-driver-ahci for AHCI, watos-vfs for VFS, watos-fat for FAT

pub mod wfs;
pub mod drives;
pub mod partition;

// Re-export AHCI driver from crate
pub use watos_driver_ahci::{AhciDriver, DiskInfo};

// Re-export VFS types from crate
pub use watos_vfs::{
    VfsError as FsError,
    VfsResult,
    Filesystem,
    FileOperations,
    FileHandle,
    FileMode,
    FileStat,
    FileType,
    DirEntry,
    SeekFrom,
    FsStats,
};

// Re-export FAT from crate
pub use watos_fat::{FatFilesystem, FatType};

pub use wfs::Wfs;
pub use drives::{drive_manager, init as init_drives, FsType, DriveInfo, create_ahci};

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;
use watos_driver_framework::{Driver, block::BlockDevice};

/// Boxed filesystem for dynamic dispatch
pub type BoxedFs = Box<dyn Filesystem>;

/// AhciController type alias for backwards compatibility
pub type AhciController = AhciDriver;

/// Create a VFS instance for a given filesystem type
pub fn create_vfs(fs_type: FsType, port: u8, start_lba: u64) -> Option<BoxedFs> {
    match fs_type {
        FsType::Wfs => {
            let ahci = create_ahci_for_port(port, start_lba)?;
            wfs::Wfs::mount(ahci).map(|w| Box::new(WfsAdapter::new(w)) as BoxedFs)
        }
        FsType::Fat12 | FsType::Fat16 | FsType::Fat32 => {
            let ahci = create_ahci_for_port(port, 0)?;
            // FAT filesystem needs BlockDevice, wrap AHCI
            let adapter = AhciBlockAdapter::new(ahci);
            FatFilesystem::new(adapter)
                .ok()
                .map(|f| Box::new(FatAdapter::new(f)) as BoxedFs)
        }
        _ => None,
    }
}

/// Create an AHCI controller for a specific port with optional LBA offset
fn create_ahci_for_port(port: u8, _lba_offset: u64) -> Option<AhciDriver> {
    use watos_driver_pci::PciDriver;
    use watos_driver_framework::bus::{PciBus, PciBar, pci_class};

    let mut pci = PciDriver::new();
    pci.init().ok()?;

    let devices = pci.find_by_class(pci_class::MASS_STORAGE, pci_class::SATA);
    let dev = devices.into_iter().find(|d| d.id.prog_if == 0x01)?;

    pci.enable_bus_master(dev.address);
    pci.enable_memory_space(dev.address);

    let mmio_base = match dev.bars[5] {
        PciBar::Memory { address, .. } => address,
        _ => return None,
    };

    if mmio_base == 0 {
        return None;
    }

    // Use the crate's probe_port function
    let mut driver = AhciDriver::probe_port(port)?;
    driver.init().ok()?;
    driver.start().ok()?;

    Some(driver)
}

// Adapter to make AhciDriver work as BlockDevice for FAT
struct AhciBlockAdapter {
    driver: AhciDriver,
}

impl AhciBlockAdapter {
    fn new(driver: AhciDriver) -> Self {
        Self { driver }
    }
}

unsafe impl Send for AhciBlockAdapter {}
unsafe impl Sync for AhciBlockAdapter {}

impl Driver for AhciBlockAdapter {
    fn info(&self) -> watos_driver_framework::DriverInfo {
        self.driver.info()
    }

    fn state(&self) -> watos_driver_framework::DriverState {
        self.driver.state()
    }

    fn init(&mut self) -> Result<(), watos_driver_framework::DriverError> {
        self.driver.init()
    }

    fn start(&mut self) -> Result<(), watos_driver_framework::DriverError> {
        self.driver.start()
    }

    fn stop(&mut self) -> Result<(), watos_driver_framework::DriverError> {
        self.driver.stop()
    }
}

impl watos_driver_framework::block::BlockDevice for AhciBlockAdapter {
    fn geometry(&self) -> watos_driver_framework::block::BlockGeometry {
        self.driver.geometry()
    }

    fn read_sectors(&mut self, start: u64, buffer: &mut [u8]) -> Result<usize, watos_driver_framework::DriverError> {
        self.driver.read_sectors(start, buffer)
    }

    fn write_sectors(&mut self, start: u64, buffer: &[u8]) -> Result<usize, watos_driver_framework::DriverError> {
        self.driver.write_sectors(start, buffer)
    }

    fn flush(&mut self) -> Result<(), watos_driver_framework::DriverError> {
        self.driver.flush()
    }
}

// Adapter to make Wfs implement Filesystem trait
// Uses interior mutability since Filesystem trait uses &self
struct WfsAdapter {
    inner: Mutex<wfs::Wfs>,
}

impl WfsAdapter {
    fn new(wfs: wfs::Wfs) -> Self {
        Self { inner: Mutex::new(wfs) }
    }
}

unsafe impl Send for WfsAdapter {}
unsafe impl Sync for WfsAdapter {}

impl Filesystem for WfsAdapter {
    fn name(&self) -> &'static str {
        "wfs"
    }

    fn open(&self, _path: &str, _mode: FileMode) -> VfsResult<Box<dyn FileOperations>> {
        // WFS doesn't support the new file operations interface yet
        Err(FsError::NotSupported)
    }

    fn stat(&self, _path: &str) -> VfsResult<FileStat> {
        Err(FsError::NotSupported)
    }

    fn mkdir(&self, _path: &str) -> VfsResult<()> {
        Err(FsError::NotSupported)
    }

    fn unlink(&self, _path: &str) -> VfsResult<()> {
        Err(FsError::NotSupported)
    }

    fn rmdir(&self, _path: &str) -> VfsResult<()> {
        Err(FsError::NotSupported)
    }

    fn readdir(&self, path: &str) -> VfsResult<Vec<DirEntry>> {
        if path == "/" || path.is_empty() {
            let mut wfs = self.inner.lock();
            let mut entries = Vec::new();
            for (idx, file) in wfs.list_files().enumerate() {
                entries.push(DirEntry {
                    name: String::from_utf8_lossy(&file.name).trim_end_matches('\0').to_string(),
                    file_type: FileType::Regular,
                    size: file.size as u64,
                    inode: idx as u64,
                });
            }
            Ok(entries)
        } else {
            Err(FsError::NotFound)
        }
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> VfsResult<()> {
        Err(FsError::NotSupported)
    }

    fn sync(&self) -> VfsResult<()> {
        Ok(()) // WFS doesn't have a sync operation yet
    }

    fn statfs(&self) -> VfsResult<FsStats> {
        let wfs = self.inner.lock();
        let layout = wfs.get_layout();
        Ok(FsStats {
            total_blocks: layout.total_blocks,
            free_blocks: 0, // Would need to calculate from bitmap
            block_size: wfs::BLOCK_SIZE,
            total_inodes: layout.max_files as u64,
            free_inodes: 0, // Would need to count free entries
            max_name_len: 32,
        })
    }
}

// Adapter to make FatFilesystem implement Filesystem trait
struct FatAdapter<D: BlockDevice> {
    inner: Mutex<FatFilesystem<D>>,
}

impl<D: BlockDevice> FatAdapter<D> {
    fn new(fat: FatFilesystem<D>) -> Self {
        Self { inner: Mutex::new(fat) }
    }
}

unsafe impl<D: BlockDevice> Send for FatAdapter<D> {}
unsafe impl<D: BlockDevice> Sync for FatAdapter<D> {}

impl<D: BlockDevice> Filesystem for FatAdapter<D> {
    fn name(&self) -> &'static str {
        "fat"
    }

    fn open(&self, _path: &str, _mode: FileMode) -> VfsResult<Box<dyn FileOperations>> {
        Err(FsError::NotSupported)
    }

    fn stat(&self, _path: &str) -> VfsResult<FileStat> {
        Err(FsError::NotSupported)
    }

    fn mkdir(&self, _path: &str) -> VfsResult<()> {
        Err(FsError::NotSupported)
    }

    fn unlink(&self, _path: &str) -> VfsResult<()> {
        Err(FsError::NotSupported)
    }

    fn rmdir(&self, _path: &str) -> VfsResult<()> {
        Err(FsError::NotSupported)
    }

    fn readdir(&self, _path: &str) -> VfsResult<Vec<DirEntry>> {
        Err(FsError::NotSupported)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> VfsResult<()> {
        Err(FsError::NotSupported)
    }

    fn sync(&self) -> VfsResult<()> {
        Ok(())
    }

    fn statfs(&self) -> VfsResult<FsStats> {
        Err(FsError::NotSupported)
    }
}
