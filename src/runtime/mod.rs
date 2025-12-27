extern crate alloc;
use alloc::{boxed::Box, vec::Vec, string::String};

pub mod dos16;

use dos16::Dos16Runtime;

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum BinaryFormat { RuNative, EightBit, DosCom, DosExe, Elf64, Unknown }

pub enum RunResult { Scheduled(u32), Failed }

pub trait Runtime {
    fn name(&self) -> &'static str;
    fn can_run(&self, format: BinaryFormat) -> bool;
    fn run(&self, filename: &str, data: &[u8]) -> RunResult;
}

static mut REGISTRY: Option<Vec<Box<dyn Runtime>>> = None;
static mut TASKS: Option<Vec<(u32, String)>> = None;

pub fn register_default_runtimes() {
    unsafe {
        if REGISTRY.is_none() {
            REGISTRY = Some(Vec::new());
        }
        if TASKS.is_none() {
            TASKS = Some(Vec::new());
        }
        if let Some(reg) = REGISTRY.as_mut() {
            reg.push(Box::new(Dos16Runtime::new()));
            // ELF64 binaries handled directly via process::exec with memory protection
        }
    }
}

pub fn detect_format(_filename: &str, data: &[u8]) -> BinaryFormat {
    // Check for ELF64 header (0x7F 'E' 'L' 'F')
    if data.len() >= 18 && data[0..4] == [0x7F, b'E', b'L', b'F'] {
        // Check class (64-bit = 2) and machine (x86-64 = 0x3E)
        let class = data[4];
        let machine = u16::from_le_bytes([data[18], data[19]]);
        if class == 2 && machine == 0x3E {
            return BinaryFormat::Elf64;
        }
    }
    if data.starts_with(b"RU64\x01") || data.starts_with(b"RUARM\x01") { return BinaryFormat::RuNative; }
    if data.starts_with(b"RU8\x01") { return BinaryFormat::EightBit; }
    if data.len() >= 2 && &data[0..2] == b"MZ" { return BinaryFormat::DosExe; }
    if data.len() <= 65536 { return BinaryFormat::DosCom; }
    BinaryFormat::Unknown
}

pub fn detect_and_run(filename: &str, data: &[u8]) -> RunResult {
    let fmt = detect_format(filename, data);

    // Handle ELF64 via process module (native execution)
    if fmt == BinaryFormat::Elf64 {
        match crate::process::exec(filename, data) {
            Ok(pid) => {
                // Process ran and completed
                crate::process::cleanup();
                return RunResult::Scheduled(pid);
            }
            Err(_e) => {
                return RunResult::Failed;
            }
        }
    }

    // Handle other formats via legacy runtimes
    unsafe {
        if let Some(reg) = REGISTRY.as_mut() {
            for r in reg.iter() {
                if r.can_run(fmt) {
                    return r.run(filename, data);
                }
            }
        }
    }
    RunResult::Failed
}

pub fn schedule_task_record(id: u32, filename: String) {
    unsafe {
        if TASKS.is_none() {
            TASKS = Some(Vec::new());
        }
        if let Some(t) = TASKS.as_mut() {
            t.push((id, filename));
        }
    }
}

// Poll registered runtimes
pub fn poll_tasks() {
    dos16::poll_tasks();
    // ELF64 processes handled separately via process module
}
