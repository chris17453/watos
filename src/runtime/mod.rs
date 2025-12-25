extern crate alloc;
use alloc::{boxed::Box, vec::Vec, string::String};

pub mod dos16;
use dos16::Dos16Runtime;

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum BinaryFormat { RuNative, EightBit, DosCom, DosExe, Unknown }

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
        }
    }
}

pub fn detect_format(_filename: &str, data: &[u8]) -> BinaryFormat {
    if data.starts_with(b"RU64\x01") || data.starts_with(b"RUARM\x01") { return BinaryFormat::RuNative; }
    if data.starts_with(b"RU8\x01") { return BinaryFormat::EightBit; }
    if data.len() >= 2 && &data[0..2] == b"MZ" { return BinaryFormat::DosExe; }
    if data.len() <= 65536 { return BinaryFormat::DosCom; }
    BinaryFormat::Unknown
}

pub fn detect_and_run(filename: &str, data: &[u8]) -> RunResult {
    let fmt = detect_format(filename, data);
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

// Poll registered runtimes (currently only Dos16)
pub fn poll_tasks() {
    dos16::poll_tasks();
}
