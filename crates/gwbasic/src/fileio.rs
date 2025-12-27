//! File I/O module for GW-BASIC
//!
//! Provides file operations for both std (host) and no_std (WATOS) environments.

#[cfg(not(feature = "std"))]
use alloc::{string::String, format};
#[cfg(not(feature = "std"))]
use alloc::collections::BTreeMap as HashMap;

#[cfg(feature = "std")]
use std::collections::HashMap;
#[cfg(feature = "std")]
use std::fs::{File, OpenOptions};
#[cfg(feature = "std")]
use std::io::{BufRead, BufReader, BufWriter, Write};
#[cfg(feature = "std")]
use std::path::PathBuf;

use crate::error::{Error, Result};

/// File access modes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileMode {
    Input,
    Output,
    Append,
    Random,
}

impl FileMode {
    pub fn to_watos_mode(&self) -> u8 {
        match self {
            FileMode::Input => 0,
            FileMode::Output => 1,
            FileMode::Append => 2,
            FileMode::Random => 3,
        }
    }
}

/// File handle for std builds
#[cfg(feature = "std")]
pub struct FileHandle {
    _file: Option<File>,
    _mode: FileMode,
    _path: PathBuf,
    reader: Option<BufReader<File>>,
    writer: Option<BufWriter<File>>,
}

/// File handle for WATOS builds
#[cfg(not(feature = "std"))]
pub struct FileHandle {
    kernel_handle: u64,
    mode: FileMode,
    buffer: [u8; 256],
    buf_pos: usize,
    buf_len: usize,
    eof: bool,
}

/// File manager
pub struct FileManager {
    handles: HashMap<i32, FileHandle>,
    #[cfg(feature = "std")]
    next_handle: i32,
}

impl FileManager {
    pub fn new() -> Self {
        FileManager {
            handles: HashMap::new(),
            #[cfg(feature = "std")]
            next_handle: 1,
        }
    }

    #[cfg(feature = "std")]
    pub fn open(&mut self, file_num: i32, path: &str, mode: FileMode) -> Result<()> {
        if self.handles.contains_key(&file_num) {
            return Err(Error::RuntimeError(format!(
                "File #{} is already open",
                file_num
            )));
        }

        let file = match mode {
            FileMode::Input => File::open(path)
                .map_err(|e| Error::IoError(format!("Cannot open file: {}", e)))?,
            FileMode::Output => File::create(path)
                .map_err(|e| Error::IoError(format!("Cannot create file: {}", e)))?,
            FileMode::Append => OpenOptions::new()
                .append(true)
                .create(true)
                .open(path)
                .map_err(|e| Error::IoError(format!("Cannot open file for append: {}", e)))?,
            FileMode::Random => OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(path)
                .map_err(|e| Error::IoError(format!("Cannot open random file: {}", e)))?,
        };

        let reader = if mode == FileMode::Input {
            Some(BufReader::new(
                File::open(path)
                    .map_err(|e| Error::IoError(format!("Cannot open file: {}", e)))?,
            ))
        } else {
            None
        };

        let writer = if mode == FileMode::Output || mode == FileMode::Append {
            Some(BufWriter::new(
                if mode == FileMode::Output {
                    File::create(path)
                } else {
                    OpenOptions::new().append(true).open(path)
                }
                .map_err(|e| Error::IoError(format!("Cannot open file for writing: {}", e)))?,
            ))
        } else {
            None
        };

        self.handles.insert(
            file_num,
            FileHandle {
                _file: Some(file),
                _mode: mode,
                _path: PathBuf::from(path),
                reader,
                writer,
            },
        );

        Ok(())
    }

    #[cfg(not(feature = "std"))]
    pub fn open(&mut self, file_num: i32, path: &str, mode: FileMode) -> Result<()> {
        if self.handles.contains_key(&file_num) {
            return Err(Error::RuntimeError(format!(
                "File #{} is already open",
                file_num
            )));
        }

        // Call WATOS kernel to open file
        let path_bytes = path.as_bytes();
        let kernel_handle = unsafe {
            watos_file_open(
                path_bytes.as_ptr(),
                path_bytes.len(),
                mode.to_watos_mode(),
            )
        };

        if kernel_handle == u64::MAX {
            return Err(Error::IoError(format!("Cannot open file: {}", path)));
        }

        self.handles.insert(
            file_num,
            FileHandle {
                kernel_handle,
                mode,
                buffer: [0; 256],
                buf_pos: 0,
                buf_len: 0,
                eof: false,
            },
        );

        Ok(())
    }

    #[cfg(feature = "std")]
    pub fn close(&mut self, file_num: i32) -> Result<()> {
        if let Some(mut handle) = self.handles.remove(&file_num) {
            if let Some(ref mut writer) = handle.writer {
                writer
                    .flush()
                    .map_err(|e| Error::IoError(format!("Error flushing file: {}", e)))?;
            }
            Ok(())
        } else {
            Err(Error::RuntimeError(format!(
                "File #{} is not open",
                file_num
            )))
        }
    }

    #[cfg(not(feature = "std"))]
    pub fn close(&mut self, file_num: i32) -> Result<()> {
        if let Some(handle) = self.handles.remove(&file_num) {
            unsafe {
                watos_file_close(handle.kernel_handle);
            }
            Ok(())
        } else {
            Err(Error::RuntimeError(format!(
                "File #{} is not open",
                file_num
            )))
        }
    }

    pub fn close_all(&mut self) -> Result<()> {
        let file_nums: alloc::vec::Vec<i32> = self.handles.keys().copied().collect();
        for num in file_nums {
            self.close(num)?;
        }
        Ok(())
    }

    #[cfg(feature = "std")]
    pub fn write_line(&mut self, file_num: i32, data: &str) -> Result<()> {
        if let Some(handle) = self.handles.get_mut(&file_num) {
            if let Some(ref mut writer) = handle.writer {
                writeln!(writer, "{}", data)
                    .map_err(|e| Error::IoError(format!("Error writing to file: {}", e)))?;
                Ok(())
            } else {
                Err(Error::RuntimeError(format!(
                    "File #{} not open for writing",
                    file_num
                )))
            }
        } else {
            Err(Error::RuntimeError(format!(
                "File #{} is not open",
                file_num
            )))
        }
    }

    #[cfg(not(feature = "std"))]
    pub fn write_line(&mut self, file_num: i32, data: &str) -> Result<()> {
        if let Some(handle) = self.handles.get_mut(&file_num) {
            if handle.mode != FileMode::Output && handle.mode != FileMode::Append {
                return Err(Error::RuntimeError(format!(
                    "File #{} not open for writing",
                    file_num
                )));
            }

            let data_bytes = data.as_bytes();
            let written = unsafe {
                watos_file_write(handle.kernel_handle, data_bytes.as_ptr(), data_bytes.len())
            };

            if written == 0 && !data_bytes.is_empty() {
                return Err(Error::IoError("Error writing to file".into()));
            }

            // Write newline
            unsafe {
                watos_file_write(handle.kernel_handle, b"\n".as_ptr(), 1);
            }

            Ok(())
        } else {
            Err(Error::RuntimeError(format!(
                "File #{} is not open",
                file_num
            )))
        }
    }

    #[cfg(feature = "std")]
    pub fn read_line(&mut self, file_num: i32) -> Result<String> {
        if let Some(handle) = self.handles.get_mut(&file_num) {
            if let Some(ref mut reader) = handle.reader {
                let mut line = String::new();
                reader
                    .read_line(&mut line)
                    .map_err(|e| Error::IoError(format!("Error reading from file: {}", e)))?;
                Ok(line.trim_end().to_string())
            } else {
                Err(Error::RuntimeError(format!(
                    "File #{} not open for reading",
                    file_num
                )))
            }
        } else {
            Err(Error::RuntimeError(format!(
                "File #{} is not open",
                file_num
            )))
        }
    }

    #[cfg(not(feature = "std"))]
    pub fn read_line(&mut self, file_num: i32) -> Result<String> {
        if let Some(handle) = self.handles.get_mut(&file_num) {
            if handle.mode != FileMode::Input && handle.mode != FileMode::Random {
                return Err(Error::RuntimeError(format!(
                    "File #{} not open for reading",
                    file_num
                )));
            }

            if handle.eof {
                return Err(Error::IoError("End of file".into()));
            }

            // Read a line from file via kernel
            let mut line = String::new();
            loop {
                // Refill buffer if needed
                if handle.buf_pos >= handle.buf_len {
                    let bytes_read = unsafe {
                        watos_file_read(
                            handle.kernel_handle,
                            handle.buffer.as_mut_ptr(),
                            handle.buffer.len(),
                        )
                    };

                    if bytes_read == 0 {
                        handle.eof = true;
                        break;
                    }

                    handle.buf_pos = 0;
                    handle.buf_len = bytes_read;
                }

                // Process buffer looking for newline
                while handle.buf_pos < handle.buf_len {
                    let byte = handle.buffer[handle.buf_pos];
                    handle.buf_pos += 1;

                    if byte == b'\n' {
                        return Ok(line);
                    } else if byte != b'\r' {
                        line.push(byte as char);
                    }
                }
            }

            Ok(line)
        } else {
            Err(Error::RuntimeError(format!(
                "File #{} is not open",
                file_num
            )))
        }
    }

    pub fn eof(&self, file_num: i32) -> Result<bool> {
        if let Some(handle) = self.handles.get(&file_num) {
            #[cfg(not(feature = "std"))]
            {
                return Ok(handle.eof);
            }
            #[cfg(feature = "std")]
            {
                let _ = handle;
                Ok(false)
            }
        } else {
            Err(Error::RuntimeError(format!(
                "File #{} is not open",
                file_num
            )))
        }
    }

    pub fn loc(&self, file_num: i32) -> Result<i32> {
        if let Some(_handle) = self.handles.get(&file_num) {
            #[cfg(not(feature = "std"))]
            {
                let pos = unsafe { watos_file_tell(_handle.kernel_handle) };
                return Ok(pos as i32);
            }
            #[cfg(feature = "std")]
            Ok(0)
        } else {
            Err(Error::RuntimeError(format!(
                "File #{} is not open",
                file_num
            )))
        }
    }

    pub fn lof(&self, file_num: i32) -> Result<i32> {
        if let Some(_handle) = self.handles.get(&file_num) {
            #[cfg(not(feature = "std"))]
            {
                let size = unsafe { watos_file_size(_handle.kernel_handle) };
                return Ok(size as i32);
            }
            #[cfg(feature = "std")]
            Ok(0)
        } else {
            Err(Error::RuntimeError(format!(
                "File #{} is not open",
                file_num
            )))
        }
    }
}

impl Default for FileManager {
    fn default() -> Self {
        Self::new()
    }
}

// WATOS file syscalls
#[cfg(not(feature = "std"))]
extern "C" {
    fn watos_file_open(path: *const u8, path_len: usize, mode: u8) -> u64;
    fn watos_file_close(handle: u64);
    fn watos_file_read(handle: u64, buf: *mut u8, len: usize) -> usize;
    fn watos_file_write(handle: u64, buf: *const u8, len: usize) -> usize;
    fn watos_file_tell(handle: u64) -> u64;
    fn watos_file_size(handle: u64) -> u64;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_manager_creation() {
        let fm = FileManager::new();
        assert_eq!(fm.handles.len(), 0);
    }
}
