//! WATOS Process Filesystem (/proc)
//!
//! A virtual filesystem exposing process and kernel information.
//!
//! # Structure
//!
//! ```text
//! /proc/
//! ├── self            -> <current_pid>  (symlink)
//! ├── <pid>/
//! │   ├── status      process status
//! │   ├── cmdline     command line arguments
//! │   ├── cwd         current working directory (symlink)
//! │   └── fd/         open file descriptors
//! ├── cpuinfo         CPU information
//! ├── meminfo         memory information
//! ├── uptime          system uptime
//! └── mounts          mounted filesystems
//! ```
//!
//! # Usage
//!
//! ```ignore
//! let procfs = ProcFs::new();
//! procfs.set_process_provider(my_provider);
//! vfs.mount("/proc", Box::new(procfs));
//! ```

#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::vec;
use spin::Mutex;

use watos_vfs::{
    DirEntry, FileMode, FileOperations, FileStat, FileType, Filesystem, FsStats,
    SeekFrom, VfsError, VfsResult,
};

/// Process state for procfs
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProcState {
    Running,
    Sleeping,
    Stopped,
    Zombie,
}

/// Process information provided to procfs
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
    pub state: ProcState,
    pub cmdline: String,
    pub cwd: String,
    pub uid: u32,
    pub gid: u32,
    pub memory_kb: u64,
    pub cpu_time_ms: u64,
}

/// Trait for providing process information to procfs
pub trait ProcessProvider: Send + Sync {
    /// Get the current process ID
    fn current_pid(&self) -> Option<u32>;

    /// Get all process IDs
    fn list_pids(&self) -> Vec<u32>;

    /// Get info for a specific process
    fn get_process(&self, pid: u32) -> Option<ProcessInfo>;
}

/// System information provider
pub trait SystemProvider: Send + Sync {
    /// Get CPU info string
    fn cpu_info(&self) -> String;

    /// Get memory info string
    fn mem_info(&self) -> String;

    /// Get uptime in seconds
    fn uptime_secs(&self) -> u64;

    /// Get mount info string
    fn mounts_info(&self) -> String;
}

/// Default system provider with stub data
struct DefaultSystemProvider;

impl SystemProvider for DefaultSystemProvider {
    fn cpu_info(&self) -> String {
        String::from("processor\t: 0\nvendor_id\t: WATOS\nmodel name\t: Virtual CPU\n")
    }

    fn mem_info(&self) -> String {
        String::from("MemTotal:       4194304 kB\nMemFree:        2097152 kB\n")
    }

    fn uptime_secs(&self) -> u64 {
        0 // TODO: Get real uptime from timer
    }

    fn mounts_info(&self) -> String {
        String::from("devfs /dev devfs rw 0 0\nprocfs /proc procfs rw 0 0\n")
    }
}

/// Default process provider (no processes)
struct DefaultProcessProvider;

impl ProcessProvider for DefaultProcessProvider {
    fn current_pid(&self) -> Option<u32> {
        None
    }

    fn list_pids(&self) -> Vec<u32> {
        Vec::new()
    }

    fn get_process(&self, _pid: u32) -> Option<ProcessInfo> {
        None
    }
}

/// ProcFS - Process Filesystem
pub struct ProcFs {
    process_provider: Mutex<Box<dyn ProcessProvider>>,
    system_provider: Mutex<Box<dyn SystemProvider>>,
}

impl ProcFs {
    /// Create a new ProcFS with default providers
    pub fn new() -> Self {
        ProcFs {
            process_provider: Mutex::new(Box::new(DefaultProcessProvider)),
            system_provider: Mutex::new(Box::new(DefaultSystemProvider)),
        }
    }

    /// Set the process provider
    pub fn set_process_provider(&self, provider: Box<dyn ProcessProvider>) {
        *self.process_provider.lock() = provider;
    }

    /// Set the system provider
    pub fn set_system_provider(&self, provider: Box<dyn SystemProvider>) {
        *self.system_provider.lock() = provider;
    }

    /// Parse a path into components
    fn parse_path<'a>(&self, path: &'a str) -> Vec<&'a str> {
        path.trim_start_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// Check if a string is a valid PID
    fn parse_pid(s: &str) -> Option<u32> {
        s.parse().ok()
    }

    /// Get file content for a process file
    fn get_process_file_content(&self, pid: u32, file: &str) -> Option<String> {
        let provider = self.process_provider.lock();
        let info = provider.get_process(pid)?;

        match file {
            "status" => Some(format!(
                "Name:\t{}\n\
                 State:\t{:?}\n\
                 Pid:\t{}\n\
                 PPid:\t{}\n\
                 Uid:\t{}\n\
                 Gid:\t{}\n\
                 VmSize:\t{} kB\n",
                info.name, info.state, info.pid, info.ppid,
                info.uid, info.gid, info.memory_kb
            )),
            "cmdline" => Some(info.cmdline.clone()),
            "comm" => Some(format!("{}\n", info.name)),
            "cwd" => Some(info.cwd.clone()),
            _ => None,
        }
    }

    /// Get file content for a system file
    fn get_system_file_content(&self, file: &str) -> Option<String> {
        let provider = self.system_provider.lock();

        match file {
            "cpuinfo" => Some(provider.cpu_info()),
            "meminfo" => Some(provider.mem_info()),
            "uptime" => Some(format!("{}.00 0.00\n", provider.uptime_secs())),
            "mounts" => Some(provider.mounts_info()),
            "version" => Some(String::from("WATOS version 0.1.0\n")),
            _ => None,
        }
    }
}

impl Default for ProcFs {
    fn default() -> Self {
        Self::new()
    }
}

impl Filesystem for ProcFs {
    fn name(&self) -> &'static str {
        "procfs"
    }

    fn open(&self, path: &str, _mode: FileMode) -> VfsResult<Box<dyn FileOperations>> {
        let components = self.parse_path(path);

        if components.is_empty() {
            return Err(VfsError::IsADirectory);
        }

        // Handle /proc/self
        let components: Vec<&str> = if components[0] == "self" {
            let provider = self.process_provider.lock();
            if let Some(pid) = provider.current_pid() {
                let pid_str = alloc::format!("{}", pid);
                // Can't easily replace first component, so handle specially
                drop(provider);
                let new_path = path.replacen("self", &pid_str, 1);
                return self.open(&new_path, _mode);
            } else {
                return Err(VfsError::NotFound);
            }
        } else {
            components
        };

        // System files at /proc/xxx
        if components.len() == 1 {
            if let Some(content) = self.get_system_file_content(components[0]) {
                return Ok(Box::new(ProcFile::new(content)));
            }
        }

        // Process files at /proc/<pid>/xxx
        if let Some(pid) = Self::parse_pid(components[0]) {
            if components.len() == 1 {
                // /proc/<pid> is a directory
                return Err(VfsError::IsADirectory);
            }

            if let Some(content) = self.get_process_file_content(pid, components[1]) {
                return Ok(Box::new(ProcFile::new(content)));
            }
        }

        Err(VfsError::NotFound)
    }

    fn stat(&self, path: &str) -> VfsResult<FileStat> {
        let components = self.parse_path(path);

        if components.is_empty() {
            // Root directory
            return Ok(FileStat {
                file_type: FileType::Directory,
                size: 0,
                nlink: 2,
                inode: 1,
                dev: 0,
                mode: 0o555,
                uid: 0,
                gid: 0,
                blksize: 512,
                blocks: 0,
                atime: 0,
                mtime: 0,
                ctime: 0,
            });
        }

        // Handle /proc/self (symlink)
        if components[0] == "self" {
            return Ok(FileStat {
                file_type: FileType::Symlink,
                size: 0,
                nlink: 1,
                inode: 2,
                mode: 0o777,
                ..Default::default()
            });
        }

        // System files
        if components.len() == 1 {
            if self.get_system_file_content(components[0]).is_some() {
                return Ok(FileStat {
                    file_type: FileType::Regular,
                    size: 0,
                    nlink: 1,
                    inode: 100,
                    mode: 0o444,
                    ..Default::default()
                });
            }
        }

        // Process directories and files
        if let Some(pid) = Self::parse_pid(components[0]) {
            let provider = self.process_provider.lock();
            if provider.get_process(pid).is_some() {
                drop(provider);

                if components.len() == 1 {
                    // /proc/<pid> directory
                    return Ok(FileStat {
                        file_type: FileType::Directory,
                        size: 0,
                        nlink: 2,
                        inode: 1000 + pid as u64,
                        mode: 0o555,
                        ..Default::default()
                    });
                }

                if self.get_process_file_content(pid, components[1]).is_some() {
                    return Ok(FileStat {
                        file_type: FileType::Regular,
                        size: 0,
                        nlink: 1,
                        inode: 2000 + pid as u64,
                        mode: 0o444,
                        ..Default::default()
                    });
                }
            }
        }

        Err(VfsError::NotFound)
    }

    fn mkdir(&self, _path: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn unlink(&self, _path: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn rmdir(&self, _path: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn readdir(&self, path: &str) -> VfsResult<Vec<DirEntry>> {
        let components = self.parse_path(path);

        if components.is_empty() {
            // Root directory
            let mut entries = vec![
                DirEntry {
                    name: String::from("self"),
                    file_type: FileType::Symlink,
                    size: 0,
                    inode: 2,
                },
                DirEntry {
                    name: String::from("cpuinfo"),
                    file_type: FileType::Regular,
                    size: 0,
                    inode: 100,
                },
                DirEntry {
                    name: String::from("meminfo"),
                    file_type: FileType::Regular,
                    size: 0,
                    inode: 101,
                },
                DirEntry {
                    name: String::from("uptime"),
                    file_type: FileType::Regular,
                    size: 0,
                    inode: 102,
                },
                DirEntry {
                    name: String::from("mounts"),
                    file_type: FileType::Regular,
                    size: 0,
                    inode: 103,
                },
                DirEntry {
                    name: String::from("version"),
                    file_type: FileType::Regular,
                    size: 0,
                    inode: 104,
                },
            ];

            // Add process directories
            let provider = self.process_provider.lock();
            for pid in provider.list_pids() {
                entries.push(DirEntry {
                    name: format!("{}", pid),
                    file_type: FileType::Directory,
                    size: 0,
                    inode: 1000 + pid as u64,
                });
            }

            return Ok(entries);
        }

        // Process directory listing
        if let Some(pid) = Self::parse_pid(components[0]) {
            let provider = self.process_provider.lock();
            if provider.get_process(pid).is_some() {
                return Ok(vec![
                    DirEntry {
                        name: String::from("status"),
                        file_type: FileType::Regular,
                        size: 0,
                        inode: 2000 + pid as u64,
                    },
                    DirEntry {
                        name: String::from("cmdline"),
                        file_type: FileType::Regular,
                        size: 0,
                        inode: 2001 + pid as u64,
                    },
                    DirEntry {
                        name: String::from("comm"),
                        file_type: FileType::Regular,
                        size: 0,
                        inode: 2002 + pid as u64,
                    },
                    DirEntry {
                        name: String::from("cwd"),
                        file_type: FileType::Symlink,
                        size: 0,
                        inode: 2003 + pid as u64,
                    },
                ]);
            }
        }

        Err(VfsError::NotADirectory)
    }

    fn rename(&self, _old: &str, _new: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn sync(&self) -> VfsResult<()> {
        Ok(())
    }

    fn statfs(&self) -> VfsResult<FsStats> {
        Ok(FsStats {
            total_blocks: 0,
            free_blocks: 0,
            block_size: 0,
            total_inodes: 0,
            free_inodes: 0,
            max_name_len: 255,
        })
    }
}

/// A virtual file backed by a string
struct ProcFile {
    content: String,
    position: usize,
}

impl ProcFile {
    fn new(content: String) -> Self {
        ProcFile { content, position: 0 }
    }
}

impl FileOperations for ProcFile {
    fn read(&mut self, buffer: &mut [u8]) -> VfsResult<usize> {
        let bytes = self.content.as_bytes();
        if self.position >= bytes.len() {
            return Ok(0);
        }

        let remaining = &bytes[self.position..];
        let to_read = remaining.len().min(buffer.len());
        buffer[..to_read].copy_from_slice(&remaining[..to_read]);
        self.position += to_read;
        Ok(to_read)
    }

    fn write(&mut self, _buffer: &[u8]) -> VfsResult<usize> {
        Err(VfsError::ReadOnly)
    }

    fn seek(&mut self, offset: i64, whence: SeekFrom) -> VfsResult<u64> {
        let new_pos = match whence {
            SeekFrom::Start => offset as usize,
            SeekFrom::Current => (self.position as i64 + offset) as usize,
            SeekFrom::End => (self.content.len() as i64 + offset) as usize,
        };
        self.position = new_pos.min(self.content.len());
        Ok(self.position as u64)
    }

    fn tell(&self) -> u64 {
        self.position as u64
    }

    fn sync(&mut self) -> VfsResult<()> {
        Ok(())
    }

    fn stat(&self) -> VfsResult<FileStat> {
        Ok(FileStat {
            file_type: FileType::Regular,
            size: self.content.len() as u64,
            mode: 0o444,
            ..Default::default()
        })
    }

    fn truncate(&mut self, _size: u64) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }
}
