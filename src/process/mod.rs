//! Process Management for WATOS
//!
//! Handles loading and executing native x86-64 ELF binaries.

extern crate alloc;
use alloc::string::String;

pub mod elf;

// Simple serial output for debugging (port 0x3F8)
#[allow(dead_code)]
unsafe fn debug_serial(s: &[u8]) {
    for &byte in s {
        // Wait for transmit ready
        let mut status: u8;
        loop {
            core::arch::asm!("in al, dx", in("dx") 0x3FD_u16, out("al") status, options(nostack));
            if status & 0x20 != 0 { break; }
        }
        core::arch::asm!("out dx, al", in("dx") 0x3F8_u16, in("al") byte, options(nostack));
    }
}

unsafe fn debug_hex(val: u64) {
    let hex_chars = b"0123456789ABCDEF";
    let mut buf = [0u8; 16];
    for i in 0..16 {
        buf[15 - i] = hex_chars[((val >> (i * 4)) & 0xF) as usize];
    }
    debug_serial(&buf);
}

/// Process state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProcessState {
    Ready,
    Running,
    Terminated(i32),  // Exit code
}

/// Process control block
pub struct Process {
    pub id: u32,
    pub name: String,
    pub state: ProcessState,
    pub entry_point: u64,
    pub stack_top: u64,
    pub heap_base: u64,
    pub heap_size: usize,
}

/// Process table - tracks all processes
const MAX_PROCESSES: usize = 16;
static mut PROCESSES: [Option<Process>; MAX_PROCESSES] = [
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
];
static mut NEXT_PID: u32 = 1;
static mut CURRENT_PROCESS: Option<u32> = None;

/// Memory regions for user processes
/// Each process gets 1MB of space (reduced to fit in lower memory)
const PROCESS_MEM_BASE: u64 = 0x400000;   // 4MB mark (after kernel at 1MB)
const PROCESS_MEM_SIZE: u64 = 0x100000;   // 1MB per process
const PROCESS_STACK_SIZE: u64 = 0x10000;  // 64KB stack

/// Saved kernel context for returning from process
static mut KERNEL_RSP: u64 = 0;
static mut KERNEL_RBP: u64 = 0;

/// Allocate memory region for a new process
fn allocate_process_memory(pid: u32) -> (u64, u64, u64) {
    let base = PROCESS_MEM_BASE + (pid as u64 - 1) * PROCESS_MEM_SIZE;
    let stack_top = base + PROCESS_MEM_SIZE;  // Stack at top, grows down
    let heap_base = base + 0x80000;  // Heap starts 512KB into region (leave room for code)
    (base, stack_top, heap_base)
}

/// Load and execute an ELF64 binary
pub fn exec(name: &str, data: &[u8]) -> Result<u32, &'static str> {
    unsafe { debug_serial(b"exec: parsing ELF...\r\n"); }

    // Parse ELF header
    let elf = elf::Elf64::parse(data)?;

    unsafe {
        debug_serial(b"exec: ELF parsed, entry=0x");
        debug_hex(elf.entry);
        debug_serial(b"\r\n");
    }

    // Allocate PID and memory
    let pid = unsafe {
        let p = NEXT_PID;
        NEXT_PID += 1;
        p
    };

    let (load_base, stack_top, heap_base) = allocate_process_memory(pid);

    unsafe {
        debug_serial(b"exec: load_base=0x");
        debug_hex(load_base);
        debug_serial(b" stack=0x");
        debug_hex(stack_top);
        debug_serial(b"\r\n");
    }

    // Load program segments
    unsafe { debug_serial(b"exec: loading segments...\r\n"); }
    elf.load_segments(data, load_base)?;
    unsafe { debug_serial(b"exec: segments loaded\r\n"); }

    // Calculate actual entry point (relocated)
    // Find the lowest vaddr to calculate entry offset
    let min_vaddr = elf.phdrs.iter()
        .filter(|p| p.ptype == elf::PT_LOAD)
        .map(|p| p.vaddr)
        .min()
        .unwrap_or(0);
    let entry = load_base + (elf.entry - min_vaddr);

    // Create process entry
    let process = Process {
        id: pid,
        name: String::from(name),
        state: ProcessState::Ready,
        entry_point: entry,
        stack_top,
        heap_base,
        heap_size: (PROCESS_MEM_SIZE - PROCESS_STACK_SIZE - 0x1000) as usize, // Leave 4KB for code
    };

    // Store in process table
    unsafe {
        for slot in PROCESSES.iter_mut() {
            if slot.is_none() {
                *slot = Some(process);
                break;
            }
        }
    }

    // Run the process
    run_process(pid)?;

    Ok(pid)
}

/// Run a process by jumping to its entry point
fn run_process(pid: u32) -> Result<(), &'static str> {
    let (entry, _stack_top) = unsafe {
        let proc = PROCESSES.iter()
            .find_map(|p| p.as_ref().filter(|p| p.id == pid))
            .ok_or("Process not found")?;

        CURRENT_PROCESS = Some(pid);
        (proc.entry_point, proc.stack_top)
    };

    // Debug: print entry point
    unsafe {
        debug_serial(b"Process entry: 0x");
        debug_hex(entry);
        debug_serial(b"\r\n");
    }

    // Simply call the entry point as a function
    // The binary's _start will call watos_exit which handles returning
    unsafe {
        // Save kernel context for watos_exit to return to
        core::arch::asm!(
            "mov {rsp}, rsp",
            "mov {rbp}, rbp",
            rsp = out(reg) KERNEL_RSP,
            rbp = out(reg) KERNEL_RBP,
            options(nostack, preserves_flags)
        );

        debug_serial(b"Calling entry point...\r\n");

        // Dump first 16 bytes at entry point to verify code is there
        debug_serial(b"Code at entry: ");
        let code_ptr = entry as *const u8;
        for i in 0..16 {
            let byte = *code_ptr.add(i);
            let hex = b"0123456789ABCDEF";
            core::arch::asm!("out dx, al", in("dx") 0x3F8_u16, in("al") hex[(byte >> 4) as usize], options(nostack));
            core::arch::asm!("out dx, al", in("dx") 0x3F8_u16, in("al") hex[(byte & 0xF) as usize], options(nostack));
            core::arch::asm!("out dx, al", in("dx") 0x3F8_u16, in("al") b' ', options(nostack));
        }
        debug_serial(b"\r\n");

        // Call the entry point directly on current stack
        // _start function is declared as -> ! so it should call watos_exit
        let entry_fn: extern "C" fn() = core::mem::transmute(entry);
        entry_fn();

        debug_serial(b"Process returned normally\r\n");

        // Clear kernel context
        KERNEL_RSP = 0;
        KERNEL_RBP = 0;
    }

    // Mark process as terminated
    unsafe {
        CURRENT_PROCESS = None;
        for slot in PROCESSES.iter_mut() {
            if let Some(ref mut p) = slot {
                if p.id == pid {
                    p.state = ProcessState::Terminated(0);
                    break;
                }
            }
        }
    }

    Ok(())
}

/// Return to kernel from a running process (called by watos_exit)
/// This is unsafe and modifies the stack to return to the kernel
#[no_mangle]
pub extern "C" fn process_exit_to_kernel(code: i32) -> ! {
    unsafe {
        // Mark current process as terminated
        if let Some(pid) = CURRENT_PROCESS {
            for slot in PROCESSES.iter_mut() {
                if let Some(ref mut p) = slot {
                    if p.id == pid {
                        p.state = ProcessState::Terminated(code);
                        break;
                    }
                }
            }
            CURRENT_PROCESS = None;
        }

        // Return to kernel by restoring saved context
        // The saved RSP points to where we need to return
        // We need to manipulate the stack to return to the right place
        if KERNEL_RSP != 0 {
            // Jump back to the kernel's stack frame
            // This will "return" to label 2 in run_process
            core::arch::asm!(
                "mov rsp, {rsp}",
                "mov rbp, {rbp}",
                "ret",
                rsp = in(reg) KERNEL_RSP,
                rbp = in(reg) KERNEL_RBP,
                options(noreturn)
            );
        }

        // Fallback: log error and return to scheduler
        unsafe {
            debug_serial(b"ERROR: process_exit_to_kernel failed to restore kernel state\r\n");
            debug_serial(b"Process ID: ");
            if let Some(pid) = CURRENT_PROCESS {
                debug_hex(pid as u64);
            } else {
                debug_serial(b"<none>");
            }
            debug_serial(b"\r\n");
            
            // Clear the current process to prevent recursion
            CURRENT_PROCESS = None;
            
            // Attempt to continue kernel operation instead of halting
            debug_serial(b"Attempting to continue kernel operation...\r\n");
            loop {
                core::arch::asm!("hlt", options(nostack, nomem));
            }
        }
    }
}

/// Called by SYS_EXIT syscall
pub fn exit_current(code: i32) {
    unsafe {
        if let Some(pid) = CURRENT_PROCESS {
            for slot in PROCESSES.iter_mut() {
                if let Some(ref mut p) = slot {
                    if p.id == pid {
                        p.state = ProcessState::Terminated(code);
                        break;
                    }
                }
            }
        }
    }
}

/// Get current process ID
pub fn current_pid() -> Option<u32> {
    unsafe { CURRENT_PROCESS }
}

/// Clean up terminated processes
pub fn cleanup() {
    unsafe {
        for slot in PROCESSES.iter_mut() {
            if let Some(ref p) = slot {
                if matches!(p.state, ProcessState::Terminated(_)) {
                    *slot = None;
                }
            }
        }
    }
}
