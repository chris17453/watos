//! WATOS Kernel
//!
//! Minimal kernel that provides:
//! - Memory management
//! - Interrupt handling
//! - Syscall interface for user-space apps
//!
//! The kernel does NOT include terminal emulation - that's a user-space app.

#![no_std]
#![no_main]

extern crate alloc;

// Kernel debug macro - only outputs when 'debug-kernel' feature is enabled
#[cfg(feature = "debug-kernel")]
macro_rules! kernel_debug {
    ($($arg:tt)*) => {{
        unsafe {
            $($arg)*
        }
    }};
}

#[cfg(not(feature = "debug-kernel"))]
macro_rules! kernel_debug {
    ($($arg:tt)*) => {};
}

// VFS debug macro - only outputs when 'debug-vfs' feature is enabled
#[cfg(feature = "debug-vfs")]
macro_rules! vfs_debug {
    ($($arg:tt)*) => {{
        unsafe {
            $($arg)*
        }
    }};
}

#[cfg(not(feature = "debug-vfs"))]
macro_rules! vfs_debug {
    ($($arg:tt)*) => {};
}

use core::panic::PanicInfo;
use linked_list_allocator::LockedHeap;
use spin::Mutex;

// Disk and filesystem support
use watos_driver_traits::{Driver, DriverState};
use watos_driver_traits::block::{BlockDevice, BlockDeviceExt};
use watos_driver_ahci::AhciDriver;
use wfs_common::{Superblock, WFS_MAGIC, BLOCK_SIZE};

// VFS and FAT filesystem
use alloc::boxed::Box;
use watos_vfs::{FileMode, FileOperations, VfsError};
use watos_fat::FatFilesystem;
use watos_procfs::{ProcFs, SystemProvider};

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

// Use layout constants - heap starts at 0x300000 (after kernel stacks)
const HEAP_START: usize = watos_mem::layout::PHYS_KERNEL_HEAP as usize;
const HEAP_SIZE: usize = watos_mem::layout::PHYS_KERNEL_HEAP_SIZE as usize;

/// Maximum number of preloaded apps
const MAX_PRELOADED_APPS: usize = 32;

/// Entry for a preloaded application
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PreloadedApp {
    pub name: [u8; 32],    // Null-terminated name (e.g., "date", "echo")
    pub addr: u64,         // Load address
    pub size: u64,         // Size in bytes
}

/// Boot info passed from bootloader at 0x80000
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BootInfo {
    pub magic: u32,
    pub framebuffer_addr: u64,
    pub framebuffer_width: u32,
    pub framebuffer_height: u32,
    pub framebuffer_pitch: u32,
    pub framebuffer_bpp: u32,
    pub pixel_format: u32, // 0=RGB, 1=BGR
    pub init_app_addr: u64,   // Address of loaded init app (TERM.EXE)
    pub init_app_size: u64,   // Size of init app in bytes
    pub app_count: u32,       // Number of preloaded apps
    pub _pad: u32,            // Padding for alignment
    pub apps: [PreloadedApp; MAX_PRELOADED_APPS], // Preloaded app table
}

const BOOT_INFO_ADDR: usize = 0x80000;
const BOOT_MAGIC: u32 = 0x5741544F; // "WATO"

/// Global boot info (copied from bootloader)
static mut BOOT_INFO: Option<BootInfo> = None;

// ============================================================================
// Drive Manager - Maps drive names (like "C", "D", "MYDATA") to mount points
// ============================================================================

/// Maximum drive name length
const MAX_DRIVE_NAME: usize = 32;

/// Maximum number of mounted drives
const MAX_DRIVES: usize = 26;

/// A mounted drive entry
#[derive(Clone)]
struct DriveEntry {
    /// Drive name (e.g., "C", "D", "MYDATA")
    name: [u8; MAX_DRIVE_NAME],
    name_len: usize,
    /// Mount path in VFS (e.g., "/mnt/c")
    mount_path: [u8; 64],
    mount_path_len: usize,
    /// Filesystem type string
    fs_type: [u8; 16],
    fs_type_len: usize,
    /// Is this entry in use?
    in_use: bool,
}

impl DriveEntry {
    const fn empty() -> Self {
        DriveEntry {
            name: [0; MAX_DRIVE_NAME],
            name_len: 0,
            mount_path: [0; 64],
            mount_path_len: 0,
            fs_type: [0; 16],
            fs_type_len: 0,
            in_use: false,
        }
    }
}

/// Global drive table
static mut DRIVE_TABLE: [DriveEntry; MAX_DRIVES] = [const { DriveEntry::empty() }; MAX_DRIVES];
static mut CURRENT_DRIVE: usize = 0; // Index into DRIVE_TABLE

/// Current working directory (relative to current drive)
const MAX_PATH_LEN: usize = 256;
static mut CURRENT_DIR: [u8; MAX_PATH_LEN] = [0; MAX_PATH_LEN];
static mut CURRENT_DIR_LEN: usize = 1; // Starts as "\"

/// Initialize current directory to root
fn init_cwd() {
    unsafe {
        CURRENT_DIR[0] = b'\\';
        CURRENT_DIR_LEN = 1;
    }
}

/// Mount a drive with a given name
/// Returns 0 on success, error code on failure
fn drive_mount(name: &[u8], mount_path: &[u8], fs_type: &[u8]) -> u64 {
    if name.is_empty() || name.len() > MAX_DRIVE_NAME {
        return 1; // Invalid name
    }
    if mount_path.is_empty() || mount_path.len() > 64 {
        return 2; // Invalid path
    }

    unsafe {
        // Check if already mounted
        for entry in &DRIVE_TABLE {
            if entry.in_use && entry.name_len == name.len() {
                let matches = entry.name[..entry.name_len].iter()
                    .zip(name.iter())
                    .all(|(a, b)| {
                        let a_upper = if *a >= b'a' && *a <= b'z' { *a - 32 } else { *a };
                        let b_upper = if *b >= b'a' && *b <= b'z' { *b - 32 } else { *b };
                        a_upper == b_upper
                    });
                if matches {
                    return 3; // Already mounted
                }
            }
        }

        // Find free slot
        for entry in &mut DRIVE_TABLE {
            if !entry.in_use {
                entry.name[..name.len()].copy_from_slice(name);
                entry.name_len = name.len();
                entry.mount_path[..mount_path.len()].copy_from_slice(mount_path);
                entry.mount_path_len = mount_path.len();
                if !fs_type.is_empty() && fs_type.len() <= 16 {
                    entry.fs_type[..fs_type.len()].copy_from_slice(fs_type);
                    entry.fs_type_len = fs_type.len();
                }
                entry.in_use = true;
                return 0; // Success
            }
        }

        4 // No free slots
    }
}

/// Unmount a drive by name
/// Returns 0 on success, error code on failure
fn drive_unmount(name: &[u8]) -> u64 {
    if name.is_empty() {
        return 1;
    }

    unsafe {
        for (i, entry) in DRIVE_TABLE.iter_mut().enumerate() {
            if entry.in_use && entry.name_len == name.len() {
                let matches = entry.name[..entry.name_len].iter()
                    .zip(name.iter())
                    .all(|(a, b)| {
                        let a_upper = if *a >= b'a' && *a <= b'z' { *a - 32 } else { *a };
                        let b_upper = if *b >= b'a' && *b <= b'z' { *b - 32 } else { *b };
                        a_upper == b_upper
                    });
                if matches {
                    // Can't unmount current drive
                    if i == CURRENT_DRIVE {
                        return 2; // Drive in use
                    }
                    *entry = DriveEntry::empty();
                    return 0;
                }
            }
        }
        3 // Not found
    }
}

/// List drives - copies drive info to buffer
/// Format: "NAME:PATH:FSTYPE\n" for each drive
/// Returns bytes written
fn drive_list(buf: &mut [u8]) -> usize {
    let mut pos = 0;

    unsafe {
        for (i, entry) in DRIVE_TABLE.iter().enumerate() {
            if entry.in_use {
                // Copy name
                let name = &entry.name[..entry.name_len];
                if pos + name.len() >= buf.len() { break; }
                buf[pos..pos + name.len()].copy_from_slice(name);
                pos += name.len();

                // Colon
                if pos >= buf.len() { break; }
                buf[pos] = b':';
                pos += 1;

                // Copy path
                let path = &entry.mount_path[..entry.mount_path_len];
                if pos + path.len() >= buf.len() { break; }
                buf[pos..pos + path.len()].copy_from_slice(path);
                pos += path.len();

                // Colon
                if pos >= buf.len() { break; }
                buf[pos] = b':';
                pos += 1;

                // Copy fs type
                let fs = &entry.fs_type[..entry.fs_type_len];
                if pos + fs.len() >= buf.len() { break; }
                buf[pos..pos + fs.len()].copy_from_slice(fs);
                pos += fs.len();

                // Mark current drive with *
                if i == CURRENT_DRIVE {
                    if pos >= buf.len() { break; }
                    buf[pos] = b'*';
                    pos += 1;
                }

                // Newline
                if pos >= buf.len() { break; }
                buf[pos] = b'\n';
                pos += 1;
            }
        }
    }

    pos
}

/// Change current drive by name
/// Returns 0 on success, error code on failure
fn drive_change(name: &[u8]) -> u64 {
    if name.is_empty() {
        return 1;
    }

    unsafe {
        for (i, entry) in DRIVE_TABLE.iter().enumerate() {
            if entry.in_use && entry.name_len == name.len() {
                let matches = entry.name[..entry.name_len].iter()
                    .zip(name.iter())
                    .all(|(a, b)| {
                        let a_upper = if *a >= b'a' && *a <= b'z' { *a - 32 } else { *a };
                        let b_upper = if *b >= b'a' && *b <= b'z' { *b - 32 } else { *b };
                        a_upper == b_upper
                    });
                if matches {
                    CURRENT_DRIVE = i;
                    return 0;
                }
            }
        }
        2 // Not found
    }
}

/// Get current drive name into buffer
/// Returns bytes written
fn drive_get_current(buf: &mut [u8]) -> usize {
    unsafe {
        let entry = &DRIVE_TABLE[CURRENT_DRIVE];
        if entry.in_use && buf.len() >= entry.name_len {
            buf[..entry.name_len].copy_from_slice(&entry.name[..entry.name_len]);
            entry.name_len
        } else {
            0
        }
    }
}

/// Get full current working directory (DRIVE:\path)
/// Returns bytes written
fn get_cwd(buf: &mut [u8]) -> usize {
    unsafe {
        let entry = &DRIVE_TABLE[CURRENT_DRIVE];
        if !entry.in_use {
            return 0;
        }

        let mut pos = 0;

        // Drive name
        if pos + entry.name_len >= buf.len() { return 0; }
        buf[pos..pos + entry.name_len].copy_from_slice(&entry.name[..entry.name_len]);
        pos += entry.name_len;

        // Colon
        if pos >= buf.len() { return 0; }
        buf[pos] = b':';
        pos += 1;

        // Current directory
        if pos + CURRENT_DIR_LEN > buf.len() { return pos; }
        buf[pos..pos + CURRENT_DIR_LEN].copy_from_slice(&CURRENT_DIR[..CURRENT_DIR_LEN]);
        pos += CURRENT_DIR_LEN;

        pos
    }
}

/// Helper to validate a path exists and is a directory via VFS
fn validate_directory(full_path: &str) -> bool {
    unsafe {
        // Switch to kernel CR3 for VFS access
        let user_cr3 = watos_mem::paging::get_cr3();
        let kernel_pml4 = watos_process::get_kernel_pml4();
        if kernel_pml4 != 0 && user_cr3 != kernel_pml4 {
            watos_mem::paging::load_cr3(kernel_pml4);
        }

        let result = watos_vfs::stat(full_path);

        // Switch back to user CR3
        if kernel_pml4 != 0 && user_cr3 != kernel_pml4 {
            watos_mem::paging::load_cr3(user_cr3);
        }

        match result {
            Ok(stat) => stat.file_type == watos_vfs::FileType::Directory,
            Err(_) => false,
        }
    }
}

/// Change current directory
/// path can be:
///   - "DRIVE:" to switch drive (resets to root of that drive)
///   - "\" or "/" to go to root
///   - ".." to go up one level
///   - "dirname" to enter subdirectory
///   - "\path" or "/path" for absolute path
/// Returns 0 on success, error code on failure
fn change_dir(path: &[u8]) -> u64 {
    if path.is_empty() {
        return 1;
    }

    unsafe {
        // Check if it's a drive change (ends with :)
        if path.len() > 1 && path[path.len() - 1] == b':' {
            let drive_name = &path[..path.len() - 1];
            let result = drive_change(drive_name);
            if result == 0 {
                // Reset to root when changing drives
                CURRENT_DIR[0] = b'\\';
                CURRENT_DIR_LEN = 1;
            }
            return result;
        }

        // Check for root
        if path.len() == 1 && (path[0] == b'\\' || path[0] == b'/') {
            CURRENT_DIR[0] = b'\\';
            CURRENT_DIR_LEN = 1;
            return 0;
        }

        // Check for ".."
        if path.len() == 2 && path[0] == b'.' && path[1] == b'.' {
            // Go up one level
            if CURRENT_DIR_LEN <= 1 {
                // Already at root
                return 0;
            }
            // Find last backslash
            let mut last_slash = 0;
            for i in 0..CURRENT_DIR_LEN {
                if CURRENT_DIR[i] == b'\\' || CURRENT_DIR[i] == b'/' {
                    last_slash = i;
                }
            }
            if last_slash == 0 {
                // Go to root
                CURRENT_DIR[0] = b'\\';
                CURRENT_DIR_LEN = 1;
            } else {
                CURRENT_DIR_LEN = last_slash;
            }
            return 0;
        }

        // Build the full path to validate
        static mut NEW_PATH_BUF: [u8; 260] = [0u8; 260];
        let mut new_path_len: usize;

        // Absolute path (starts with \ or /)
        if path[0] == b'\\' || path[0] == b'/' {
            if path.len() >= MAX_PATH_LEN {
                return 2; // Path too long
            }
            // Copy and normalize slashes
            for i in 0..path.len() {
                NEW_PATH_BUF[i] = if path[i] == b'/' { b'\\' } else { path[i] };
            }
            new_path_len = path.len();
        } else {
            // Relative path - append to current directory
            let needed = if CURRENT_DIR_LEN > 1 {
                CURRENT_DIR_LEN + 1 + path.len() // existing + \ + new
            } else {
                1 + path.len() // \ + new
            };

            if needed >= MAX_PATH_LEN {
                return 2; // Path too long
            }

            // Copy current directory
            NEW_PATH_BUF[..CURRENT_DIR_LEN].copy_from_slice(&CURRENT_DIR[..CURRENT_DIR_LEN]);
            new_path_len = CURRENT_DIR_LEN;

            // Add separator if not at root
            if new_path_len > 1 {
                NEW_PATH_BUF[new_path_len] = b'\\';
                new_path_len += 1;
            }

            // Append new path component
            for i in 0..path.len() {
                NEW_PATH_BUF[new_path_len + i] = if path[i] == b'/' { b'\\' } else { path[i] };
            }
            new_path_len += path.len();
        }

        // Build full VFS path with drive letter: "X:\path"
        static mut FULL_PATH_BUF: [u8; 264] = [0u8; 264];
        let drive = &DRIVE_TABLE[CURRENT_DRIVE];
        let name_len = drive.name_len.min(MAX_DRIVE_NAME);
        let mut pos = 0;
        FULL_PATH_BUF[pos..pos + name_len].copy_from_slice(&drive.name[..name_len]);
        pos += name_len;
        FULL_PATH_BUF[pos] = b':';
        pos += 1;
        FULL_PATH_BUF[pos..pos + new_path_len].copy_from_slice(&NEW_PATH_BUF[..new_path_len]);
        pos += new_path_len;

        // Convert to str for VFS
        let full_path_str = match core::str::from_utf8(&FULL_PATH_BUF[..pos]) {
            Ok(s) => s,
            Err(_) => return 1, // Invalid path
        };

        watos_arch::serial_write(b"[KERNEL] cd validate: ");
        watos_arch::serial_write(&FULL_PATH_BUF[..pos]);
        watos_arch::serial_write(b"\r\n");

        // Validate directory exists (root always exists)
        if new_path_len > 1 && !validate_directory(full_path_str) {
            watos_arch::serial_write(b"[KERNEL] cd: directory not found\r\n");
            return 3; // No such directory
        }

        // Update current directory
        CURRENT_DIR[..new_path_len].copy_from_slice(&NEW_PATH_BUF[..new_path_len]);
        CURRENT_DIR_LEN = new_path_len;

        0
    }
}

// ============================================================================
// Disk and Filesystem Subsystem
// ============================================================================

/// Global AHCI driver (wrapped in Mutex for thread-safety)
static DISK_DRIVER: Mutex<Option<AhciDriver>> = Mutex::new(None);

/// WFS Superblock (cached after initialization)
static mut WFS_SUPERBLOCK: Option<Superblock> = None;

/// Initialize disk and filesystem
/// Probes AHCI ports looking for WFS data disk and mounts it in VFS as D:
fn init_disk() -> bool {
    unsafe { watos_arch::serial_write(b"[KERNEL] Initializing disk subsystem...\r\n"); }

    // Try probing each AHCI port looking for WFS
    // Port 0 is handled by init_vfs() for FAT boot disk, so start from port 1
    for port in 1..4 {
        unsafe {
            watos_arch::serial_write(b"[KERNEL] Probing AHCI port ");
            watos_arch::serial_hex(port as u64);
            watos_arch::serial_write(b"...\r\n");
        }

        let driver = match AhciDriver::probe_port(port) {
            Some(d) => d,
            None => continue,
        };

        unsafe {
            watos_arch::serial_write(b"[KERNEL] Found disk on port ");
            watos_arch::serial_hex(port as u64);
            watos_arch::serial_write(b"\r\n");
        }

        // Initialize and start the driver
        let mut driver = driver;
        if driver.init().is_err() {
            unsafe { watos_arch::serial_write(b"[KERNEL] AHCI init failed\r\n"); }
            continue;
        }
        if driver.start().is_err() {
            unsafe { watos_arch::serial_write(b"[KERNEL] AHCI start failed\r\n"); }
            continue;
        }

        // Try to create WFS filesystem and mount in VFS
        match wfs_common::WfsFilesystem::new(driver) {
            Ok(wfs_fs) => {
                unsafe {
                    watos_arch::serial_write(b"[KERNEL] WFS filesystem detected on port ");
                    watos_arch::serial_hex(port as u64);
                    watos_arch::serial_write(b"\r\n");
                }

                // Mount as drive D: in VFS
                match watos_vfs::mount_drive('D', alloc::boxed::Box::new(wfs_fs)) {
                    Ok(()) => {
                        unsafe { watos_arch::serial_write(b"[KERNEL] Mounted WFS as D:\r\n"); }
                        return true;
                    }
                    Err(_) => {
                        unsafe { watos_arch::serial_write(b"[KERNEL] Failed to mount WFS in VFS\r\n"); }
                    }
                }
            }
            Err(_) => {
                unsafe {
                    watos_arch::serial_write(b"[KERNEL] Port ");
                    watos_arch::serial_hex(port as u64);
                    watos_arch::serial_write(b" is not WFS\r\n");
                }
            }
        }
    }

    unsafe { watos_arch::serial_write(b"[KERNEL] No WFS disk found\r\n"); }
    false
}

/// Initialize VFS and mount boot disk (FAT) as drive C:
/// System provider for procfs that returns real kernel stats
struct WatosSystemProvider;

impl SystemProvider for WatosSystemProvider {
    fn cpu_info(&self) -> alloc::string::String {
        use alloc::format;
        format!(
            "processor\t: 0\n\
             vendor_id\t: WATOS\n\
             model name\t: x86_64 Virtual CPU\n\
             cpu family\t: 6\n\
             model\t\t: 0\n\
             stepping\t: 0\n"
        )
    }

    fn mem_info(&self) -> alloc::string::String {
        use alloc::format;
        let phys_stats = watos_mem::pmm::stats();
        let heap_stats = watos_mem::heap::stats();

        let total_kb = phys_stats.total_bytes / 1024;
        let free_kb = phys_stats.free_bytes / 1024;
        let used_kb = (phys_stats.total_bytes - phys_stats.free_bytes) / 1024;
        let heap_total_kb = heap_stats.total / 1024;
        let heap_used_kb = heap_stats.used / 1024;
        let heap_free_kb = (heap_stats.total - heap_stats.used) / 1024;

        format!(
            "MemTotal:       {} kB\n\
             MemFree:        {} kB\n\
             MemUsed:        {} kB\n\
             HeapTotal:      {} kB\n\
             HeapUsed:       {} kB\n\
             HeapFree:       {} kB\n",
            total_kb, free_kb, used_kb,
            heap_total_kb, heap_used_kb, heap_free_kb
        )
    }

    fn uptime_secs(&self) -> u64 {
        // Get ticks and convert to seconds (assuming 1000 ticks/sec)
        watos_arch::idt::get_ticks() / 1000
    }

    fn mounts_info(&self) -> alloc::string::String {
        use alloc::format;
        use alloc::string::String;

        // Get drive list from kernel
        let mut buf = [0u8; 1024];
        let len = drive_list(&mut buf);
        let drive_data = &buf[..len];

        let mut result = String::new();

        // Parse drives (format: "NAME:PATH:FSTYPE\n" per line)
        let mut line_start = 0;
        for i in 0..drive_data.len() {
            if drive_data[i] == b'\n' {
                let line = &drive_data[line_start..i];

                // Parse "NAME:PATH:FSTYPE"
                if let Some(colon1) = line.iter().position(|&c| c == b':') {
                    if let Some(colon2) = line[colon1+1..].iter().position(|&c| c == b':') {
                        let colon2 = colon1 + 1 + colon2;

                        let name = core::str::from_utf8(&line[..colon1]).unwrap_or("?");
                        let path = core::str::from_utf8(&line[colon1+1..colon2]).unwrap_or("?");
                        let fstype = core::str::from_utf8(&line[colon2+1..]).unwrap_or("?");

                        result.push_str(&format!("{} {} {} rw 0 0\n", name, path, fstype));
                    }
                }

                line_start = i + 1;
            }
        }

        result
    }
}

fn init_vfs() -> bool {
    unsafe { watos_arch::serial_write(b"[KERNEL] Initializing VFS...\r\n"); }

    // Initialize the global VFS instance
    watos_vfs::init();
    unsafe { watos_arch::serial_write(b"[KERNEL] VFS initialized\r\n"); }

    // Mount procfs at /proc
    let procfs = ProcFs::new();
    procfs.set_system_provider(Box::new(WatosSystemProvider));

    match watos_vfs::mount("/proc", Box::new(procfs)) {
        Ok(()) => {
            unsafe { watos_arch::serial_write(b"[KERNEL] Mounted procfs at /proc\r\n"); }
        }
        Err(_) => {
            unsafe { watos_arch::serial_write(b"[KERNEL] Failed to mount procfs\r\n"); }
        }
    }

    // Try all AHCI ports to find a valid FAT filesystem for C:
    // Port 0 = UEFI boot disk (uefi_boot.img - has apps)
    // Scan ports in normal order
    for port in [0u8, 1, 2, 3].iter().copied() {
        unsafe {
            watos_arch::serial_write(b"[KERNEL] Trying AHCI port ");
            watos_arch::serial_hex(port as u64);
            watos_arch::serial_write(b" for FAT...\r\n");
        }

        let driver = match AhciDriver::probe_port(port) {
            Some(d) => d,
            None => continue,
        };

        let mut driver = driver;
        if driver.init().is_err() {
            continue;
        }
        if driver.start().is_err() {
            continue;
        }

        // Try to create FAT filesystem
        match FatFilesystem::new(driver) {
            Ok(fat_fs) => {
                unsafe {
                    watos_arch::serial_write(b"[KERNEL] FAT filesystem found on port ");
                    watos_arch::serial_hex(port as u64);
                    watos_arch::serial_write(b"\r\n");
                }

                // Mount as drive C:
                match watos_vfs::mount_drive('C', Box::new(fat_fs)) {
                    Ok(()) => {
                        unsafe { watos_arch::serial_write(b"[KERNEL] Mounted FAT as C:\r\n"); }

                        // Also add to legacy drive table so CURRENT_DRIVE works
                        drive_mount(b"C", b"/", b"FAT");

                        return true;
                    }
                    Err(_) => {
                        unsafe { watos_arch::serial_write(b"[KERNEL] Failed to mount C:\r\n"); }
                    }
                }
            }
            Err(e) => {
                // Log the specific error
                unsafe {
                    watos_arch::serial_write(b"[KERNEL] FAT probe failed on port ");
                    watos_arch::serial_hex(port as u64);
                    let err_msg: &[u8] = match e {
                        VfsError::IoError => b" IoError\r\n",
                        VfsError::InvalidArgument => b" InvalidArgument\r\n",
                        VfsError::NotFound => b" NotFound\r\n",
                        _ => b" other\r\n",
                    };
                    watos_arch::serial_write(err_msg);
                }
                continue;
            }
        }
    }

    unsafe { watos_arch::serial_write(b"[KERNEL] No valid FAT filesystem found for C:\r\n"); }
    false
}

/// Read WFS directory entries into buffer
/// Returns bytes written in format: "TYPE NAME SIZE\n" per entry
fn wfs_readdir(_path: &[u8], buf: &mut [u8]) -> usize {
    // TEMPORARY: Skip WFS disk reads to debug crash
    // Just return basic entries without disk I/O
    unsafe { watos_arch::serial_write(b"[WFS] wfs_readdir called (stub mode)\r\n"); }

    let mut pos = 0;

    // Add "." and ".." entries
    let dot = b"D . 0\n";
    if pos + dot.len() < buf.len() {
        buf[pos..pos + dot.len()].copy_from_slice(dot);
        pos += dot.len();
    }

    let dotdot = b"D .. 0\n";
    if pos + dotdot.len() < buf.len() {
        buf[pos..pos + dotdot.len()].copy_from_slice(dotdot);
        pos += dotdot.len();
    }

    // Add a test entry without disk I/O
    let test = b"F TEST.TXT 100\n";
    if pos + test.len() < buf.len() {
        buf[pos..pos + test.len()].copy_from_slice(test);
        pos += test.len();
    }

    unsafe { watos_arch::serial_write(b"[WFS] wfs_readdir returning\r\n"); }
    pos
}

/// Create a directory in WFS
/// Returns true on success
/// TODO: Reimplement using WFS v1 B+tree API or VFS layer
fn wfs_mkdir(_path: &[u8]) -> bool {
    // WFS v1 uses B+tree structure, not flat file table
    // Direct manipulation should go through VFS layer
    false
}

/// Format a number as a decimal string
/// Returns a static buffer with the result
fn format_num(mut n: u64) -> &'static str {
    static mut NUM_BUF: [u8; 20] = [0; 20];

    unsafe {
        if n == 0 {
            NUM_BUF[0] = b'0';
            return core::str::from_utf8_unchecked(&NUM_BUF[..1]);
        }

        let mut i = 20;
        while n > 0 && i > 0 {
            i -= 1;
            NUM_BUF[i] = (n % 10) as u8 + b'0';
            n /= 10;
        }

        core::str::from_utf8_unchecked(&NUM_BUF[i..])
    }
}

/// Kernel Console Subsystem - ring buffer for process output
/// All SYS_WRITE calls go here, console app reads from here
const CONSOLE_BUFFER_SIZE: usize = 4096;
static mut CONSOLE_BUFFER: [u8; CONSOLE_BUFFER_SIZE] = [0; CONSOLE_BUFFER_SIZE];
static mut CONSOLE_READ_POS: usize = 0;
static mut CONSOLE_WRITE_POS: usize = 0;

/// Write bytes to the console ring buffer
fn console_write(data: &[u8]) {
    unsafe {
        watos_arch::serial_write(b"[CBUF_W:");
        watos_arch::serial_hex(data.len() as u64);
        watos_arch::serial_write(b"]");
        for &byte in data {
            let next_write = (CONSOLE_WRITE_POS + 1) % CONSOLE_BUFFER_SIZE;
            // If buffer is full, drop oldest byte (advance read pos)
            if next_write == CONSOLE_READ_POS {
                CONSOLE_READ_POS = (CONSOLE_READ_POS + 1) % CONSOLE_BUFFER_SIZE;
            }
            CONSOLE_BUFFER[CONSOLE_WRITE_POS] = byte;
            CONSOLE_WRITE_POS = next_write;
        }
    }
}

/// Read bytes from the console ring buffer (non-blocking)
/// Returns number of bytes read
fn console_read(buf: &mut [u8]) -> usize {
    unsafe {
        let mut count = 0;
        while count < buf.len() && CONSOLE_READ_POS != CONSOLE_WRITE_POS {
            buf[count] = CONSOLE_BUFFER[CONSOLE_READ_POS];
            CONSOLE_READ_POS = (CONSOLE_READ_POS + 1) % CONSOLE_BUFFER_SIZE;
            count += 1;
        }
        if count > 0 {
            watos_arch::serial_write(b"[CBUF_R:");
            watos_arch::serial_hex(count as u64);
            watos_arch::serial_write(b"]");
        }
        count
    }
}

// ============================================================================
// Keyboard Scancode to ASCII Conversion
// ============================================================================

/// Convert PS/2 scancode to ASCII character using the keyboard driver
fn scancode_to_ascii(scancode: u8) -> u8 {
    watos_driver_keyboard::process_scancode(scancode)
}

// ============================================================================
// File Descriptor Table
// ============================================================================

/// Maximum number of open file descriptors
const MAX_FDS: usize = 64;

/// File descriptor entry
struct FileDescriptor {
    /// The open file operations object
    file: Option<Box<dyn FileOperations>>,
}

impl FileDescriptor {
    const fn empty() -> Self {
        FileDescriptor { file: None }
    }
}

/// Global file descriptor table
/// fd 0 = console input buffer (special)
/// fd 1 = stdout (console output)
/// fd 2 = stderr (console output)
/// fd 3+ = regular files
static FD_TABLE: Mutex<[Option<Box<dyn FileOperations>>; MAX_FDS]> =
    Mutex::new([const { None }; MAX_FDS]);

/// Allocate a new file descriptor for an open file
/// Returns the fd number, or -1 if table is full
fn fd_alloc(file: Box<dyn FileOperations>) -> i64 {
    let mut table = FD_TABLE.lock();
    // Start from fd 3 (0=console, 1=stdout, 2=stderr are special)
    for fd in 3..MAX_FDS {
        if table[fd].is_none() {
            table[fd] = Some(file);
            return fd as i64;
        }
    }
    -1 // No free fd
}

/// Close a file descriptor
fn fd_close(fd: i64) -> i64 {
    if fd < 3 || fd >= MAX_FDS as i64 {
        return -1; // Invalid fd (can't close 0, 1, 2)
    }
    let mut table = FD_TABLE.lock();
    if table[fd as usize].take().is_some() {
        0
    } else {
        -1 // Was not open
    }
}

/// Read from a file descriptor
fn fd_read(fd: i64, buf: &mut [u8]) -> i64 {
    if fd < 3 || fd >= MAX_FDS as i64 {
        return -1;
    }

    kernel_debug! {
        watos_arch::serial_write(b"  [fd_read] Acquiring FD_TABLE lock...\r\n");
    }

    let mut table = FD_TABLE.lock();

    kernel_debug! {
        watos_arch::serial_write(b"  [fd_read] Lock acquired, calling file.read()...\r\n");
    }

    if let Some(ref mut file) = table[fd as usize] {
        let result = file.read(buf);

        kernel_debug! {
            watos_arch::serial_write(b"  [fd_read] file.read() returned, processing result...\r\n");
        }

        match result {
            Ok(n) => {
                kernel_debug! {
                    watos_arch::serial_write(b"  [fd_read] Success, ");
                    watos_arch::serial_hex(n as u64);
                    watos_arch::serial_write(b" bytes\r\n");
                }
                n as i64
            },
            Err(_) => {
                kernel_debug! {
                    watos_arch::serial_write(b"  [fd_read] Error\r\n");
                }
                -1
            }
        }
    } else {
        kernel_debug! {
            watos_arch::serial_write(b"  [fd_read] FD not open\r\n");
        }
        -1 // Not open
    }
}

/// Find a preloaded app by name (case-insensitive)
fn find_preloaded_app(name: &[u8]) -> Option<(u64, u64)> {
    unsafe {
        let info = BOOT_INFO.as_ref()?;
        for i in 0..(info.app_count as usize) {
            let app = &info.apps[i];
            // Get the app's name length (null-terminated)
            let app_name_len = app.name.iter().position(|&c| c == 0).unwrap_or(32);
            let app_name = &app.name[..app_name_len];

            // Compare names (case-insensitive for flexibility)
            if name.len() == app_name.len() {
                let matches = name.iter().zip(app_name.iter()).all(|(a, b)| {
                    let a_lower = if *a >= b'A' && *a <= b'Z' { *a + 32 } else { *a };
                    let b_lower = if *b >= b'A' && *b <= b'Z' { *b + 32 } else { *b };
                    a_lower == b_lower
                });
                if matches {
                    return Some((app.addr, app.size));
                }
            }
        }
        None
    }
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    // 1. Init heap
    unsafe {
        ALLOCATOR.lock().init(HEAP_START as *mut u8, HEAP_SIZE);
    }

    // 2. Init architecture (GDT, IDT, PIC)
    let kernel_stack = HEAP_START as u64 + HEAP_SIZE as u64;
    watos_arch::init(kernel_stack);

    // Enable timer interrupt (IRQ0) for tick counter
    watos_arch::pic::enable_timer();

    unsafe { watos_arch::serial_write(b"WATOS kernel started\r\n"); }

    // 3. Copy boot info
    unsafe {
        let boot_info = &*(BOOT_INFO_ADDR as *const BootInfo);
        if boot_info.magic != BOOT_MAGIC {
            watos_arch::serial_write(b"ERROR: Invalid boot magic\r\n");
            loop { watos_arch::halt(); }
        }
        BOOT_INFO = Some(*boot_info);

        watos_arch::serial_write(b"[KERNEL] Framebuffer: ");
        watos_arch::serial_hex(boot_info.framebuffer_width as u64);
        watos_arch::serial_write(b"x");
        watos_arch::serial_hex(boot_info.framebuffer_height as u64);
        watos_arch::serial_write(b"\r\n");
    }

    // 4. Install syscall handler
    watos_arch::idt::install_syscall_handler(syscall_handler);
    unsafe { watos_arch::serial_write(b"[KERNEL] Syscall handler installed\r\n"); }

    // 4.5. Initialize Physical Memory Manager (PMM)
    // Uses layout::PHYS_ALLOCATOR_START (16MB) as base
    // Gives 128MB for user process physical pages
    watos_mem::pmm::init(watos_mem::PHYS_ALLOCATOR_START, 128 * 1024 * 1024);
    unsafe { watos_arch::serial_write(b"[KERNEL] Physical allocator initialized (128MB @ 16MB)\r\n"); }

// 5. Initialize process subsystem
    watos_process::init();
    unsafe { watos_arch::serial_write(b"[KERNEL] Process subsystem initialized\r\n"); }

    // 5.3 Initialize user management subsystem
    watos_users::init();
    unsafe { watos_arch::serial_write(b"[KERNEL] User management initialized\r\n"); }

    // 5.4 Initialize console session manager
    watos_console::init();
    unsafe { watos_arch::serial_write(b"[KERNEL] Console session manager initialized\r\n"); }

    // 5.4 Initialize video driver from boot info
    unsafe {
        if let Some(info) = BOOT_INFO {
            if info.framebuffer_addr != 0 {
                let is_bgr = info.pixel_format == 1;
                watos_driver_video::init_from_boot_info(
                    info.framebuffer_addr,
                    info.framebuffer_width,
                    info.framebuffer_height,
                    info.framebuffer_pitch,
                    info.framebuffer_bpp,
                    is_bgr,
                );
                watos_arch::serial_write(b"[KERNEL] Video driver initialized\r\n");

                // Initialize VT subsystem (kernel virtual terminals)
                watos_vt::init(
                    info.framebuffer_addr as usize,
                    info.framebuffer_width,
                    info.framebuffer_height,
                    info.framebuffer_pitch,
                    info.framebuffer_bpp,
                    is_bgr,
                );
            } else {
                watos_arch::serial_write(b"[KERNEL] WARNING: No framebuffer from bootloader\r\n");
            }
        }
    }

    // 5.5 Initialize VFS and mount boot disk as C:
    init_cwd();
    let vfs_ok = init_vfs();
    if !vfs_ok {
        unsafe { watos_arch::serial_write(b"[KERNEL] WARNING: VFS init failed\r\n"); }
    }

    // Also check for WFS data disk on other ports
    let wfs_found = init_disk();
    if wfs_found {
        // Also add to legacy drive table for CURRENT_DRIVE tracking
        drive_mount(b"D", b"/", b"WFS");
    }

    // 6. Execute init app - try login first, then fall back to TERM.EXE
    unsafe {
        if let Some(info) = BOOT_INFO {
            // First try to find and launch login app from preloaded apps
            let login_found = find_preloaded_app(b"login");
            
            let (name_bytes, app_data_opt): (&[u8], Option<(u64, u64)>) = if login_found.is_some() {
                (b"login", login_found)
            } else if info.init_app_addr != 0 && info.init_app_size != 0 {
                (b"TERM.EXE", Some((info.init_app_addr, info.init_app_size)))
            } else {
                (b"", None)
            };

            match app_data_opt {
                Some((addr, size)) => {
                    watos_arch::serial_write(b"[KERNEL] Launching ");
                    watos_arch::serial_write(name_bytes);
                    watos_arch::serial_write(b" at 0x");
                    watos_arch::serial_hex(addr);
                    watos_arch::serial_write(b" (");
                    watos_arch::serial_hex(size);
                    watos_arch::serial_write(b" bytes)\r\n");

                    // Create slice from the loaded app data
                    let app_data = core::slice::from_raw_parts(
                        addr as *const u8,
                        size as usize,
                    );

                    // Execute the app
                    let name_str = core::str::from_utf8(name_bytes).unwrap_or("app");
                    match watos_process::exec(name_str, app_data, name_str) {
                        Ok(pid) => {
                            watos_arch::serial_write(b"[KERNEL] ");
                            watos_arch::serial_write(name_bytes);
                            watos_arch::serial_write(b" running as PID ");
                            watos_arch::serial_hex(pid as u64);
                            watos_arch::serial_write(b"\r\n");
                        }
                        Err(e) => {
                            watos_arch::serial_write(b"[KERNEL] Failed to exec ");
                            watos_arch::serial_write(name_bytes);
                            watos_arch::serial_write(b": ");
                            watos_arch::serial_write(e.as_bytes());
                            watos_arch::serial_write(b"\r\n");
                        }
                    }
                }
                None => {
                    watos_arch::serial_write(b"[KERNEL] No init app loaded\r\n");
                }
            }
        }
    }

    // 7. Idle loop (should not reach here if init app runs)
    unsafe { watos_arch::serial_write(b"[KERNEL] Entering idle loop\r\n"); }
    loop {
        watos_arch::halt();
    }
}

// ============================================================================
// Syscall Interface (numbers from watos-syscall crate)
// ============================================================================

/// Syscall numbers - must match watos_syscall::numbers
mod syscall {
    // Console/IO
    pub const SYS_WRITE: u64 = 1;
    pub const SYS_READ: u64 = 2;
    pub const SYS_OPEN: u64 = 3;
    pub const SYS_CLOSE: u64 = 4;
    pub const SYS_GETKEY: u64 = 5;
    pub const SYS_EXIT: u64 = 6;

    // System
    pub const SYS_SLEEP: u64 = 11;
    pub const SYS_MALLOC: u64 = 14;
    pub const SYS_FREE: u64 = 15;
    pub const SYS_PUTCHAR: u64 = 16;
    pub const SYS_SBRK: u64 = 17;      // Set heap break (returns old break)
    pub const SYS_HEAP_BREAK: u64 = 18; // Get current heap break

    // Console handle management
    pub const SYS_CONSOLE_IN: u64 = 20;    // Get stdin handle (returns 0)
    pub const SYS_CONSOLE_OUT: u64 = 21;   // Get stdout handle (returns 1)
    pub const SYS_CONSOLE_ERR: u64 = 22;   // Get stderr handle (returns 2)

    // Framebuffer
    pub const SYS_FB_INFO: u64 = 50;
    pub const SYS_FB_ADDR: u64 = 51;
    pub const SYS_FB_DIMENSIONS: u64 = 52;

    // Raw keyboard
    pub const SYS_READ_SCANCODE: u64 = 60;

    // VGA Graphics
    pub const SYS_VGA_SET_MODE: u64 = 30;
    pub const SYS_VGA_SET_PIXEL: u64 = 31;
    pub const SYS_VGA_GET_PIXEL: u64 = 32;
    pub const SYS_VGA_BLIT: u64 = 33;
    pub const SYS_VGA_CLEAR: u64 = 34;
    pub const SYS_VGA_FLIP: u64 = 35;
    pub const SYS_VGA_SET_PALETTE: u64 = 36;
    pub const SYS_VGA_CREATE_SESSION: u64 = 37;
    pub const SYS_VGA_DESTROY_SESSION: u64 = 38;
    pub const SYS_VGA_SET_ACTIVE_SESSION: u64 = 39;
    pub const SYS_VGA_GET_SESSION_INFO: u64 = 40;
    pub const SYS_VGA_ENUMERATE_MODES: u64 = 41;

    // GFX Graphics (GWBASIC SCREEN commands)
    pub const SYS_GFX_PSET: u64 = 40;      // Set pixel
    pub const SYS_GFX_LINE: u64 = 41;      // Draw line
    pub const SYS_GFX_CIRCLE: u64 = 42;    // Draw circle
    pub const SYS_GFX_CLS: u64 = 43;       // Clear graphics screen
    pub const SYS_GFX_MODE: u64 = 44;      // Set graphics mode
    pub const SYS_GFX_DISPLAY: u64 = 45;   // Display graphics buffer

    // Process management
    pub const SYS_GETPID: u64 = 12;

    // Process execution
    pub const SYS_EXEC: u64 = 80;
    pub const SYS_GETARGS: u64 = 83;

    // Date/Time
    pub const SYS_GETDATE: u64 = 90;
    pub const SYS_GETTIME: u64 = 91;
    pub const SYS_GETTICKS: u64 = 92;

    // Drive/Mount operations
    pub const SYS_MOUNT: u64 = 78;
    pub const SYS_UNMOUNT: u64 = 79;
    pub const SYS_CHDIR: u64 = 77;
    pub const SYS_GETCWD: u64 = 76;
    pub const SYS_LISTDRIVES: u64 = 85;

    // Filesystem operations
    pub const SYS_READDIR: u64 = 71;
    pub const SYS_MKDIR: u64 = 72;
    pub const SYS_STAT: u64 = 70;

    // User authentication and session management
    pub const SYS_AUTHENTICATE: u64 = 120;
    pub const SYS_SETUID: u64 = 121;
    pub const SYS_GETUID: u64 = 122;
    pub const SYS_GETGID: u64 = 123;
    pub const SYS_SETGID: u64 = 124;
    pub const SYS_GETEUID: u64 = 126;
    pub const SYS_GETEGID: u64 = 127;

    // Permission operations
    pub const SYS_CHMOD: u64 = 140;
    pub const SYS_CHOWN: u64 = 141;
    pub const SYS_ACCESS: u64 = 142;

    // Console session management
    pub const SYS_SESSION_CREATE: u64 = 130;
    pub const SYS_SESSION_SWITCH: u64 = 131;
    pub const SYS_SESSION_GET_CURRENT: u64 = 132;

    // Memory info
    pub const SYS_MEMINFO: u64 = 135;

    // Environment variables
    pub const SYS_SETENV: u64 = 136;
    pub const SYS_GETENV: u64 = 137;
    pub const SYS_UNSETENV: u64 = 138;
    pub const SYS_LISTENV: u64 = 139;

    // Keyboard configuration
    pub const SYS_SET_KEYMAP: u64 = 150;
    pub const SYS_SET_CODEPAGE: u64 = 151;
    pub const SYS_GET_CODEPAGE: u64 = 152;
}

/// Syscall handler - naked function called from IDT
///
/// When INT 0x80 is called from Ring 3:
/// - CPU pushes SS, RSP, RFLAGS, CS, RIP onto kernel stack
/// - We must save user registers, handle syscall, restore registers, IRETQ
#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn syscall_handler() {
    core::arch::naked_asm!(
        // Save original register state to global (for parent context saving during exec)
        // This must be done BEFORE we modify any registers
        "mov qword ptr [rip + {saved_regs} + 0], rbx",   // Save RBX
        "mov qword ptr [rip + {saved_regs} + 8], rcx",   // Save RCX
        "mov qword ptr [rip + {saved_regs} + 16], rdx",  // Save RDX
        "mov qword ptr [rip + {saved_regs} + 24], rsi",  // Save RSI
        "mov qword ptr [rip + {saved_regs} + 32], rdi",  // Save RDI
        "mov qword ptr [rip + {saved_regs} + 40], r8",   // Save R8
        "mov qword ptr [rip + {saved_regs} + 48], r9",   // Save R9
        "mov qword ptr [rip + {saved_regs} + 56], r10",  // Save R10
        "mov qword ptr [rip + {saved_regs} + 64], r11",  // Save R11
        "mov qword ptr [rip + {saved_regs} + 72], rbp",  // Save RBP
        "mov qword ptr [rip + {saved_regs} + 80], r12",  // Save R12
        "mov qword ptr [rip + {saved_regs} + 88], r13",  // Save R13
        "mov qword ptr [rip + {saved_regs} + 96], r14",  // Save R14
        "mov qword ptr [rip + {saved_regs} + 104], r15", // Save R15

        // Save all caller-saved registers on stack (syscall may clobber them)
        // RAX contains syscall number, will be overwritten with result
        // RDI, RSI, RDX contain args 1-3
        "push rbx",
        "push rcx",
        "push rdx",
        "push rsi",
        "push rdi",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        "push rbp",

        // Call the inner handler
        // Args are already in correct registers: rdi=arg1, rsi=arg2, rdx=arg3
        // Move syscall number (rax) to rdi, shift others

        // Read interrupt frame values for parent context saving
        // Stack layout: 10 saved regs (80 bytes) then interrupt frame
        // Interrupt frame: RIP at +80, CS at +88, RFLAGS at +96, RSP at +104, SS at +112
        "mov r8, [rsp + 80]",   // RIP from interrupt frame -> r8 (5th param)
        "mov r9, [rsp + 104]",  // RSP from interrupt frame -> r9 (6th param)

        "mov rcx, rdx",     // arg3 -> rcx (4th param)
        "mov rdx, rsi",     // arg2 -> rdx (3rd param)
        "mov rsi, rdi",     // arg1 -> rsi (2nd param)
        "mov rdi, rax",     // syscall_num -> rdi (1st param)

        // Call Rust handler
        "call {handler}",

        // Result is in RAX - leave it there for user

        // Restore registers (except RAX which has result)
        "pop rbp",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop rcx",
        "pop rbx",

        // Return to user mode
        "iretq",

        handler = sym handle_syscall_inner,
        saved_regs = sym SAVED_SYSCALL_REGS,
    );
}

/// Static buffer for copying user data before CR3 switch
static mut SYSCALL_PATH_BUF: [u8; 256] = [0u8; 256];
/// Static buffer for file read operations
static mut SYSCALL_READ_BUF: [u8; 4096] = [0u8; 4096];

/// Saved register state from syscall entry (for parent context saving)
#[repr(C)]
struct SavedSyscallRegs {
    rbx: u64,
    rcx: u64,
    rdx: u64,
    rsi: u64,
    rdi: u64,
    r8: u64,
    r9: u64,
    r10: u64,
    r11: u64,
    rbp: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
}

static mut SAVED_SYSCALL_REGS: SavedSyscallRegs = SavedSyscallRegs {
    rbx: 0, rcx: 0, rdx: 0, rsi: 0, rdi: 0, rbp: 0,
    r8: 0, r9: 0, r10: 0, r11: 0, r12: 0, r13: 0, r14: 0, r15: 0,
};

/// Inner syscall handler - called from naked handler
/// return_rip and return_rsp are from the interrupt frame for saving parent context
#[inline(never)]
extern "C" fn handle_syscall_inner(num: u64, arg1: u64, arg2: u64, arg3: u64, return_rip: u64, return_rsp: u64) -> u64 {
    // Enable user memory access (SMAP bypass) for the duration of syscall handling.
    // This is required because syscalls often need to read/write user buffers.
    watos_mem::stac();

    let result = handle_syscall_impl(num, arg1, arg2, arg3, return_rip, return_rsp);

    // Restore SMAP protection before returning to user mode
    watos_mem::clac();

    result
}

fn handle_syscall_impl(num: u64, arg1: u64, arg2: u64, arg3: u64, return_rip: u64, return_rsp: u64) -> u64 {
    // For file I/O syscalls that access disk, we need to switch to kernel page table
    // to access AHCI MMIO. But we must copy user data first since user pointers
    // become invalid after CR3 switch.

    match num {
        syscall::SYS_OPEN => {
            // arg1 = path pointer, arg2 = path length, arg3 = mode
            let path_ptr = arg1 as *const u8;
            let path_len = (arg2 as usize).min(255);

            if path_ptr.is_null() || path_len == 0 {
                return u64::MAX;
            }

            // Copy path from user memory while still in user page table
            let path_copy: &[u8] = unsafe {
                let user_path = core::slice::from_raw_parts(path_ptr, path_len);
                SYSCALL_PATH_BUF[..path_len].copy_from_slice(user_path);
                &SYSCALL_PATH_BUF[..path_len]
            };

            // Now switch to kernel page table for disk access
            let user_cr3 = watos_mem::paging::get_cr3();
            let kernel_pml4 = watos_process::get_kernel_pml4();

            if kernel_pml4 != 0 && user_cr3 != kernel_pml4 {
                unsafe { watos_mem::paging::load_cr3(kernel_pml4); }
            }

            // Call the open handler with kernel buffer
            let result = handle_sys_open(path_copy, arg3);

            // Restore user page table
            if kernel_pml4 != 0 && user_cr3 != kernel_pml4 {
                unsafe { watos_mem::paging::load_cr3(user_cr3); }
            }

            result
        }

        syscall::SYS_CLOSE => {
            // SYS_CLOSE only needs fd number, no user pointers
            let user_cr3 = watos_mem::paging::get_cr3();
            let kernel_pml4 = watos_process::get_kernel_pml4();

            if kernel_pml4 != 0 && user_cr3 != kernel_pml4 {
                unsafe { watos_mem::paging::load_cr3(kernel_pml4); }
            }

            let result = handle_syscall(num, arg1, arg2, arg3, return_rip, return_rsp);

            if kernel_pml4 != 0 && user_cr3 != kernel_pml4 {
                unsafe { watos_mem::paging::load_cr3(user_cr3); }
            }

            result
        }

        syscall::SYS_READ if arg1 >= 3 => {
            // File read needs special handling:
            // 1. Switch to kernel CR3 to read from disk
            // 2. Switch back to user CR3 to copy to user buffer
            let fd = arg1 as i64;
            let user_buf_ptr = arg2 as *mut u8;
            let user_buf_len = (arg3 as usize).min(4096);

            if user_buf_ptr.is_null() || user_buf_len == 0 {
                return 0;
            }

            // Switch to kernel page table for disk access
            let user_cr3 = watos_mem::paging::get_cr3();
            let kernel_pml4 = watos_process::get_kernel_pml4();

            if kernel_pml4 != 0 && user_cr3 != kernel_pml4 {
                unsafe { watos_mem::paging::load_cr3(kernel_pml4); }
            }

            // Read into kernel buffer
            let bytes_read = unsafe {
                use core::ptr::addr_of_mut;
                let buf = &mut *addr_of_mut!(SYSCALL_READ_BUF);
                let kernel_buf = &mut buf[..user_buf_len];
                let result = fd_read(fd, kernel_buf);
                if result < 0 { 0usize } else { result as usize }
            };

            // Switch back to user page table
            if kernel_pml4 != 0 && user_cr3 != kernel_pml4 {
                unsafe { watos_mem::paging::load_cr3(user_cr3); }
            }

            // Copy from kernel buffer to user buffer
            if bytes_read > 0 {
                unsafe {
                    let user_buf = core::slice::from_raw_parts_mut(user_buf_ptr, bytes_read);
                    user_buf.copy_from_slice(&SYSCALL_READ_BUF[..bytes_read]);
                }
            }

            bytes_read as u64
        }

        _ => {
            // All other syscalls run in user page table
            handle_syscall(num, arg1, arg2, arg3, return_rip, return_rsp)
        }
    }
}

/// Handle SYS_OPEN with path already copied to kernel buffer
fn handle_sys_open(path: &[u8], mode_flags: u64) -> u64 {
    let path_str = match core::str::from_utf8(path) {
        Ok(s) => s,
        Err(_) => return u64::MAX,
    };

    unsafe {
        watos_arch::serial_write(b"[KERNEL] SYS_OPEN: ");
        watos_arch::serial_write(path);
        watos_arch::serial_write(b"\r\n");
    }

    // Build FileMode based on flags
    let mode = if mode_flags == 0 {
        FileMode::READ
    } else if mode_flags == 1 {
        FileMode::WRITE
    } else {
        FileMode::READ_WRITE
    };

    // Open via VFS
    match watos_vfs::open(path_str, mode) {
        Ok(file) => {
            let fd = fd_alloc(file);
            unsafe {
                watos_arch::serial_write(b"[KERNEL] Opened fd=");
                watos_arch::serial_hex(fd as u64);
                watos_arch::serial_write(b"\r\n");
            }
            fd as u64
        }
        Err(e) => {
            unsafe {
                watos_arch::serial_write(b"[KERNEL] Open failed: ");
                match e {
                    VfsError::NotFound => watos_arch::serial_write(b"NotFound"),
                    VfsError::IoError => watos_arch::serial_write(b"IoError"),
                    _ => watos_arch::serial_write(b"Other"),
                }
                watos_arch::serial_write(b"\r\n");
            }
            u64::MAX
        }
    }
}

fn handle_syscall(num: u64, arg1: u64, arg2: u64, arg3: u64, return_rip: u64, return_rsp: u64) -> u64 {
    match num {
        syscall::SYS_EXIT => {
            // Check if there's a parent process to return to
            if watos_process::has_parent_context() {
                // CRITICAL: Switch to kernel page table BEFORE freeing child process
                // The child's page table will be deallocated when we free the process,
                // so we must not be using it (CR3) at that point!
                let kernel_pml4 = watos_process::get_kernel_pml4();
                unsafe { watos_mem::paging::load_cr3(kernel_pml4); }

                // Now safe to free the current child process
                watos_process::free_current_process();

                // Resume parent (this will switch to parent's page table)
                watos_process::resume_parent(); // Never returns
            }
            // No parent - top-level process exiting, halt
            loop { watos_arch::halt(); }
        }

        syscall::SYS_WRITE => {
            // arg1 = fd (0=serial only, 1=stdout/console, 2=stderr/console)
            // arg2 = pointer to string
            // arg3 = length
            let fd = arg1;
            let ptr = arg2 as *const u8;
            let len = arg3 as usize;

            unsafe {
                let slice = core::slice::from_raw_parts(ptr, len);

                // Always write to serial for debugging
                watos_arch::serial_write(slice);

                // If fd is stdout (1) or stderr (2), write to active VT
                // The kernel VT driver will render it to the framebuffer
                if fd == 1 || fd == 2 {
                    watos_vt::vt_write_active(slice);
                }
            }
            len as u64
        }

        syscall::SYS_READ => {
            // arg1 = fd, arg2 = buffer, arg3 = max_len
            // fd=0: read from console buffer (what other processes wrote to stdout)
            // fd>=3: read from open file
            let fd = arg1 as i64;
            let buf_ptr = arg2 as *mut u8;
            let buf_size = arg3 as usize;

            if buf_ptr.is_null() || buf_size == 0 {
                return 0;
            }

            if fd == 0 {
                // Read from console output buffer
                let mut temp = [0u8; 256];
                let read_size = buf_size.min(temp.len());
                let count = console_read(&mut temp[..read_size]);
                if count > 0 {
                    unsafe {
                        core::ptr::copy_nonoverlapping(temp.as_ptr(), buf_ptr, count);
                    }
                }
                count as u64
            } else if fd >= 3 {
                // Read from open file via VFS - switch to kernel page table for disk access
                let user_cr3 = watos_mem::paging::get_cr3();
                let kernel_pml4 = watos_process::get_kernel_pml4();

                if kernel_pml4 != 0 && user_cr3 != kernel_pml4 {
                    unsafe { watos_mem::paging::load_cr3(kernel_pml4); }
                }

                // Read in kernel space using static buffer, then copy to user
                let result = unsafe {
                    use core::ptr::{addr_of, addr_of_mut};
                    let buf_ref = &*addr_of!(SYSCALL_READ_BUF);
                    let max_chunk = buf_size.min(buf_ref.len());
                    let buf = &mut *addr_of_mut!(SYSCALL_READ_BUF);
                    let result = fd_read(fd, &mut buf[..max_chunk]);
                    result
                };

                // Restore user page table
                if kernel_pml4 != 0 && user_cr3 != kernel_pml4 {
                    unsafe { watos_mem::paging::load_cr3(user_cr3); }
                }

                // Copy data to user buffer (now in user page table)
                if result > 0 {
                    unsafe {
                        let copy_len = (result as usize).min(buf_size);
                        core::ptr::copy_nonoverlapping(SYSCALL_READ_BUF.as_ptr(), buf_ptr, copy_len);
                    }
                    result as u64
                } else {
                    0
                }
            } else {
                // fd 1 or 2 - can't read from stdout/stderr
                0
            }
        }

        // SYS_OPEN is handled specially in handle_syscall_inner for CR3 switching

        syscall::SYS_CLOSE => {
            // arg1 = file descriptor
            // Returns 0 on success, error on failure
            let fd = arg1 as i64;
            if fd_close(fd) == 0 {
                0
            } else {
                u64::MAX
            }
        }

        syscall::SYS_CONSOLE_IN => {
            // Return stdin file descriptor
            0
        }

        syscall::SYS_CONSOLE_OUT => {
            // Return stdout file descriptor
            1
        }

        syscall::SYS_CONSOLE_ERR => {
            // Return stderr file descriptor
            2
        }

        syscall::SYS_GETKEY => {
            // Returns ASCII key or 0 if no key
            watos_arch::idt::get_scancode().map(|scancode| {
                scancode_to_ascii(scancode) as u64
            }).unwrap_or(0)
        }

        syscall::SYS_MALLOC => {
            // arg1 = size, returns pointer in user heap (0x800000+)
            // Uses per-process user heap, NOT kernel heap
            let size = arg1 as usize;
            watos_process::heap_alloc(size)
        }

        syscall::SYS_FREE => {
            // arg1 = pointer, arg2 = size
            // Currently a no-op - user heap is bump-allocated, freed on process exit
            // User-space can implement proper malloc/free on top of sbrk
            0
        }

        syscall::SYS_SBRK => {
            // arg1 = new break address
            // Returns old break on success, u64::MAX on failure
            let new_break = arg1;
            watos_process::set_heap_break(new_break)
        }

        syscall::SYS_HEAP_BREAK => {
            // Returns current heap break
            watos_process::heap_break()
        }

        syscall::SYS_FB_INFO => {
            // Returns pointer to BootInfo struct
            unsafe {
                BOOT_INFO.as_ref().map(|b| b as *const _ as u64).unwrap_or(0)
            }
        }

        syscall::SYS_FB_ADDR => {
            // Returns framebuffer address
            unsafe {
                BOOT_INFO.map(|b| b.framebuffer_addr).unwrap_or(0)
            }
        }

        syscall::SYS_FB_DIMENSIONS => {
            // Returns width/height/pitch packed
            // Format: high 32 bits = width | mid 16 bits = height | low 16 bits = pitch/4
            unsafe {
                BOOT_INFO.map(|b| {
                    let w = b.framebuffer_width as u64;
                    let h = b.framebuffer_height as u64;
                    let p = (b.framebuffer_pitch / 4) as u64;
                    (w << 32) | (h << 16) | p
                }).unwrap_or(0)
            }
        }

        syscall::SYS_READ_SCANCODE => {
            // Returns raw PS/2 scancode or 0 if no key
            watos_arch::idt::get_scancode().map(|s| s as u64).unwrap_or(0)
        }

        syscall::SYS_EXEC => {
            // arg1 = pointer to full command line string
            // arg2 = length of command line
            // Returns: 0 on success, non-zero on error
            let cmdline_ptr = arg1 as *const u8;
            let cmdline_len = arg2 as usize;

            if cmdline_ptr.is_null() || cmdline_len == 0 || cmdline_len > 256 {
                return u64::MAX; // Invalid args
            }

            // Copy cmdline from user memory while still in user page table
            let mut cmdline_buf = [0u8; 256];
            let cmdline_copy = unsafe {
                let user_cmdline = core::slice::from_raw_parts(cmdline_ptr, cmdline_len);
                cmdline_buf[..cmdline_len].copy_from_slice(user_cmdline);
                &cmdline_buf[..cmdline_len]
            };

            // Extract program name (first word before space)
            let program_name = {
                let space_pos = cmdline_copy.iter().position(|&c| c == b' ').unwrap_or(cmdline_len);
                &cmdline_copy[..space_pos]
            };

            // Convert program name to string and build path
            let program_str = core::str::from_utf8(program_name).unwrap_or("app");

            // Build full path: C:/apps/system/<name>
            let mut full_path_buf = [0u8; 128];
            let prefix = b"C:/apps/system/";
            let mut pos = 0;
            for &b in prefix {
                if pos < full_path_buf.len() {
                    full_path_buf[pos] = b;
                    pos += 1;
                }
            }
            for &b in program_name {
                if pos < full_path_buf.len() {
                    full_path_buf[pos] = b;
                    pos += 1;
                }
            }
            let full_path_str = core::str::from_utf8(&full_path_buf[..pos]).unwrap_or("");

            // Build system path: C:/system/<name>
            let mut system_path_buf = [0u8; 128];
            let system_prefix = b"C:/system/";
            let mut sys_pos = 0;
            for &b in system_prefix {
                if sys_pos < system_path_buf.len() {
                    system_path_buf[sys_pos] = b;
                    sys_pos += 1;
                }
            }
            for &b in program_name {
                if sys_pos < system_path_buf.len() {
                    system_path_buf[sys_pos] = b;
                    sys_pos += 1;
                }
            }
            let system_path_str = core::str::from_utf8(&system_path_buf[..sys_pos]).unwrap_or("");

            // Try multiple paths to find the executable
            let paths = [
                program_str,       // Try as-is (e.g., "/apps/system/shell")
                system_path_str,   // Try C:/system/<name> (for term, etc.)
                full_path_str,     // Try C:/apps/system/<name>
            ];

            // Switch to kernel page table for disk access
            let user_cr3 = watos_mem::paging::get_cr3();
            let kernel_pml4 = watos_process::get_kernel_pml4();

            if kernel_pml4 != 0 && user_cr3 != kernel_pml4 {
                unsafe { watos_mem::paging::load_cr3(kernel_pml4); }
            }

            // Now we can safely use heap allocation
            extern crate alloc;
            use alloc::vec::Vec;

            let mut app_data: Option<Vec<u8>> = None;

            for path in &paths {
                if path.is_empty() {
                    continue;
                }

                vfs_debug! {
                    watos_arch::serial_write(b"[VFS] Trying to load: ");
                    watos_arch::serial_write(path.as_bytes());
                    watos_arch::serial_write(b"\r\n");
                }

                // Try to open and read the file from VFS
                let fd = handle_sys_open(path.as_bytes(), 0); // 0 = read mode
                if fd != u64::MAX {
                    vfs_debug! {
                        watos_arch::serial_write(b"[VFS] File opened, allocating buffer...\r\n");
                    }

                    // Read file using Vec
                    let mut file_contents = Vec::new();
                    const CHUNK_SIZE: usize = 4096;
                    let mut read_buf = [0u8; CHUNK_SIZE];

                    kernel_debug! {
                        watos_arch::serial_write(b"[KERNEL] Starting read loop...\r\n");
                    }

                    let mut iteration = 0;
                    loop {
                        kernel_debug! {
                            watos_arch::serial_write(b"[KERNEL] Calling fd_read #");
                            watos_arch::serial_hex(iteration + 1);
                            watos_arch::serial_write(b"...\r\n");
                        }
                        let chunk_read = fd_read(fd as i64, &mut read_buf);
                        iteration += 1;
                        kernel_debug! {
                            watos_arch::serial_write(b"[KERNEL] fd_read returned ");
                            watos_arch::serial_hex(chunk_read as u64);
                            watos_arch::serial_write(b"\r\n");
                        }

                        if chunk_read <= 0 {
                            kernel_debug! {
                                watos_arch::serial_write(b"[KERNEL] EOF after ");
                                watos_arch::serial_hex(iteration);
                                watos_arch::serial_write(b" reads\r\n");
                            }
                            break;
                        }

                        kernel_debug! {
                            watos_arch::serial_write(b"[KERNEL] Extending buffer, current size=");
                            watos_arch::serial_hex(file_contents.len() as u64);
                            watos_arch::serial_write(b"\r\n");
                        }

                        file_contents.extend_from_slice(&read_buf[..chunk_read as usize]);

                        kernel_debug! {
                            watos_arch::serial_write(b"[KERNEL] Extended to ");
                            watos_arch::serial_hex(file_contents.len() as u64);
                            watos_arch::serial_write(b"\r\n");
                        }

                        // Safety limit: max 1MB per executable
                        if file_contents.len() >= 1024 * 1024 {
                            kernel_debug! {
                                watos_arch::serial_write(b"[KERNEL] Hit 1MB limit after ");
                                watos_arch::serial_hex(iteration);
                                watos_arch::serial_write(b" reads, size=");
                                watos_arch::serial_hex(file_contents.len() as u64);
                                watos_arch::serial_write(b"\r\n");
                            }
                            break;
                        }
                    }

                    kernel_debug! {
                        watos_arch::serial_write(b"[KERNEL] Read complete, total=");
                        watos_arch::serial_hex(file_contents.len() as u64);
                        watos_arch::serial_write(b"\r\n");
                    }

                    fd_close(fd as i64);

                    if !file_contents.is_empty() {
                        kernel_debug! {
                            watos_arch::serial_write(b"[KERNEL] Loaded ");
                            watos_arch::serial_hex(file_contents.len() as u64);
                            watos_arch::serial_write(b" bytes from ");
                            watos_arch::serial_write(path.as_bytes());
                            watos_arch::serial_write(b"\r\n");
                        }
                        app_data = Some(file_contents);
                        break;
                    }
                }
            }

            let result = if let Some(data) = app_data {
                // Save parent context with all register state
                let regs = unsafe { &SAVED_SYSCALL_REGS };
                watos_process::save_parent_context_with_frame(
                    return_rip,
                    return_rsp,
                    regs.rbx,
                    regs.rcx,
                    regs.rdx,
                    regs.rsi,
                    regs.rdi,
                    regs.rbp,
                    regs.r8,
                    regs.r9,
                    regs.r10,
                    regs.r11,
                    regs.r12,
                    regs.r13,
                    regs.r14,
                    regs.r15,
                );

                // Execute the app with the full command line as args
                let cmdline_str = core::str::from_utf8(cmdline_copy).unwrap_or("");

                match watos_process::exec(program_str, &data, cmdline_str) {
                    Ok(_pid) => 0, // Success
                    Err(e) => {
                        unsafe {
                            watos_arch::serial_write(b"[KERNEL] exec failed: ");
                            watos_arch::serial_write(e.as_bytes());
                            watos_arch::serial_write(b"\r\n");
                        }
                        1 // Error
                    }
                }
            } else {
                unsafe {
                    watos_arch::serial_write(b"[KERNEL] App not found: ");
                    watos_arch::serial_write(program_name);
                    watos_arch::serial_write(b"\r\n");
                }
                2 // Not found
            };

            // Restore user page table
            if kernel_pml4 != 0 && user_cr3 != kernel_pml4 {
                unsafe { watos_mem::paging::load_cr3(user_cr3); }
            }

            result
        }

        syscall::SYS_GETARGS => {
            // arg1 = buffer pointer
            // arg2 = buffer size
            // Returns: number of bytes copied
            unsafe {
                watos_arch::serial_write(b"[KERNEL] SYS_GETARGS: buf=0x");
                watos_arch::serial_hex(arg1);
                watos_arch::serial_write(b" size=");
                watos_arch::serial_hex(arg2);
                watos_arch::serial_write(b"\r\n");
            }

            let buf_ptr = arg1 as *mut u8;
            let buf_size = arg2 as usize;

            if buf_ptr.is_null() || buf_size == 0 {
                unsafe { watos_arch::serial_write(b"[KERNEL] SYS_GETARGS: invalid buffer\r\n"); }
                return 0;
            }

            let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr, buf_size) };
            let copied = watos_process::get_current_args(buf);
            unsafe {
                watos_arch::serial_write(b"[KERNEL] SYS_GETARGS: copied ");
                watos_arch::serial_hex(copied as u64);
                watos_arch::serial_write(b" bytes\r\n");
            }
            copied as u64
        }

        syscall::SYS_GETPID => {
            // Returns current process ID
            watos_process::current_pid().unwrap_or(0) as u64
        }

        syscall::SYS_GETDATE => {
            // Returns packed date: year << 16 | month << 8 | day
            watos_arch::rtc::get_packed_date() as u64
        }

        syscall::SYS_GETTIME => {
            // Returns packed time: hours << 16 | minutes << 8 | seconds
            watos_arch::rtc::get_packed_time() as u64
        }

        syscall::SYS_GETTICKS => {
            // Returns timer ticks since boot
            watos_arch::idt::get_ticks()
        }

        syscall::SYS_MEMINFO => {
            // arg1 = pointer to u64[5] buffer
            // Returns: 0 on success, 1 on error
            let buf_ptr = arg1 as *mut u64;
            if buf_ptr.is_null() {
                return 1;
            }

            unsafe {
                let phys_stats = watos_mem::pmm::stats();
                let heap_stats = watos_mem::heap::stats();

                *buf_ptr.offset(0) = phys_stats.total_bytes as u64;
                *buf_ptr.offset(1) = phys_stats.free_bytes as u64;
                *buf_ptr.offset(2) = (phys_stats.total_bytes - phys_stats.free_bytes) as u64;
                *buf_ptr.offset(3) = heap_stats.total as u64;
                *buf_ptr.offset(4) = heap_stats.used as u64;
            }
            0
        }

        syscall::SYS_MOUNT => {
            // arg1 = pointer to drive name
            // arg2 = length of drive name
            // arg3 = pointer to struct { mount_path_ptr, mount_path_len, fs_type_ptr, fs_type_len }
            // For simplicity, use packed args: arg3 = mount_path_ptr, we'll use fixed format
            // New format: arg1 = name_ptr, arg2 = name_len, arg3 = mount_path_ptr (null-term)
            let name_ptr = arg1 as *const u8;
            let name_len = arg2 as usize;
            let mount_ptr = arg3 as *const u8;

            if name_ptr.is_null() || name_len == 0 || name_len > MAX_DRIVE_NAME {
                return u64::MAX;
            }

            unsafe {
                let name = core::slice::from_raw_parts(name_ptr, name_len);

                // Find null-terminated mount path length
                let mut mount_len = 0usize;
                if !mount_ptr.is_null() {
                    while mount_len < 64 && *mount_ptr.add(mount_len) != 0 {
                        mount_len += 1;
                    }
                }
                let mount_path = if mount_len > 0 {
                    core::slice::from_raw_parts(mount_ptr, mount_len)
                } else {
                    b"/"
                };

                watos_arch::serial_write(b"[KERNEL] SYS_MOUNT: ");
                watos_arch::serial_write(name);
                watos_arch::serial_write(b" -> ");
                watos_arch::serial_write(mount_path);
                watos_arch::serial_write(b"\r\n");

                drive_mount(name, mount_path, b"WFS")
            }
        }

        syscall::SYS_UNMOUNT => {
            // arg1 = pointer to drive name
            // arg2 = length of drive name
            let name_ptr = arg1 as *const u8;
            let name_len = arg2 as usize;

            if name_ptr.is_null() || name_len == 0 {
                return u64::MAX;
            }

            unsafe {
                let name = core::slice::from_raw_parts(name_ptr, name_len);
                watos_arch::serial_write(b"[KERNEL] SYS_UNMOUNT: ");
                watos_arch::serial_write(name);
                watos_arch::serial_write(b"\r\n");
                drive_unmount(name)
            }
        }

        syscall::SYS_LISTDRIVES => {
            // arg1 = buffer pointer
            // arg2 = buffer size
            // Returns bytes written
            let buf_ptr = arg1 as *mut u8;
            let buf_size = arg2 as usize;

            if buf_ptr.is_null() || buf_size == 0 {
                return 0;
            }

            unsafe {
                let buf = core::slice::from_raw_parts_mut(buf_ptr, buf_size);
                drive_list(buf) as u64
            }
        }

        syscall::SYS_CHDIR => {
            // arg1 = pointer to path
            // arg2 = length
            // Handles: "DRIVE:", "\", "..", "dirname", "\path"
            let path_ptr = arg1 as *const u8;
            let path_len = arg2 as usize;

            if path_ptr.is_null() || path_len == 0 {
                return u64::MAX;
            }

            unsafe {
                let path = core::slice::from_raw_parts(path_ptr, path_len);
                watos_arch::serial_write(b"[KERNEL] SYS_CHDIR: ");
                watos_arch::serial_write(path);
                watos_arch::serial_write(b"\r\n");
                change_dir(path)
            }
        }

        syscall::SYS_READDIR => {
            // arg1 = path pointer (null = current directory)
            // arg2 = path length (0 = current directory)
            // arg3 = buffer pointer for output
            // Returns bytes written: entries as "TYPE NAME SIZE\n"
            // TYPE: D=directory, F=file
            let path_ptr = arg1 as *const u8;
            let path_len = arg2 as usize;
            let buf_ptr = arg3 as *mut u8;

            unsafe {
                watos_arch::serial_write(b"[KERNEL] SYS_READDIR: path_ptr=0x");
                watos_arch::serial_hex(arg1);
                watos_arch::serial_write(b" path_len=");
                watos_arch::serial_hex(arg2);
                watos_arch::serial_write(b"\r\n");
            }

            if buf_ptr.is_null() {
                return 0;
            }

            unsafe {
                let buf_size = 4096usize;
                let buf = core::slice::from_raw_parts_mut(buf_ptr, buf_size);

                // Get path (construct full path with drive letter if not specified)
                static mut FULL_PATH_BUF: [u8; 260] = [0u8; 260];
                let path_bytes: &[u8] = if path_ptr.is_null() || path_len == 0 {
                    // Construct full path: "X:\path"
                    let drive = &DRIVE_TABLE[CURRENT_DRIVE];
                    let name_len = drive.name_len.min(MAX_DRIVE_NAME);
                    let mut pos = 0;

                    // Drive letter (e.g., "C")
                    FULL_PATH_BUF[pos..pos + name_len].copy_from_slice(&drive.name[..name_len]);
                    pos += name_len;

                    // Colon
                    FULL_PATH_BUF[pos] = b':';
                    pos += 1;

                    // Current directory path
                    FULL_PATH_BUF[pos..pos + CURRENT_DIR_LEN].copy_from_slice(&CURRENT_DIR[..CURRENT_DIR_LEN]);
                    pos += CURRENT_DIR_LEN;

                    &FULL_PATH_BUF[..pos]
                } else {
                    core::slice::from_raw_parts(path_ptr, path_len)
                };

                // Convert to string for VFS
                let path_str = match core::str::from_utf8(path_bytes) {
                    Ok(s) => s,
                    Err(_) => return 0,
                };

                watos_arch::serial_write(b"[KERNEL] SYS_READDIR path: ");
                watos_arch::serial_write(path_bytes);
                watos_arch::serial_write(b"\r\n");

                // Switch to kernel page table for disk access
                let user_cr3 = watos_mem::paging::get_cr3();
                let kernel_pml4 = watos_process::get_kernel_pml4();

                if kernel_pml4 != 0 && user_cr3 != kernel_pml4 {
                    watos_mem::paging::load_cr3(kernel_pml4);
                }

                // Use VFS to read directory (with kernel page table)
                let vfs_result = watos_vfs::readdir(path_str);

                // Restore user page table before writing to user buffer
                if kernel_pml4 != 0 && user_cr3 != kernel_pml4 {
                    watos_mem::paging::load_cr3(user_cr3);
                }

                match vfs_result {
                    Ok(entries) => {
                        watos_arch::serial_write(b"[KERNEL] VFS readdir returned ");
                        watos_arch::serial_hex(entries.len() as u64);
                        watos_arch::serial_write(b" entries\r\n");

                        let mut pos = 0;
                        for entry in entries {
                            // Format: "TYPE NAME SIZE\n"
                            // TYPE: D=directory, F=file, L=symlink, etc.
                            let type_char = match entry.file_type {
                                watos_vfs::FileType::Directory => b'D',
                                watos_vfs::FileType::Regular => b'F',
                                watos_vfs::FileType::Symlink => b'L',
                                watos_vfs::FileType::CharDevice => b'C',
                                watos_vfs::FileType::BlockDevice => b'B',
                                watos_vfs::FileType::Fifo => b'P',
                                watos_vfs::FileType::Socket => b'S',
                                watos_vfs::FileType::Unknown => b'?',
                            };

                            let name_bytes = entry.name.as_bytes();
                            let size_str = format_num(entry.size);
                            let size_bytes = size_str.as_bytes();

                            // Check buffer space
                            let needed = 1 + 1 + name_bytes.len() + 1 + size_bytes.len() + 1;
                            if pos + needed > buf_size {
                                break;
                            }

                            buf[pos] = type_char;
                            pos += 1;
                            buf[pos] = b' ';
                            pos += 1;
                            buf[pos..pos + name_bytes.len()].copy_from_slice(name_bytes);
                            pos += name_bytes.len();
                            buf[pos] = b' ';
                            pos += 1;
                            buf[pos..pos + size_bytes.len()].copy_from_slice(size_bytes);
                            pos += size_bytes.len();
                            buf[pos] = b'\n';
                            pos += 1;
                        }
                        pos as u64
                    }
                    Err(e) => {
                        watos_arch::serial_write(b"[KERNEL] VFS readdir error: ");
                        let err_msg: &[u8] = match e {
                            VfsError::NotFound => b"NotFound",
                            VfsError::PermissionDenied => b"PermissionDenied",
                            VfsError::NotADirectory => b"NotADirectory",
                            VfsError::NotAFile => b"NotAFile",
                            VfsError::AlreadyExists => b"AlreadyExists",
                            VfsError::DirectoryNotEmpty => b"DirectoryNotEmpty",
                            VfsError::IoError => b"IoError",
                            VfsError::InvalidPath => b"InvalidPath",
                            VfsError::TooManyOpenFiles => b"TooManyOpenFiles",
                            VfsError::NotInitialized => b"NotInitialized",
                            VfsError::NotSupported => b"NotSupported",
                            VfsError::NoSpace => b"NoSpace",
                            VfsError::ReadOnly => b"ReadOnly",
                            VfsError::IsADirectory => b"IsADirectory",
                            VfsError::InvalidArgument => b"InvalidArgument",
                            VfsError::CrossDevice => b"CrossDevice",
                            VfsError::NameTooLong => b"NameTooLong",
                            VfsError::PathTooLong => b"PathTooLong",
                            VfsError::NotMounted => b"NotMounted",
                            VfsError::AlreadyMounted => b"AlreadyMounted",
                            VfsError::Busy => b"Busy",
                            VfsError::InvalidName => b"InvalidName",
                            VfsError::Corrupted => b"Corrupted",
                            VfsError::FsError(_) => b"FsError",
                        };
                        watos_arch::serial_write(err_msg);
                        watos_arch::serial_write(b"\r\n");
                        0
                    }
                }
            }
        }

        syscall::SYS_MKDIR => {
            // arg1 = path pointer
            // arg2 = path length
            // Returns 0 on success, error code on failure
            let path_ptr = arg1 as *const u8;
            let path_len = arg2 as usize;

            if path_ptr.is_null() || path_len == 0 {
                return u64::MAX;
            }

            unsafe {
                let path = core::slice::from_raw_parts(path_ptr, path_len);
                watos_arch::serial_write(b"[KERNEL] SYS_MKDIR: ");
                watos_arch::serial_write(path);
                watos_arch::serial_write(b"\r\n");

                // Try WFS first
                if WFS_SUPERBLOCK.is_some() {
                    if wfs_mkdir(path) {
                        return 0;
                    }
                    watos_arch::serial_write(b"[KERNEL] WFS mkdir failed\r\n");
                    return 1; // Error
                }

                // No WFS - fail
                watos_arch::serial_write(b"[KERNEL] No persistent storage available\r\n");
            }
            1 // Error - no filesystem
        }

        syscall::SYS_STAT => {
            // arg1 = path pointer
            // arg2 = path length
            // arg3 = stat buffer pointer (returns: type, size)
            // Returns 0 on success, error on not found
            let path_ptr = arg1 as *const u8;
            let path_len = arg2 as usize;
            let stat_ptr = arg3 as *mut u64;

            if path_ptr.is_null() || path_len == 0 || stat_ptr.is_null() {
                return u64::MAX;
            }

            unsafe {
                let path = core::slice::from_raw_parts(path_ptr, path_len);

                // Check for directories
                if path == b"." || path == b".." || path == b"\\" || path == b"/" ||
                   path == b"SYSTEM" || path == b"apps" {
                    // Directory: type=1, size=0
                    *stat_ptr = 1; // type: directory
                    *stat_ptr.add(1) = 0; // size
                    return 0;
                }

                // Check preloaded apps
                if let Some(info) = BOOT_INFO.as_ref() {
                    for i in 0..(info.app_count as usize) {
                        let app = &info.apps[i];
                        let name_len = app.name.iter().position(|&c| c == 0).unwrap_or(32);
                        let name = &app.name[..name_len];

                        // Case-insensitive compare
                        if path.len() == name.len() {
                            let matches = path.iter().zip(name.iter()).all(|(a, b)| {
                                let a_upper = if *a >= b'a' && *a <= b'z' { *a - 32 } else { *a };
                                let b_upper = if *b >= b'a' && *b <= b'z' { *b - 32 } else { *b };
                                a_upper == b_upper
                            });
                            if matches {
                                // File: type=0, size=app.size
                                *stat_ptr = 0; // type: file
                                *stat_ptr.add(1) = app.size;
                                return 0;
                            }
                        }
                    }
                }

                // Not found
                1
            }
        }

        syscall::SYS_GETCWD => {
            // arg1 = buffer pointer
            // arg2 = buffer size
            // Returns the current drive name + path (e.g., "C:\path")
            let buf_ptr = arg1 as *mut u8;
            let buf_size = arg2 as usize;

            if buf_ptr.is_null() || buf_size == 0 {
                return 0;
            }

            unsafe {
                let buf = core::slice::from_raw_parts_mut(buf_ptr, buf_size);
                get_cwd(buf) as u64
            }
        }

        // VGA Graphics Syscalls
        syscall::SYS_VGA_SET_MODE => {
            // arg1 = mode number (not used directly, set via session or use default)
            // For now, just return success
            0
        }

        syscall::SYS_VGA_SET_PIXEL => {
            // arg1 = x, arg2 = y, arg3 = color
            let x = arg1 as u32;
            let y = arg2 as u32;
            let color = arg3 as u32;
            watos_driver_video::set_pixel(x, y, color);
            0
        }

        syscall::SYS_VGA_GET_PIXEL => {
            // arg1 = x, arg2 = y
            let x = arg1 as u32;
            let y = arg2 as u32;
            watos_driver_video::get_pixel(x, y) as u64
        }

        syscall::SYS_VGA_BLIT => {
            // arg1 = buffer pointer, arg2 = width, arg3 = height
            let buf_ptr = arg1 as *const u8;
            let width = arg2 as usize;
            let height = arg3 as usize;
            let stride = width; // stride in pixels = width for packed buffer

            if buf_ptr.is_null() || width == 0 || height == 0 {
                return u64::MAX;
            }

            // Get session's bpp for proper buffer size calculation
            let bytes_per_pixel = if let Some(session_id) = watos_driver_video::get_active_session() {
                watos_driver_video::get_session_info(session_id)
                    .map(|m| ((m.bpp as usize + 7) / 8))
                    .unwrap_or(1)
            } else {
                1 // Default to 8-bit
            };

            let buf_size = stride * height * bytes_per_pixel;

            // Use SMAP bypass to access user buffer
            watos_mem::with_user_access(|| {
                unsafe {
                    let data = core::slice::from_raw_parts(buf_ptr, buf_size);
                    // Blit to active session if exists, otherwise to physical framebuffer
                    if let Some(session_id) = watos_driver_video::get_active_session() {
                        watos_driver_video::session_blit(session_id, data, width, height, stride);
                    } else {
                        // Direct blit to physical framebuffer
                        for y in 0..height {
                            for x in 0..width {
                                let offset = y * stride + x;
                                if offset < data.len() {
                                    let color = data[offset] as u32;
                                    watos_driver_video::set_pixel(x as u32, y as u32, color);
                                }
                            }
                        }
                    }
                }
            });
            0
        }

        syscall::SYS_VGA_CLEAR => {
            // arg1 = color (8-bit indexed or low byte of RGB)
            let color = arg1 as u32;
            // Clear the active session's virtual framebuffer, not physical
            if let Some(session_id) = watos_driver_video::get_active_session() {
                watos_driver_video::session_clear(session_id, color);
            } else {
                watos_driver_video::clear(color);
            }
            0
        }

        syscall::SYS_VGA_FLIP => {
            // Composite active session to display
            if let Some(session_id) = watos_driver_video::get_active_session() {
                watos_driver_video::session_flip(session_id);
            }
            0
        }

        syscall::SYS_VGA_SET_PALETTE => {
            // arg1 = index, arg2 = r, arg3 = g, arg4 (r10) = b
            let index = (arg1 & 0xFF) as u8;
            let r = (arg2 & 0xFF) as u8;
            let g = (arg3 & 0xFF) as u8;
            // For now, assume b is in upper bits of arg3 or separate arg
            let b = ((arg3 >> 8) & 0xFF) as u8;
            
            match watos_driver_video::set_palette(index, r, g, b) {
                Ok(_) => 0,
                Err(_) => u64::MAX,
            }
        }

        // VGA Session Management Syscalls
        syscall::SYS_VGA_CREATE_SESSION => {
            // arg1 = width, arg2 = height, arg3 = bpp
            let width = arg1 as u32;
            let height = arg2 as u32;
            let bpp = arg3 as u8;

            // Find matching mode or use default
            let mode = watos_driver_video::get_current_mode()
                .unwrap_or(watos_driver_video::modes::SVGA_800X600X32);

            match watos_driver_video::create_session(mode) {
                Some(session_id) => session_id as u64,
                None => u64::MAX,
            }
        }

        syscall::SYS_VGA_DESTROY_SESSION => {
            // arg1 = session_id
            let session_id = arg1 as u32;
            if watos_driver_video::destroy_session(session_id) {
                0
            } else {
                u64::MAX
            }
        }

        syscall::SYS_VGA_SET_ACTIVE_SESSION => {
            // arg1 = session_id
            let session_id = arg1 as u32;
            if watos_driver_video::set_active_session(session_id) {
                0
            } else {
                u64::MAX
            }
        }

        syscall::SYS_VGA_GET_SESSION_INFO => {
            // arg1 = session_id, arg2 = buffer pointer for VideoMode struct
            let session_id = arg1 as u32;
            let buf_ptr = arg2 as *mut u32;

            if buf_ptr.is_null() {
                return u64::MAX;
            }

            if let Some(mode) = watos_driver_video::get_session_info(session_id) {
                unsafe {
                    // Write VideoMode: width, height, bpp, format
                    *buf_ptr = mode.width;
                    *buf_ptr.add(1) = mode.height;
                    *buf_ptr.add(2) = mode.bpp as u32;
                    *buf_ptr.add(3) = mode.format as u32;
                }
                0
            } else {
                u64::MAX
            }
        }

        syscall::SYS_VGA_ENUMERATE_MODES => {
            // arg1 = buffer pointer, arg2 = buffer size (in VideoMode structs)
            // Returns number of modes written
            let buf_ptr = arg1 as *mut u32;
            let max_modes = arg2 as usize;

            if buf_ptr.is_null() || max_modes == 0 {
                return 0;
            }

            let modes = watos_driver_video::get_available_modes();
            let count = modes.len().min(max_modes);

            unsafe {
                for (i, mode) in modes.iter().take(count).enumerate() {
                    let offset = i * 4; // 4 u32s per VideoMode
                    *buf_ptr.add(offset) = mode.width;
                    *buf_ptr.add(offset + 1) = mode.height;
                    *buf_ptr.add(offset + 2) = mode.bpp as u32;
                    *buf_ptr.add(offset + 3) = mode.format as u32;
                }
            }

            count as u64
        }

        syscall::SYS_AUTHENTICATE => {
            // arg1 = username pointer
            // arg2 = username length
            // arg3 = password pointer
            // Returns UID on success, u64::MAX on failure
            // Note: Password length is determined by null terminator (max 64 bytes)
            let username_ptr = arg1 as *const u8;
            let username_len = arg2 as usize;
            let password_ptr = arg3 as *const u8;

            if username_ptr.is_null() || username_len == 0 || password_ptr.is_null() {
                return u64::MAX;
            }

            unsafe {
                let username = core::slice::from_raw_parts(username_ptr, username_len);
                
                // Safely find password length (null-terminated, max 64 bytes)
                let mut password_len = 0usize;
                for i in 0..64 {
                    if *password_ptr.add(i) == 0 {
                        break;
                    }
                    password_len += 1;
                }
                
                // Validate we found a null terminator
                if password_len == 64 && *password_ptr.add(63) != 0 {
                    watos_arch::serial_write(b"[KERNEL] Password too long or not null-terminated\r\n");
                    return u64::MAX;
                }
                
                let password = core::slice::from_raw_parts(password_ptr, password_len);

                watos_arch::serial_write(b"[KERNEL] SYS_AUTHENTICATE: user=");
                watos_arch::serial_write(username);
                watos_arch::serial_write(b"\r\n");

                // Authenticate via users subsystem
                match watos_users::authenticate(username, password) {
                    Some(uid) => {
                        watos_arch::serial_write(b"[KERNEL] Authentication successful, UID=");
                        watos_arch::serial_hex(uid as u64);
                        watos_arch::serial_write(b"\r\n");
                        uid as u64
                    }
                    None => {
                        watos_arch::serial_write(b"[KERNEL] Authentication failed\r\n");
                        u64::MAX
                    }
                }
            }
        }

        syscall::SYS_SETUID => {
            // arg1 = UID to set
            let uid = arg1 as u32;
            
            unsafe {
                watos_arch::serial_write(b"[KERNEL] SYS_SETUID: ");
                watos_arch::serial_hex(uid as u64);
                watos_arch::serial_write(b"\r\n");
            }

            if watos_process::set_current_uid(uid) {
                0
            } else {
                u64::MAX
            }
        }

        syscall::SYS_GETUID => {
            // Returns current process UID
            watos_process::get_current_uid() as u64
        }

        syscall::SYS_GETGID => {
            // Returns current process GID
            watos_process::get_current_gid() as u64
        }

        syscall::SYS_SETGID => {
            // arg1 = GID to set
            let gid = arg1 as u32;

            if watos_process::set_current_gid(gid) {
                0
            } else {
                u64::MAX
            }
        }

        syscall::SYS_GETEUID => {
            // Returns effective UID (same as UID for now - no setuid support)
            watos_process::get_current_uid() as u64
        }

        syscall::SYS_GETEGID => {
            // Returns effective GID (same as GID for now - no setgid support)
            watos_process::get_current_gid() as u64
        }

        syscall::SYS_CHMOD => {
            // arg1 = path pointer
            // arg2 = path length
            // arg3 = mode
            // Returns 0 on success, u64::MAX on error
            let path_ptr = arg1 as *const u8;
            let path_len = arg2 as usize;
            let mode = arg3 as u32;

            if path_ptr.is_null() || path_len == 0 || path_len > 256 {
                return u64::MAX;
            }

            // Copy path from user memory
            let mut path_buf = [0u8; 256];
            unsafe {
                core::ptr::copy_nonoverlapping(path_ptr, path_buf.as_mut_ptr(), path_len);
            }

            let path_str = match core::str::from_utf8(&path_buf[..path_len]) {
                Ok(s) => s,
                Err(_) => return u64::MAX,
            };

            // Switch to kernel page table for disk access
            let user_cr3 = watos_mem::paging::get_cr3();
            let kernel_pml4 = watos_process::get_kernel_pml4();

            if kernel_pml4 != 0 && user_cr3 != kernel_pml4 {
                unsafe { watos_mem::paging::load_cr3(kernel_pml4); }
            }

            let result = watos_vfs::chmod(path_str, mode);

            // Restore user page table
            if kernel_pml4 != 0 && user_cr3 != kernel_pml4 {
                unsafe { watos_mem::paging::load_cr3(user_cr3); }
            }

            match result {
                Ok(()) => 0,
                Err(_) => u64::MAX,
            }
        }

        syscall::SYS_CHOWN => {
            // arg1 = path pointer
            // arg2 = path length
            // arg3 = uid
            // r10 = gid
            // Returns 0 on success, u64::MAX on error
            let path_ptr = arg1 as *const u8;
            let path_len = arg2 as usize;
            let uid = arg3 as u32;
            let gid = unsafe { SAVED_SYSCALL_REGS.r10 as u32 };

            if path_ptr.is_null() || path_len == 0 || path_len > 256 {
                return u64::MAX;
            }

            // Copy path from user memory
            let mut path_buf = [0u8; 256];
            unsafe {
                core::ptr::copy_nonoverlapping(path_ptr, path_buf.as_mut_ptr(), path_len);
            }

            let path_str = match core::str::from_utf8(&path_buf[..path_len]) {
                Ok(s) => s,
                Err(_) => return u64::MAX,
            };

            // Switch to kernel page table for disk access
            let user_cr3 = watos_mem::paging::get_cr3();
            let kernel_pml4 = watos_process::get_kernel_pml4();

            if kernel_pml4 != 0 && user_cr3 != kernel_pml4 {
                unsafe { watos_mem::paging::load_cr3(kernel_pml4); }
            }

            let result = watos_vfs::chown(path_str, uid, gid);

            // Restore user page table
            if kernel_pml4 != 0 && user_cr3 != kernel_pml4 {
                unsafe { watos_mem::paging::load_cr3(user_cr3); }
            }

            match result {
                Ok(()) => 0,
                Err(_) => u64::MAX,
            }
        }

        syscall::SYS_ACCESS => {
            // arg1 = path pointer
            // arg2 = path length
            // arg3 = access mode (F_OK=0, R_OK=4, W_OK=2, X_OK=1)
            // Returns 0 if access allowed, u64::MAX if denied
            let path_ptr = arg1 as *const u8;
            let path_len = arg2 as usize;
            let _access_mode = arg3 as u32;

            if path_ptr.is_null() || path_len == 0 || path_len > 256 {
                return u64::MAX;
            }

            // Copy path from user memory
            let mut path_buf = [0u8; 256];
            unsafe {
                core::ptr::copy_nonoverlapping(path_ptr, path_buf.as_mut_ptr(), path_len);
            }

            let path_str = match core::str::from_utf8(&path_buf[..path_len]) {
                Ok(s) => s,
                Err(_) => return u64::MAX,
            };

            // Switch to kernel page table for disk access
            let user_cr3 = watos_mem::paging::get_cr3();
            let kernel_pml4 = watos_process::get_kernel_pml4();

            if kernel_pml4 != 0 && user_cr3 != kernel_pml4 {
                unsafe { watos_mem::paging::load_cr3(kernel_pml4); }
            }

            // For now, just check if path exists using stat
            // Full permission checking will be added in VFS layer
            let result = watos_vfs::stat(path_str);

            // Restore user page table
            if kernel_pml4 != 0 && user_cr3 != kernel_pml4 {
                unsafe { watos_mem::paging::load_cr3(user_cr3); }
            }

            match result {
                Ok(_) => 0, // File exists
                Err(_) => u64::MAX,
            }
        }

        syscall::SYS_SESSION_CREATE => {
            // arg1 = name pointer
            // arg2 = name length
            // arg3 = task_id
            // Returns session ID on success, u64::MAX on failure
            let name_ptr = arg1 as *const u8;
            let name_len = arg2 as usize;
            let task_id = arg3 as u32;

            if name_ptr.is_null() || name_len == 0 {
                return u64::MAX;
            }

            unsafe {
                let name_bytes = core::slice::from_raw_parts(name_ptr, name_len);
                let name_str = core::str::from_utf8(name_bytes).unwrap_or("session");
                
                match watos_console::manager().create_console(name_str, task_id) {
                    Some(id) => id as u64,
                    None => u64::MAX,
                }
            }
        }

        syscall::SYS_SESSION_SWITCH => {
            // arg1 = session ID (or F-key number 0-11)
            let session_id = arg1 as u8;
            
            let new_id = watos_console::manager().switch_to_fkey(session_id);
            new_id as u64
        }

        syscall::SYS_SESSION_GET_CURRENT => {
            // Returns current session ID
            watos_console::manager().active_id() as u64
        }

        syscall::SYS_SETENV => {
            // arg1 = key pointer, arg2 = key length, arg3 = value pointer, r10 = value length
            let key_ptr = arg1 as *const u8;
            let key_len = arg2 as usize;
            let val_ptr = arg3 as *const u8;
            let val_len = unsafe { SAVED_SYSCALL_REGS.r10 as usize };

            if key_ptr.is_null() || val_ptr.is_null() || key_len == 0 || key_len > 256 || val_len > 4096 {
                return 1; // Error
            }

            unsafe {
                let key_slice = core::slice::from_raw_parts(key_ptr, key_len);
                let val_slice = core::slice::from_raw_parts(val_ptr, val_len);

                if let (Ok(key), Ok(val)) = (core::str::from_utf8(key_slice), core::str::from_utf8(val_slice)) {
                    if watos_process::setenv(key, val) {
                        0 // Success
                    } else {
                        1 // Error
                    }
                } else {
                    1 // Invalid UTF-8
                }
            }
        }

        syscall::SYS_GETENV => {
            // arg1 = key pointer, arg2 = key length, arg3 = buffer pointer, r10 = buffer length
            // Returns actual length of value (0 if not found)
            let key_ptr = arg1 as *const u8;
            let key_len = arg2 as usize;
            let buf_ptr = arg3 as *mut u8;
            let buf_len = unsafe { SAVED_SYSCALL_REGS.r10 as usize };

            if key_ptr.is_null() || buf_ptr.is_null() || key_len == 0 || key_len > 256 {
                return 0; // Not found
            }

            unsafe {
                let key_slice = core::slice::from_raw_parts(key_ptr, key_len);

                if let Ok(key) = core::str::from_utf8(key_slice) {
                    if let Some(value) = watos_process::getenv(key) {
                        let value_bytes = value.as_bytes();
                        let copy_len = core::cmp::min(value_bytes.len(), buf_len);
                        core::ptr::copy_nonoverlapping(value_bytes.as_ptr(), buf_ptr, copy_len);
                        value_bytes.len() as u64 // Return actual length
                    } else {
                        0 // Not found
                    }
                } else {
                    0 // Invalid UTF-8
                }
            }
        }

        syscall::SYS_UNSETENV => {
            // arg1 = key pointer, arg2 = key length
            let key_ptr = arg1 as *const u8;
            let key_len = arg2 as usize;

            if key_ptr.is_null() || key_len == 0 || key_len > 256 {
                return 1; // Error
            }

            unsafe {
                let key_slice = core::slice::from_raw_parts(key_ptr, key_len);

                if let Ok(key) = core::str::from_utf8(key_slice) {
                    if watos_process::unsetenv(key) {
                        0 // Success
                    } else {
                        1 // Error
                    }
                } else {
                    1 // Invalid UTF-8
                }
            }
        }

        syscall::SYS_LISTENV => {
            // arg1 = buffer pointer, arg2 = buffer length
            // Returns number of variables
            // Format: null-separated strings "KEY=VALUE\0KEY2=VALUE2\0"
            let buf_ptr = arg1 as *mut u8;
            let buf_len = arg2 as usize;

            if buf_ptr.is_null() {
                // Just return count
                return watos_process::listenv().len() as u64;
            }

            unsafe {
                let env_list = watos_process::listenv();
                let mut offset = 0;

                for entry in env_list.iter() {
                    let entry_bytes = entry.as_bytes();
                    if offset + entry_bytes.len() + 1 > buf_len {
                        break; // Buffer full
                    }
                    core::ptr::copy_nonoverlapping(entry_bytes.as_ptr(), buf_ptr.add(offset), entry_bytes.len());
                    offset += entry_bytes.len();
                    *buf_ptr.add(offset) = 0; // Null terminator
                    offset += 1;
                }

                env_list.len() as u64
            }
        }

        syscall::SYS_SET_KEYMAP => {
            // arg1 = keymap data pointer, arg2 = data length
            // Keymap file format: KMAP + version + name + 3x256 byte maps
            let data_ptr = arg1 as *const u8;
            let data_len = arg2 as usize;

            if data_ptr.is_null() || data_len < 776 {
                return 1; // Error: invalid parameters
            }

            unsafe {
                let data = core::slice::from_raw_parts(data_ptr, data_len);

                // Parse and validate the keymap
                match watos_driver_keyboard::load_keymap(data) {
                    Ok(()) => {
                        watos_arch::serial_write(b"[KERNEL] Keymap loaded successfully\r\n");
                        0 // Success
                    }
                    Err(msg) => {
                        watos_arch::serial_write(b"[KERNEL] Keymap load failed: ");
                        watos_arch::serial_write(msg.as_bytes());
                        watos_arch::serial_write(b"\r\n");
                        2 // Error
                    }
                }
            }
        }

        syscall::SYS_SET_CODEPAGE => {
            // arg1 = codepage data pointer, arg2 = data length
            // Codepage file format: CPAG + version + ID + name + 256x4 byte map
            let data_ptr = arg1 as *const u8;
            let data_len = arg2 as usize;

            if data_ptr.is_null() || data_len < 1032 {
                return 1; // Error: invalid parameters
            }

            unsafe {
                let data = core::slice::from_raw_parts(data_ptr, data_len);

                // Parse and validate the codepage
                match watos_driver_keyboard::load_codepage(data) {
                    Ok(()) => {
                        watos_arch::serial_write(b"[KERNEL] Codepage loaded successfully\r\n");
                        0 // Success
                    }
                    Err(msg) => {
                        watos_arch::serial_write(b"[KERNEL] Codepage load failed: ");
                        watos_arch::serial_write(msg.as_bytes());
                        watos_arch::serial_write(b"\r\n");
                        2 // Error
                    }
                }
            }
        }

        syscall::SYS_GET_CODEPAGE => {
            // Returns current code page ID
            watos_driver_keyboard::get_codepage() as u64
        }

        syscall::SYS_PUTCHAR => {
            // arg1 = character to write
            let ch = (arg1 & 0xFF) as u8;
            let buf = [ch];

            // Write the same way as SYS_WRITE to stdout
            unsafe {
                watos_arch::serial_write(&buf);
            }
            watos_vt::vt_write_active(&buf);

            0
        }

        syscall::SYS_SLEEP => {
            // arg1 = milliseconds to sleep
            // For now, just do a simple delay loop
            let ms = arg1 as usize;
            for _ in 0..(ms * 1000) {
                core::hint::spin_loop();
            }
            0
        }

        // GFX Graphics Syscalls (for GWBASIC and other apps)
        syscall::SYS_GFX_PSET => {
            // arg1 = x, arg2 = y, arg3 = color
            let x = arg1 as u32;
            let y = arg2 as u32;
            let color = arg3 as u32;
            watos_driver_video::set_pixel(x, y, color);
            0
        }

        syscall::SYS_GFX_LINE => {
            // arg1 = x1, arg2 = y1, arg3 = x2, r10 = y2, r8 = color
            // Bresenham's line algorithm in kernel for performance
            let x1 = arg1 as i32;
            let y1 = arg2 as i32;
            let x2 = arg3 as i32;
            // r10 and r8 are passed but we need to access them via inline asm
            // For simplicity, pack y2 and color in a different way or use saved regs
            // Actually, the syscall handler saves r10/r8, let's access from SAVED_SYSCALL_REGS
            let (y2, color) = unsafe {
                let regs = &SAVED_SYSCALL_REGS;
                (regs.r10 as i32, regs.r8 as u32)
            };

            // Bresenham's line algorithm
            let mut x = x1;
            let mut y = y1;
            let dx = (x2 - x1).abs();
            let dy = -(y2 - y1).abs();
            let sx = if x1 < x2 { 1 } else { -1 };
            let sy = if y1 < y2 { 1 } else { -1 };
            let mut err = dx + dy;

            loop {
                watos_driver_video::set_pixel(x as u32, y as u32, color);
                if x == x2 && y == y2 { break; }
                let e2 = 2 * err;
                if e2 >= dy {
                    if x == x2 { break; }
                    err += dy;
                    x += sx;
                }
                if e2 <= dx {
                    if y == y2 { break; }
                    err += dx;
                    y += sy;
                }
            }
            0
        }

        syscall::SYS_GFX_CIRCLE => {
            // arg1 = cx, arg2 = cy, arg3 = radius, r10 = color
            let cx = arg1 as i32;
            let cy = arg2 as i32;
            let radius = arg3 as i32;
            let color = unsafe { SAVED_SYSCALL_REGS.r10 as u32 };

            // Midpoint circle algorithm
            let mut x = radius;
            let mut y = 0i32;
            let mut err = 0i32;

            while x >= y {
                watos_driver_video::set_pixel((cx + x) as u32, (cy + y) as u32, color);
                watos_driver_video::set_pixel((cx + y) as u32, (cy + x) as u32, color);
                watos_driver_video::set_pixel((cx - y) as u32, (cy + x) as u32, color);
                watos_driver_video::set_pixel((cx - x) as u32, (cy + y) as u32, color);
                watos_driver_video::set_pixel((cx - x) as u32, (cy - y) as u32, color);
                watos_driver_video::set_pixel((cx - y) as u32, (cy - x) as u32, color);
                watos_driver_video::set_pixel((cx + y) as u32, (cy - x) as u32, color);
                watos_driver_video::set_pixel((cx + x) as u32, (cy - y) as u32, color);

                y += 1;
                err += 1 + 2 * y;
                if 2 * (err - x) + 1 > 0 {
                    x -= 1;
                    err += 1 - 2 * x;
                }
            }
            0
        }

        syscall::SYS_GFX_CLS => {
            // Clear graphics screen to black
            watos_driver_video::clear(0);
            0
        }

        syscall::SYS_GFX_MODE => {
            // arg1 = mode number (BASIC SCREEN modes)
            // Mode 0 = text, 1 = 320x200, 2 = 640x200, 3 = 640x480, 4 = 800x600
            let mode = arg1 as u8;
            let video_mode = match mode {
                1 => watos_driver_video::modes::CGA_320X200X4,
                2 => watos_driver_video::modes::EGA_640X200X16,
                3 => watos_driver_video::modes::VGA_640X480X16,
                4 => watos_driver_video::modes::SVGA_800X600X32,
                _ => watos_driver_video::modes::TEXT_80X25,
            };
            match watos_driver_video::set_mode(video_mode) {
                Ok(_) => 0,
                Err(_) => u64::MAX,
            }
        }

        syscall::SYS_GFX_DISPLAY => {
            // Flip/display the graphics buffer
            if let Some(session_id) = watos_driver_video::get_active_session() {
                watos_driver_video::session_flip(session_id);
            }
            0
        }

        _ => {
            unsafe {
                watos_arch::serial_write(b"[SYSCALL] Unknown: ");
                watos_arch::serial_hex(num);
                watos_arch::serial_write(b"\r\n");
            }
            u64::MAX // Error
        }
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    unsafe {
        watos_arch::serial_write(b"\r\n!!! KERNEL PANIC !!!\r\n");
        if let Some(location) = info.location() {
            watos_arch::serial_write(b"  at ");
            watos_arch::serial_write(location.file().as_bytes());
            watos_arch::serial_write(b":");
            // Print line number
            let mut line = location.line();
            let mut buf = [0u8; 10];
            let mut i = 0;
            if line == 0 {
                buf[0] = b'0';
                i = 1;
            } else {
                while line > 0 {
                    buf[i] = b'0' + (line % 10) as u8;
                    line /= 10;
                    i += 1;
                }
            }
            for j in (0..i).rev() {
                watos_arch::serial_write(&buf[j..j+1]);
            }
            watos_arch::serial_write(b"\r\n");
        }
    }
    loop {
        watos_arch::halt();
    }
}
