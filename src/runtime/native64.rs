//! Native 64-bit runtime for WATOS
//!
//! Executes x86-64 ELF binaries with syscall support via INT 0x80.
//! Uses interpretation for portability and safety.

extern crate alloc;
use alloc::{boxed::Box, string::String, vec::Vec, vec};
use core::sync::atomic::{AtomicU32, Ordering};

use super::{BinaryFormat, Runtime, RunResult};
use crate::console;

/// Serial port for debug output
const SERIAL_PORT: u16 = 0x3F8;

/// Read from I/O port
#[inline]
unsafe fn inb(port: u16) -> u8 {
    let result: u8;
    core::arch::asm!(
        "in al, dx",
        out("al") result,
        in("dx") port,
        options(nomem, nostack, preserves_flags)
    );
    result
}

/// Write to I/O port
#[inline]
unsafe fn outb(port: u16, value: u8) {
    core::arch::asm!(
        "out dx, al",
        in("dx") port,
        in("al") value,
        options(nomem, nostack, preserves_flags)
    );
}

/// Write a byte to serial port for debugging
fn serial_write_byte(b: u8) {
    unsafe {
        // Wait for transmit buffer empty
        while (inb(SERIAL_PORT + 5) & 0x20) == 0 {}
        outb(SERIAL_PORT, b);
    }
}

/// Write bytes to serial port
fn serial_print(s: &[u8]) {
    for &b in s {
        serial_write_byte(b);
    }
}

/// Write a string and newline to serial
fn serial_println(s: &[u8]) {
    serial_print(s);
    serial_print(b"\r\n");
}

/// Print a number in hex to serial
fn serial_print_hex(n: u64) {
    const HEX: &[u8] = b"0123456789ABCDEF";
    serial_print(b"0x");
    for i in (0..16).rev() {
        let digit = ((n >> (i * 4)) & 0xF) as usize;
        serial_write_byte(HEX[digit]);
    }
}

/// Syscall numbers for WATOS native apps (compatible with gwbasic expectations)
pub mod syscall {
    pub const SYS_EXIT: u64 = 0;
    pub const SYS_WRITE: u64 = 1;
    pub const SYS_READ: u64 = 2;
    pub const SYS_OPEN: u64 = 3;
    pub const SYS_CLOSE: u64 = 4;
    pub const SYS_GETKEY: u64 = 5;
    pub const SYS_PUTCHAR: u64 = 6;
    pub const SYS_CURSOR: u64 = 7;
    pub const SYS_CLEAR: u64 = 8;
    pub const SYS_COLOR: u64 = 9;
    pub const SYS_TIMER: u64 = 10;
    pub const SYS_SLEEP: u64 = 11;

    // Graphics syscalls
    pub const SYS_GFX_PSET: u64 = 20;
    pub const SYS_GFX_LINE: u64 = 21;
    pub const SYS_GFX_CIRCLE: u64 = 22;
    pub const SYS_GFX_CLS: u64 = 23;
    pub const SYS_GFX_MODE: u64 = 24;
    pub const SYS_GFX_DISPLAY: u64 = 25;

    // VGA syscalls
    pub const SYS_VGA_SET_MODE: u64 = 30;
    pub const SYS_VGA_BLIT: u64 = 33;
    pub const SYS_VGA_FLIP: u64 = 34;
    pub const SYS_VGA_CLEAR: u64 = 35;
}

/// Task state
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Running,
    Blocked,
    Terminated,
}

/// x86-64 CPU register state
#[derive(Clone)]
pub struct Cpu64 {
    // General purpose registers
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,

    // Instruction pointer
    pub rip: u64,

    // Flags register
    pub rflags: u64,
}

impl Default for Cpu64 {
    fn default() -> Self {
        Cpu64 {
            rax: 0, rbx: 0, rcx: 0, rdx: 0,
            rsi: 0, rdi: 0, rbp: 0, rsp: 0,
            r8: 0, r9: 0, r10: 0, r11: 0,
            r12: 0, r13: 0, r14: 0, r15: 0,
            rip: 0,
            rflags: 0x202, // IF set
        }
    }
}

/// Native 64-bit task
pub struct Native64Task {
    pub id: u32,
    pub filename: String,
    pub state: TaskState,
    pub cpu: Cpu64,
    pub memory: Vec<u8>,
    pub memory_base: u64,  // Virtual address base
    pub console_id: u8,
    pub exit_code: i32,
}

impl Native64Task {
    pub fn new(id: u32, filename: String) -> Self {
        Native64Task {
            id,
            filename,
            state: TaskState::Running,
            cpu: Cpu64::default(),
            memory: Vec::new(),
            memory_base: 0,
            console_id: 0,
            exit_code: 0,
        }
    }
}

/// ELF64 header structure
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct Elf64Header {
    e_ident: [u8; 16],      // Magic number and other info
    e_type: u16,            // Object file type
    e_machine: u16,         // Architecture
    e_version: u32,         // Object file version
    e_entry: u64,           // Entry point virtual address
    e_phoff: u64,           // Program header table file offset
    e_shoff: u64,           // Section header table file offset
    e_flags: u32,           // Processor-specific flags
    e_ehsize: u16,          // ELF header size in bytes
    e_phentsize: u16,       // Program header table entry size
    e_phnum: u16,           // Program header table entry count
    e_shentsize: u16,       // Section header table entry size
    e_shnum: u16,           // Section header table entry count
    e_shstrndx: u16,        // Section header string table index
}

/// ELF64 program header
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct Elf64Phdr {
    p_type: u32,            // Segment type
    p_flags: u32,           // Segment flags
    p_offset: u64,          // Segment file offset
    p_vaddr: u64,           // Segment virtual address
    p_paddr: u64,           // Segment physical address
    p_filesz: u64,          // Segment size in file
    p_memsz: u64,           // Segment size in memory
    p_align: u64,           // Segment alignment
}

const PT_LOAD: u32 = 1;     // Loadable segment

/// Task list for native64 runtime
static mut TASK_LIST: Option<Vec<Native64Task>> = None;
static NEXT_TASK_ID: AtomicU32 = AtomicU32::new(1);

/// Native 64-bit runtime
pub struct Native64Runtime;

impl Native64Runtime {
    pub fn new() -> Self {
        Native64Runtime
    }

    /// Parse ELF header and load program segments
    fn load_elf(data: &[u8]) -> Option<(u64, Vec<u8>, u64)> {
        if data.len() < core::mem::size_of::<Elf64Header>() {
            return None;
        }

        // Safety: We've checked the length
        let header: Elf64Header = unsafe {
            core::ptr::read_unaligned(data.as_ptr() as *const Elf64Header)
        };

        // Validate ELF magic
        if header.e_ident[0..4] != [0x7F, b'E', b'L', b'F'] {
            return None;
        }

        // Check it's 64-bit x86-64
        if header.e_ident[4] != 2 || header.e_machine != 0x3E {
            return None;
        }

        let entry = header.e_entry;
        let phoff = header.e_phoff as usize;
        let phnum = header.e_phnum as usize;
        let phentsize = header.e_phentsize as usize;

        // Find memory range needed
        let mut min_vaddr = u64::MAX;
        let mut max_vaddr = 0u64;

        for i in 0..phnum {
            let ph_offset = phoff + i * phentsize;
            if ph_offset + phentsize > data.len() {
                continue;
            }

            let phdr: Elf64Phdr = unsafe {
                core::ptr::read_unaligned(data.as_ptr().add(ph_offset) as *const Elf64Phdr)
            };

            if phdr.p_type == PT_LOAD {
                min_vaddr = min_vaddr.min(phdr.p_vaddr);
                max_vaddr = max_vaddr.max(phdr.p_vaddr + phdr.p_memsz);
            }
        }

        if min_vaddr == u64::MAX {
            return None;
        }

        // Allocate memory for program
        let memory_size = (max_vaddr - min_vaddr) as usize;
        let mut memory = vec![0u8; memory_size + 0x10000]; // Extra for stack

        // Load segments
        for i in 0..phnum {
            let ph_offset = phoff + i * phentsize;
            if ph_offset + phentsize > data.len() {
                continue;
            }

            let phdr: Elf64Phdr = unsafe {
                core::ptr::read_unaligned(data.as_ptr().add(ph_offset) as *const Elf64Phdr)
            };

            if phdr.p_type == PT_LOAD {
                let file_offset = phdr.p_offset as usize;
                let file_size = phdr.p_filesz as usize;
                let mem_offset = (phdr.p_vaddr - min_vaddr) as usize;

                if file_offset + file_size <= data.len() && mem_offset + file_size <= memory.len() {
                    memory[mem_offset..mem_offset + file_size]
                        .copy_from_slice(&data[file_offset..file_offset + file_size]);
                }
            }
        }

        Some((entry, memory, min_vaddr))
    }
}

impl Runtime for Native64Runtime {
    fn name(&self) -> &'static str {
        "Native64Runtime"
    }

    fn can_run(&self, format: BinaryFormat) -> bool {
        matches!(format, BinaryFormat::Elf64)
    }

    fn run(&self, filename: &str, data: &[u8]) -> RunResult {
        // Load ELF
        let (entry, memory, base) = match Self::load_elf(data) {
            Some(result) => result,
            None => return RunResult::Failed,
        };

        let task_id = NEXT_TASK_ID.fetch_add(1, Ordering::SeqCst);

        let mut task = Native64Task::new(task_id, filename.into());
        task.memory = memory;
        task.memory_base = base;
        task.cpu.rip = entry;

        // Set up stack at end of memory
        let stack_top = base + (task.memory.len() as u64) - 8;
        task.cpu.rsp = stack_top;
        task.cpu.rbp = stack_top;

        // Initialize console for task
        task.console_id = 1; // Use secondary console

        unsafe {
            if TASK_LIST.is_none() {
                TASK_LIST = Some(Vec::new());
            }
            if let Some(list) = TASK_LIST.as_mut() {
                list.push(task);
            }
        }

        serial_print(b"Native64: Started task ");
        serial_print(filename.as_bytes());
        serial_println(b"");
        RunResult::Scheduled(task_id)
    }
}

/// Poll and execute native64 tasks
pub fn poll_tasks() {
    unsafe {
        if TASK_LIST.is_none() {
            return;
        }

        if let Some(tasks) = TASK_LIST.as_mut() {
            for task in tasks.iter_mut() {
                if task.state == TaskState::Running {
                    // Execute a batch of instructions
                    for _ in 0..256 {
                        if task.state != TaskState::Running {
                            break;
                        }
                        execute_instruction(task);
                    }
                }
            }

            // Remove terminated tasks
            tasks.retain(|t| t.state != TaskState::Terminated);
        }
    }
}

/// Check if any native64 tasks are running
pub fn has_running_tasks() -> bool {
    unsafe {
        if let Some(tasks) = TASK_LIST.as_ref() {
            tasks.iter().any(|t| t.state == TaskState::Running)
        } else {
            false
        }
    }
}

/// Read a byte from task memory
fn mem_read_u8(task: &Native64Task, addr: u64) -> Option<u8> {
    let offset = addr.checked_sub(task.memory_base)? as usize;
    task.memory.get(offset).copied()
}

/// Read a u16 from task memory
fn mem_read_u16(task: &Native64Task, addr: u64) -> Option<u16> {
    let offset = addr.checked_sub(task.memory_base)? as usize;
    if offset + 2 <= task.memory.len() {
        Some(u16::from_le_bytes([
            task.memory[offset],
            task.memory[offset + 1],
        ]))
    } else {
        None
    }
}

/// Read a u32 from task memory
fn mem_read_u32(task: &Native64Task, addr: u64) -> Option<u32> {
    let offset = addr.checked_sub(task.memory_base)? as usize;
    if offset + 4 <= task.memory.len() {
        Some(u32::from_le_bytes([
            task.memory[offset],
            task.memory[offset + 1],
            task.memory[offset + 2],
            task.memory[offset + 3],
        ]))
    } else {
        None
    }
}

/// Read a u64 from task memory
fn mem_read_u64(task: &Native64Task, addr: u64) -> Option<u64> {
    let offset = addr.checked_sub(task.memory_base)? as usize;
    if offset + 8 <= task.memory.len() {
        Some(u64::from_le_bytes([
            task.memory[offset],
            task.memory[offset + 1],
            task.memory[offset + 2],
            task.memory[offset + 3],
            task.memory[offset + 4],
            task.memory[offset + 5],
            task.memory[offset + 6],
            task.memory[offset + 7],
        ]))
    } else {
        None
    }
}

/// Write a byte to task memory
fn mem_write_u8(task: &mut Native64Task, addr: u64, val: u8) -> bool {
    if let Some(offset) = addr.checked_sub(task.memory_base) {
        let offset = offset as usize;
        if offset < task.memory.len() {
            task.memory[offset] = val;
            return true;
        }
    }
    false
}

/// Write a u64 to task memory
fn mem_write_u64(task: &mut Native64Task, addr: u64, val: u64) -> bool {
    if let Some(offset) = addr.checked_sub(task.memory_base) {
        let offset = offset as usize;
        if offset + 8 <= task.memory.len() {
            let bytes = val.to_le_bytes();
            task.memory[offset..offset + 8].copy_from_slice(&bytes);
            return true;
        }
    }
    false
}

/// Execute a single x86-64 instruction
fn execute_instruction(task: &mut Native64Task) {
    let rip = task.cpu.rip;

    // Read instruction bytes
    let b0 = match mem_read_u8(task, rip) {
        Some(b) => b,
        None => {
            serial_print(b"Native64: Invalid memory access at RIP=");
            serial_print_hex(rip);
            serial_println(b"");
            task.state = TaskState::Terminated;
            task.exit_code = -1;
            return;
        }
    };

    // Handle REX prefixes
    let has_rex = (b0 & 0xF0) == 0x40;
    let rex_w = has_rex && (b0 & 0x08) != 0;
    let rex_r = has_rex && (b0 & 0x04) != 0;
    let rex_x = has_rex && (b0 & 0x02) != 0;
    let rex_b = has_rex && (b0 & 0x01) != 0;

    let (opcode_start, _rex_w, _rex_r, _rex_x, _rex_b) = if has_rex {
        (rip + 1, rex_w, rex_r, rex_x, rex_b)
    } else {
        (rip, false, false, false, false)
    };

    let opcode = match mem_read_u8(task, opcode_start) {
        Some(b) => b,
        None => {
            task.state = TaskState::Terminated;
            return;
        }
    };

    match opcode {
        // NOP
        0x90 => {
            task.cpu.rip = opcode_start + 1;
        }

        // RET
        0xC3 => {
            // Pop return address from stack
            if let Some(ret_addr) = mem_read_u64(task, task.cpu.rsp) {
                task.cpu.rsp += 8;
                task.cpu.rip = ret_addr;
            } else {
                // Stack underflow - terminate
                task.state = TaskState::Terminated;
            }
        }

        // INT imm8
        0xCD => {
            let int_num = mem_read_u8(task, opcode_start + 1).unwrap_or(0);
            task.cpu.rip = opcode_start + 2;

            if int_num == 0x80 {
                handle_syscall(task);
            } else {
                serial_print(b"Native64: Unhandled INT ");
                serial_print_hex(int_num as u64);
                serial_println(b"");
            }
        }

        // SYSCALL (0x0F 0x05)
        0x0F => {
            let b1 = mem_read_u8(task, opcode_start + 1).unwrap_or(0);
            if b1 == 0x05 {
                // SYSCALL instruction
                task.cpu.rip = opcode_start + 2;
                handle_syscall(task);
            } else {
                serial_print(b"Native64: Unknown 0F opcode: 0F ");
                serial_print_hex(b1 as u64);
                serial_println(b"");
                task.cpu.rip = opcode_start + 2;
            }
        }

        // MOV r64, imm64 (B8-BF with REX.W)
        0xB8..=0xBF if has_rex && rex_w => {
            let reg = (opcode - 0xB8) | if rex_b { 8 } else { 0 };
            if let Some(imm) = mem_read_u64(task, opcode_start + 1) {
                set_reg64(task, reg, imm);
                task.cpu.rip = opcode_start + 9;
            } else {
                task.state = TaskState::Terminated;
            }
        }

        // MOV r32, imm32 (B8-BF without REX.W)
        0xB8..=0xBF => {
            let reg = opcode - 0xB8;
            if let Some(imm) = mem_read_u32(task, opcode_start + 1) {
                set_reg64(task, reg, imm as u64);
                task.cpu.rip = opcode_start + 5;
            } else {
                task.state = TaskState::Terminated;
            }
        }

        // XOR r/m64, r64 (with REX.W) or XOR r/m32, r32
        0x31 => {
            let modrm = mem_read_u8(task, opcode_start + 1).unwrap_or(0);
            let mod_field = (modrm >> 6) & 0x03;
            let reg_field = ((modrm >> 3) & 0x07) | if rex_r { 8 } else { 0 };
            let rm_field = (modrm & 0x07) | if rex_b { 8 } else { 0 };

            if mod_field == 0x03 {
                // Register-register XOR
                let src = get_reg64(task, reg_field);
                let dst = get_reg64(task, rm_field);
                let result = dst ^ src;
                set_reg64(task, rm_field, result);

                // Update flags
                task.cpu.rflags &= !(0x8C5); // Clear OF, SF, ZF, PF, CF
                if result == 0 {
                    task.cpu.rflags |= 0x40; // ZF
                }

                task.cpu.rip = opcode_start + 2;
            } else {
                serial_println(b"Native64: XOR with memory not yet supported");
                task.cpu.rip = opcode_start + 2;
            }
        }

        // PUSH r64
        0x50..=0x57 => {
            let reg = (opcode - 0x50) | if rex_b { 8 } else { 0 };
            let val = get_reg64(task, reg);
            task.cpu.rsp -= 8;
            mem_write_u64(task, task.cpu.rsp, val);
            task.cpu.rip = opcode_start + 1;
        }

        // POP r64
        0x58..=0x5F => {
            let reg = (opcode - 0x58) | if rex_b { 8 } else { 0 };
            if let Some(val) = mem_read_u64(task, task.cpu.rsp) {
                set_reg64(task, reg, val);
                task.cpu.rsp += 8;
            }
            task.cpu.rip = opcode_start + 1;
        }

        // HLT - terminate task
        0xF4 => {
            task.state = TaskState::Terminated;
        }

        _ => {
            // Unknown opcode - skip it
            serial_print(b"Native64: Unknown opcode ");
            serial_print_hex(opcode as u64);
            serial_print(b" at RIP=");
            serial_print_hex(rip);
            serial_println(b"");
            task.cpu.rip = opcode_start + 1;
        }
    }
}

/// Get value from a 64-bit register
fn get_reg64(task: &Native64Task, reg: u8) -> u64 {
    match reg {
        0 => task.cpu.rax,
        1 => task.cpu.rcx,
        2 => task.cpu.rdx,
        3 => task.cpu.rbx,
        4 => task.cpu.rsp,
        5 => task.cpu.rbp,
        6 => task.cpu.rsi,
        7 => task.cpu.rdi,
        8 => task.cpu.r8,
        9 => task.cpu.r9,
        10 => task.cpu.r10,
        11 => task.cpu.r11,
        12 => task.cpu.r12,
        13 => task.cpu.r13,
        14 => task.cpu.r14,
        15 => task.cpu.r15,
        _ => 0,
    }
}

/// Set value of a 64-bit register
fn set_reg64(task: &mut Native64Task, reg: u8, val: u64) {
    match reg {
        0 => task.cpu.rax = val,
        1 => task.cpu.rcx = val,
        2 => task.cpu.rdx = val,
        3 => task.cpu.rbx = val,
        4 => task.cpu.rsp = val,
        5 => task.cpu.rbp = val,
        6 => task.cpu.rsi = val,
        7 => task.cpu.rdi = val,
        8 => task.cpu.r8 = val,
        9 => task.cpu.r9 = val,
        10 => task.cpu.r10 = val,
        11 => task.cpu.r11 = val,
        12 => task.cpu.r12 = val,
        13 => task.cpu.r13 = val,
        14 => task.cpu.r14 = val,
        15 => task.cpu.r15 = val,
        _ => {}
    }
}

/// Handle INT 0x80 / SYSCALL
fn handle_syscall(task: &mut Native64Task) {
    let syscall_num = task.cpu.rax;
    let arg1 = task.cpu.rdi;
    let arg2 = task.cpu.rsi;
    let arg3 = task.cpu.rdx;
    let _arg4 = task.cpu.r10;
    let _arg5 = task.cpu.r8;
    let _arg6 = task.cpu.r9;

    match syscall_num {
        syscall::SYS_EXIT => {
            task.exit_code = arg1 as i32;
            task.state = TaskState::Terminated;
            serial_print(b"Native64: Task exited with code ");
            serial_print_hex(task.exit_code as u64);
            serial_println(b"");
        }

        syscall::SYS_WRITE => {
            // arg1 = fd (0 = stdout), arg2 = buf ptr, arg3 = len
            let buf_addr = arg2;
            let len = arg3 as usize;

            // Read string from task memory and print
            let mut output = Vec::with_capacity(len);
            for i in 0..len {
                if let Some(b) = mem_read_u8(task, buf_addr + i as u64) {
                    output.push(b);
                } else {
                    break;
                }
            }

            // Output to active console
            console::print(&output);

            task.cpu.rax = output.len() as u64;
        }

        syscall::SYS_READ => {
            // arg1 = fd, arg2 = buf ptr, arg3 = max len
            // For now, just return 0 (EOF)
            task.cpu.rax = 0;
        }

        syscall::SYS_GETKEY => {
            // Get keyboard scancode
            if let Some(scancode) = crate::interrupts::get_scancode() {
                task.cpu.rax = scancode as u64;
            } else {
                task.cpu.rax = 0;
            }
        }

        syscall::SYS_PUTCHAR => {
            // arg1 = character
            let ch = (arg1 & 0xFF) as u8;
            console::putchar(ch);
            task.cpu.rax = 0;
        }

        syscall::SYS_CLEAR => {
            // Clear screen
            console::clear();
            task.cpu.rax = 0;
        }

        syscall::SYS_TIMER => {
            // Return timer ticks (placeholder)
            task.cpu.rax = 0;
        }

        syscall::SYS_SLEEP => {
            // arg1 = milliseconds - just return for now
            task.cpu.rax = 0;
        }

        syscall::SYS_GFX_MODE => {
            // arg1 = mode number
            serial_print(b"Native64: SCREEN mode ");
            serial_print_hex(arg1);
            serial_println(b"");
            task.cpu.rax = 0;
        }

        syscall::SYS_GFX_CLS => {
            task.cpu.rax = 0;
        }

        _ => {
            serial_print(b"Native64: Unknown syscall ");
            serial_print_hex(syscall_num);
            serial_println(b"");
            task.cpu.rax = u64::MAX; // -1 as error
        }
    }
}

// WATOS syscall functions are now in main.rs
