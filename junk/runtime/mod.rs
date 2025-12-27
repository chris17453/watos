//! Runtime execution environments
//!
//! Uses watos-runtime and watos-dos-emulator crates

extern crate alloc;
use alloc::boxed::Box;
use alloc::vec::Vec;
use alloc::string::String;

pub use watos_runtime::{BinaryFormat, RunResult, Runtime, detect_format};

static mut TASKS: Option<Vec<(u32, String)>> = None;

pub fn register_default_runtimes() {
    unsafe {
        if TASKS.is_none() {
            TASKS = Some(Vec::new());
        }
    }
}

pub fn detect_and_run(filename: &str, data: &[u8]) -> RunResult {
    let fmt = detect_format(data);

    // Handle ELF64 via process module (native execution)
    if fmt == BinaryFormat::Elf64 {
        match crate::process::exec(filename, data) {
            Ok(pid) => {
                crate::process::cleanup();
                return RunResult::Scheduled(pid);
            }
            Err(_e) => {
                return RunResult::Failed;
            }
        }
    }

    // DOS COM/EXE - not yet implemented in new crate
    if fmt == BinaryFormat::DosCom || fmt == BinaryFormat::DosExe {
        // TODO: Use watos-dos-emulator crate
        return RunResult::Failed;
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

pub fn poll_tasks() {
    // TODO: Poll DOS emulator tasks
}

/// Check if any DOS tasks are running
pub fn has_running_tasks() -> bool {
    // TODO: Check DOS emulator
    false
}

/// Get DOS free memory
pub fn get_dos_free_memory() -> Option<u32> {
    // TODO: Get from DOS emulator
    Some(640 * 1024) // 640KB conventional memory
}
