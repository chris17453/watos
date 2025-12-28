//! Standard device implementations

use alloc::boxed::Box;
use watos_vfs::{FileMode, FileOperations, FileStat, FileType, SeekFrom, VfsError, VfsResult};

use crate::Device;

// ============================================================================
// /dev/null - Discards all writes, reads return EOF
// ============================================================================

/// The null device - writes are discarded, reads return EOF
pub struct NullDevice;

impl Device for NullDevice {
    fn name(&self) -> &'static str {
        "null"
    }

    fn device_type(&self) -> FileType {
        FileType::CharDevice
    }

    fn major(&self) -> u32 {
        1
    }

    fn minor(&self) -> u32 {
        3
    }

    fn open(&self, _mode: FileMode) -> VfsResult<Box<dyn FileOperations>> {
        Ok(Box::new(NullFile))
    }
}

struct NullFile;

impl FileOperations for NullFile {
    fn read(&mut self, _buffer: &mut [u8]) -> VfsResult<usize> {
        Ok(0) // EOF
    }

    fn write(&mut self, buffer: &[u8]) -> VfsResult<usize> {
        Ok(buffer.len()) // Discard, pretend success
    }

    fn seek(&mut self, _offset: i64, _whence: SeekFrom) -> VfsResult<u64> {
        Ok(0)
    }

    fn tell(&self) -> u64 {
        0
    }

    fn sync(&mut self) -> VfsResult<()> {
        Ok(())
    }

    fn stat(&self) -> VfsResult<FileStat> {
        Ok(FileStat {
            file_type: FileType::CharDevice,
            size: 0,
            dev: (1 << 8) | 3,
            ..Default::default()
        })
    }

    fn truncate(&mut self, _size: u64) -> VfsResult<()> {
        Ok(())
    }
}

// ============================================================================
// /dev/zero - Writes are discarded, reads return zeros
// ============================================================================

/// The zero device - writes are discarded, reads return zeros
pub struct ZeroDevice;

impl Device for ZeroDevice {
    fn name(&self) -> &'static str {
        "zero"
    }

    fn device_type(&self) -> FileType {
        FileType::CharDevice
    }

    fn major(&self) -> u32 {
        1
    }

    fn minor(&self) -> u32 {
        5
    }

    fn open(&self, _mode: FileMode) -> VfsResult<Box<dyn FileOperations>> {
        Ok(Box::new(ZeroFile))
    }
}

struct ZeroFile;

impl FileOperations for ZeroFile {
    fn read(&mut self, buffer: &mut [u8]) -> VfsResult<usize> {
        for byte in buffer.iter_mut() {
            *byte = 0;
        }
        Ok(buffer.len())
    }

    fn write(&mut self, buffer: &[u8]) -> VfsResult<usize> {
        Ok(buffer.len()) // Discard, pretend success
    }

    fn seek(&mut self, _offset: i64, _whence: SeekFrom) -> VfsResult<u64> {
        Ok(0)
    }

    fn tell(&self) -> u64 {
        0
    }

    fn sync(&mut self) -> VfsResult<()> {
        Ok(())
    }

    fn stat(&self) -> VfsResult<FileStat> {
        Ok(FileStat {
            file_type: FileType::CharDevice,
            size: 0,
            dev: (1 << 8) | 5,
            ..Default::default()
        })
    }

    fn truncate(&mut self, _size: u64) -> VfsResult<()> {
        Ok(())
    }
}

// ============================================================================
// /dev/full - Writes fail with ENOSPC, reads return zeros
// ============================================================================

/// The full device - writes fail with ENOSPC, reads return zeros
pub struct FullDevice;

impl Device for FullDevice {
    fn name(&self) -> &'static str {
        "full"
    }

    fn device_type(&self) -> FileType {
        FileType::CharDevice
    }

    fn major(&self) -> u32 {
        1
    }

    fn minor(&self) -> u32 {
        7
    }

    fn open(&self, _mode: FileMode) -> VfsResult<Box<dyn FileOperations>> {
        Ok(Box::new(FullFile))
    }
}

struct FullFile;

impl FileOperations for FullFile {
    fn read(&mut self, buffer: &mut [u8]) -> VfsResult<usize> {
        for byte in buffer.iter_mut() {
            *byte = 0;
        }
        Ok(buffer.len())
    }

    fn write(&mut self, _buffer: &[u8]) -> VfsResult<usize> {
        Err(VfsError::NoSpace) // Always full
    }

    fn seek(&mut self, _offset: i64, _whence: SeekFrom) -> VfsResult<u64> {
        Ok(0)
    }

    fn tell(&self) -> u64 {
        0
    }

    fn sync(&mut self) -> VfsResult<()> {
        Ok(())
    }

    fn stat(&self) -> VfsResult<FileStat> {
        Ok(FileStat {
            file_type: FileType::CharDevice,
            size: 0,
            dev: (1 << 8) | 7,
            ..Default::default()
        })
    }

    fn truncate(&mut self, _size: u64) -> VfsResult<()> {
        Ok(())
    }
}

// ============================================================================
// /dev/random - Reads return pseudo-random bytes
// ============================================================================

/// The random device - reads return pseudo-random bytes
/// Uses a simple xorshift PRNG for speed
pub struct RandomDevice {
    /// PRNG state
    state: spin::Mutex<u64>,
}

impl RandomDevice {
    /// Create a new random device with a seed
    pub fn new() -> Self {
        // Use a fixed seed for now - in real implementation,
        // this should be seeded from hardware RNG or timing
        RandomDevice {
            state: spin::Mutex::new(0x853c49e6748fea9b),
        }
    }

    /// Create with a specific seed
    pub fn with_seed(seed: u64) -> Self {
        RandomDevice {
            state: spin::Mutex::new(if seed == 0 { 1 } else { seed }),
        }
    }

    /// Reseed the random device
    #[allow(dead_code)]
    pub fn reseed(&self, seed: u64) {
        let mut state = self.state.lock();
        *state = if seed == 0 { 1 } else { seed };
    }
}

impl Default for RandomDevice {
    fn default() -> Self {
        Self::new()
    }
}

impl Device for RandomDevice {
    fn name(&self) -> &'static str {
        "random"
    }

    fn device_type(&self) -> FileType {
        FileType::CharDevice
    }

    fn major(&self) -> u32 {
        1
    }

    fn minor(&self) -> u32 {
        8
    }

    fn open(&self, _mode: FileMode) -> VfsResult<Box<dyn FileOperations>> {
        Ok(Box::new(RandomFile {
            state: self.state.lock().clone(),
        }))
    }
}

struct RandomFile {
    state: u64,
}

impl RandomFile {
    fn next(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }
}

impl FileOperations for RandomFile {
    fn read(&mut self, buffer: &mut [u8]) -> VfsResult<usize> {
        let mut remaining = buffer.len();
        let mut offset = 0;

        while remaining > 0 {
            let rand = self.next();
            let bytes = rand.to_le_bytes();
            let to_copy = remaining.min(8);
            buffer[offset..offset + to_copy].copy_from_slice(&bytes[..to_copy]);
            offset += to_copy;
            remaining -= to_copy;
        }

        Ok(buffer.len())
    }

    fn write(&mut self, buffer: &[u8]) -> VfsResult<usize> {
        // Writing to /dev/random adds entropy (we just mix it into state)
        for chunk in buffer.chunks(8) {
            let mut bytes = [0u8; 8];
            bytes[..chunk.len()].copy_from_slice(chunk);
            let val = u64::from_le_bytes(bytes);
            self.state ^= val;
            self.next(); // Mix it
        }
        Ok(buffer.len())
    }

    fn seek(&mut self, _offset: i64, _whence: SeekFrom) -> VfsResult<u64> {
        Ok(0)
    }

    fn tell(&self) -> u64 {
        0
    }

    fn sync(&mut self) -> VfsResult<()> {
        Ok(())
    }

    fn stat(&self) -> VfsResult<FileStat> {
        Ok(FileStat {
            file_type: FileType::CharDevice,
            size: 0,
            dev: (1 << 8) | 8,
            ..Default::default()
        })
    }

    fn truncate(&mut self, _size: u64) -> VfsResult<()> {
        Ok(())
    }
}

// ============================================================================
// /dev/urandom - Same as random for our purposes
// ============================================================================

/// The urandom device - non-blocking random (same as random in our implementation)
pub struct URandomDevice(RandomDevice);

impl URandomDevice {
    pub fn new() -> Self {
        URandomDevice(RandomDevice::new())
    }
}

impl Default for URandomDevice {
    fn default() -> Self {
        Self::new()
    }
}

impl Device for URandomDevice {
    fn name(&self) -> &'static str {
        "urandom"
    }

    fn device_type(&self) -> FileType {
        FileType::CharDevice
    }

    fn major(&self) -> u32 {
        1
    }

    fn minor(&self) -> u32 {
        9
    }

    fn open(&self, mode: FileMode) -> VfsResult<Box<dyn FileOperations>> {
        self.0.open(mode)
    }
}
