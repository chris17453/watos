//! Pipe implementation for inter-process communication
//!
//! Supports both anonymous pipes (for `cmd1 | cmd2`) and named pipes (FIFOs).
//!
//! # Anonymous Pipes
//!
//! ```ignore
//! let (read_end, write_end) = create_pipe()?;
//! // write_end.write(b"hello");
//! // read_end.read(&mut buf);
//! ```
//!
//! # Named Pipes (FIFOs)
//!
//! Created via filesystem with `mkfifo` and opened like regular files.

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use spin::Mutex;

use crate::{FileOperations, FileStat, FileType, SeekFrom, VfsError, VfsResult};

/// Default pipe buffer size (64KB)
pub const PIPE_BUF_SIZE: usize = 65536;

/// Pipe buffer shared between read and write ends
pub struct PipeBuffer {
    /// The actual buffer
    data: VecDeque<u8>,
    /// Maximum buffer size
    capacity: usize,
    /// Is the write end closed?
    write_closed: bool,
    /// Is the read end closed?
    read_closed: bool,
}

impl PipeBuffer {
    /// Create a new pipe buffer with default capacity
    pub fn new() -> Self {
        Self::with_capacity(PIPE_BUF_SIZE)
    }

    /// Create a new pipe buffer with specified capacity
    pub fn with_capacity(capacity: usize) -> Self {
        PipeBuffer {
            data: VecDeque::with_capacity(capacity),
            capacity,
            write_closed: false,
            read_closed: false,
        }
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Check if buffer is full
    pub fn is_full(&self) -> bool {
        self.data.len() >= self.capacity
    }

    /// Available space for writing
    pub fn available_space(&self) -> usize {
        self.capacity.saturating_sub(self.data.len())
    }

    /// Available data for reading
    pub fn available_data(&self) -> usize {
        self.data.len()
    }
}

impl Default for PipeBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared pipe state
pub type SharedPipe = Arc<Mutex<PipeBuffer>>;

/// Create a new anonymous pipe
///
/// Returns (read_end, write_end) file operations
pub fn create_pipe() -> (Box<dyn FileOperations>, Box<dyn FileOperations>) {
    let buffer = Arc::new(Mutex::new(PipeBuffer::new()));

    let read_end = Box::new(PipeReadEnd {
        buffer: buffer.clone(),
    });

    let write_end = Box::new(PipeWriteEnd { buffer });

    (read_end, write_end)
}

/// Create a pipe with custom buffer size
pub fn create_pipe_with_capacity(capacity: usize) -> (Box<dyn FileOperations>, Box<dyn FileOperations>) {
    let buffer = Arc::new(Mutex::new(PipeBuffer::with_capacity(capacity)));

    let read_end = Box::new(PipeReadEnd {
        buffer: buffer.clone(),
    });

    let write_end = Box::new(PipeWriteEnd { buffer });

    (read_end, write_end)
}

/// Read end of a pipe
pub struct PipeReadEnd {
    buffer: SharedPipe,
}

impl FileOperations for PipeReadEnd {
    fn read(&mut self, buf: &mut [u8]) -> VfsResult<usize> {
        let mut pipe = self.buffer.lock();

        if pipe.is_empty() {
            if pipe.write_closed {
                // EOF - write end is closed and no more data
                return Ok(0);
            }
            // Would block - in a real OS we'd sleep here
            // For now, return 0 to indicate no data available
            // The caller should retry
            return Ok(0);
        }

        // Read available data
        let to_read = buf.len().min(pipe.available_data());
        for i in 0..to_read {
            buf[i] = pipe.data.pop_front().unwrap();
        }

        Ok(to_read)
    }

    fn write(&mut self, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::PermissionDenied) // Can't write to read end
    }

    fn seek(&mut self, _offset: i64, _whence: SeekFrom) -> VfsResult<u64> {
        Err(VfsError::InvalidArgument) // Pipes are not seekable
    }

    fn tell(&self) -> u64 {
        0
    }

    fn sync(&mut self) -> VfsResult<()> {
        Ok(())
    }

    fn stat(&self) -> VfsResult<FileStat> {
        let pipe = self.buffer.lock();
        Ok(FileStat {
            file_type: FileType::Fifo,
            size: pipe.available_data() as u64,
            mode: 0o444, // Read-only
            ..Default::default()
        })
    }

    fn truncate(&mut self, _size: u64) -> VfsResult<()> {
        Err(VfsError::InvalidArgument)
    }
}

impl Drop for PipeReadEnd {
    fn drop(&mut self) {
        let mut pipe = self.buffer.lock();
        pipe.read_closed = true;
    }
}

/// Write end of a pipe
pub struct PipeWriteEnd {
    buffer: SharedPipe,
}

impl FileOperations for PipeWriteEnd {
    fn read(&mut self, _buf: &mut [u8]) -> VfsResult<usize> {
        Err(VfsError::PermissionDenied) // Can't read from write end
    }

    fn write(&mut self, buf: &[u8]) -> VfsResult<usize> {
        let mut pipe = self.buffer.lock();

        if pipe.read_closed {
            // Broken pipe - no readers
            return Err(VfsError::IoError);
        }

        if pipe.is_full() {
            // Would block - in a real OS we'd sleep here
            // Return 0 to indicate buffer full, caller should retry
            return Ok(0);
        }

        // Write as much as possible
        let to_write = buf.len().min(pipe.available_space());
        for &byte in &buf[..to_write] {
            pipe.data.push_back(byte);
        }

        Ok(to_write)
    }

    fn seek(&mut self, _offset: i64, _whence: SeekFrom) -> VfsResult<u64> {
        Err(VfsError::InvalidArgument) // Pipes are not seekable
    }

    fn tell(&self) -> u64 {
        0
    }

    fn sync(&mut self) -> VfsResult<()> {
        Ok(())
    }

    fn stat(&self) -> VfsResult<FileStat> {
        let pipe = self.buffer.lock();
        Ok(FileStat {
            file_type: FileType::Fifo,
            size: pipe.available_space() as u64,
            mode: 0o222, // Write-only
            ..Default::default()
        })
    }

    fn truncate(&mut self, _size: u64) -> VfsResult<()> {
        Err(VfsError::InvalidArgument)
    }
}

impl Drop for PipeWriteEnd {
    fn drop(&mut self) {
        let mut pipe = self.buffer.lock();
        pipe.write_closed = true;
    }
}

/// Named pipe (FIFO) that can be created in a filesystem
///
/// Unlike anonymous pipes, named pipes:
/// - Have a name in the filesystem
/// - Can be opened by multiple processes
/// - Persist until explicitly deleted
pub struct NamedPipe {
    buffer: SharedPipe,
    /// Number of readers
    readers: Mutex<usize>,
    /// Number of writers
    writers: Mutex<usize>,
}

impl NamedPipe {
    /// Create a new named pipe
    pub fn new() -> Self {
        NamedPipe {
            buffer: Arc::new(Mutex::new(PipeBuffer::new())),
            readers: Mutex::new(0),
            writers: Mutex::new(0),
        }
    }

    /// Open the read end of this named pipe
    pub fn open_read(&self) -> Box<dyn FileOperations> {
        *self.readers.lock() += 1;
        {
            let mut pipe = self.buffer.lock();
            pipe.read_closed = false;
        }
        Box::new(NamedPipeReadEnd {
            buffer: self.buffer.clone(),
            readers: unsafe { &*(self as *const Self) },
        })
    }

    /// Open the write end of this named pipe
    pub fn open_write(&self) -> Box<dyn FileOperations> {
        *self.writers.lock() += 1;
        {
            let mut pipe = self.buffer.lock();
            pipe.write_closed = false;
        }
        Box::new(NamedPipeWriteEnd {
            buffer: self.buffer.clone(),
            writers: unsafe { &*(self as *const Self) },
        })
    }

    /// Get file stat for this named pipe
    pub fn stat(&self) -> FileStat {
        let pipe = self.buffer.lock();
        FileStat {
            file_type: FileType::Fifo,
            size: pipe.available_data() as u64,
            mode: 0o666,
            ..Default::default()
        }
    }
}

impl Default for NamedPipe {
    fn default() -> Self {
        Self::new()
    }
}

// Named pipe ends are similar to anonymous pipe ends but track reader/writer counts
struct NamedPipeReadEnd {
    buffer: SharedPipe,
    readers: *const NamedPipe,
}

unsafe impl Send for NamedPipeReadEnd {}
unsafe impl Sync for NamedPipeReadEnd {}

impl FileOperations for NamedPipeReadEnd {
    fn read(&mut self, buf: &mut [u8]) -> VfsResult<usize> {
        let mut pipe = self.buffer.lock();

        if pipe.is_empty() {
            if pipe.write_closed {
                return Ok(0); // EOF
            }
            return Ok(0); // Would block
        }

        let to_read = buf.len().min(pipe.available_data());
        for i in 0..to_read {
            buf[i] = pipe.data.pop_front().unwrap();
        }

        Ok(to_read)
    }

    fn write(&mut self, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::PermissionDenied)
    }

    fn seek(&mut self, _offset: i64, _whence: SeekFrom) -> VfsResult<u64> {
        Err(VfsError::InvalidArgument)
    }

    fn tell(&self) -> u64 {
        0
    }

    fn sync(&mut self) -> VfsResult<()> {
        Ok(())
    }

    fn stat(&self) -> VfsResult<FileStat> {
        let pipe = self.buffer.lock();
        Ok(FileStat {
            file_type: FileType::Fifo,
            size: pipe.available_data() as u64,
            mode: 0o444,
            ..Default::default()
        })
    }

    fn truncate(&mut self, _size: u64) -> VfsResult<()> {
        Err(VfsError::InvalidArgument)
    }
}

impl Drop for NamedPipeReadEnd {
    fn drop(&mut self) {
        unsafe {
            let named_pipe = &*self.readers;
            let mut count = named_pipe.readers.lock();
            *count -= 1;
            if *count == 0 {
                let mut pipe = self.buffer.lock();
                pipe.read_closed = true;
            }
        }
    }
}

struct NamedPipeWriteEnd {
    buffer: SharedPipe,
    writers: *const NamedPipe,
}

unsafe impl Send for NamedPipeWriteEnd {}
unsafe impl Sync for NamedPipeWriteEnd {}

impl FileOperations for NamedPipeWriteEnd {
    fn read(&mut self, _buf: &mut [u8]) -> VfsResult<usize> {
        Err(VfsError::PermissionDenied)
    }

    fn write(&mut self, buf: &[u8]) -> VfsResult<usize> {
        let mut pipe = self.buffer.lock();

        if pipe.read_closed {
            return Err(VfsError::IoError); // Broken pipe
        }

        if pipe.is_full() {
            return Ok(0); // Would block
        }

        let to_write = buf.len().min(pipe.available_space());
        for &byte in &buf[..to_write] {
            pipe.data.push_back(byte);
        }

        Ok(to_write)
    }

    fn seek(&mut self, _offset: i64, _whence: SeekFrom) -> VfsResult<u64> {
        Err(VfsError::InvalidArgument)
    }

    fn tell(&self) -> u64 {
        0
    }

    fn sync(&mut self) -> VfsResult<()> {
        Ok(())
    }

    fn stat(&self) -> VfsResult<FileStat> {
        let pipe = self.buffer.lock();
        Ok(FileStat {
            file_type: FileType::Fifo,
            size: pipe.available_space() as u64,
            mode: 0o222,
            ..Default::default()
        })
    }

    fn truncate(&mut self, _size: u64) -> VfsResult<()> {
        Err(VfsError::InvalidArgument)
    }
}

impl Drop for NamedPipeWriteEnd {
    fn drop(&mut self) {
        unsafe {
            let named_pipe = &*self.writers;
            let mut count = named_pipe.writers.lock();
            *count -= 1;
            if *count == 0 {
                let mut pipe = self.buffer.lock();
                pipe.write_closed = true;
            }
        }
    }
}
