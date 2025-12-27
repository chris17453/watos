//! WATOS Runtime Framework
//!
//! Provides traits and types for runtime execution environments.

#![no_std]

/// Binary format detection
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum BinaryFormat {
    /// WATOS native 64-bit format
    RuNative,
    /// 8-bit interpreted format
    EightBit,
    /// DOS COM file (flat binary, max 64KB)
    DosCom,
    /// DOS EXE file (MZ header, relocatable)
    DosExe,
    /// 64-bit ELF executable
    Elf64,
    /// Unknown format
    Unknown,
}

/// Result of attempting to run a binary
#[derive(Debug)]
pub enum RunResult {
    /// Successfully scheduled with task ID
    Scheduled(u32),
    /// Execution completed with exit code
    Completed(i32),
    /// Failed to run
    Failed,
}

/// Runtime execution environment trait
pub trait Runtime: Send + Sync {
    /// Get runtime name
    fn name(&self) -> &'static str;

    /// Check if this runtime can execute the given format
    fn can_run(&self, format: BinaryFormat) -> bool;

    /// Execute a binary
    ///
    /// # Arguments
    /// * `filename` - Name of the file being executed
    /// * `data` - Binary contents
    /// * `args` - Command line arguments
    ///
    /// # Returns
    /// Result of execution attempt
    fn run(&self, filename: &str, data: &[u8], args: &[&str]) -> RunResult;

    /// Poll for task completion (for async runtimes)
    fn poll(&self) {}
}

/// Detect binary format from file header
pub fn detect_format(data: &[u8]) -> BinaryFormat {
    // Check for ELF64 header (0x7F 'E' 'L' 'F')
    if data.len() >= 20 && data[0..4] == [0x7F, b'E', b'L', b'F'] {
        let class = data[4];
        let machine = u16::from_le_bytes([data[18], data[19]]);
        if class == 2 && machine == 0x3E {
            return BinaryFormat::Elf64;
        }
    }

    // Check for WATOS native formats
    if data.starts_with(b"RU64\x01") || data.starts_with(b"RUARM\x01") {
        return BinaryFormat::RuNative;
    }
    if data.starts_with(b"RU8\x01") {
        return BinaryFormat::EightBit;
    }

    // Check for DOS MZ header
    if data.len() >= 2 && &data[0..2] == b"MZ" {
        return BinaryFormat::DosExe;
    }

    // Small files without MZ header are assumed to be COM
    if data.len() <= 65536 {
        return BinaryFormat::DosCom;
    }

    BinaryFormat::Unknown
}
