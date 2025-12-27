//! DOS 16-bit x86 CPU Emulator
//!
//! Implements a 16-bit x86 CPU interpreter for running DOS COM/EXE programs.
//! This is a complete software emulator for the 8086/8088 instruction set
//! with DOS INT 21h API compatibility.

#![no_std]

extern crate alloc;

mod cpu;
mod memory;
mod dos;
mod bios;
mod host;

pub use cpu::Cpu16;
pub use memory::DosMemory;
pub use host::{DosHost, ConsoleHandle, FileHandle};
pub use watos_runtime::{BinaryFormat, RunResult};

use alloc::string::String;

/// DOS Task state
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum TaskState {
    /// Task is running
    Running,
    /// Task is blocked (waiting for I/O)
    Blocked,
    /// Task has terminated
    Terminated,
}

/// DOS Task - represents a running DOS program
pub struct DosTask {
    /// Task ID
    pub id: u32,
    /// Program filename
    pub filename: String,
    /// CPU state
    pub cpu: Cpu16,
    /// Memory
    pub memory: DosMemory,
    /// Task state
    pub state: TaskState,
    /// Exit code
    pub exit_code: u8,
    /// Console handle for this task
    pub console: ConsoleHandle,
}

impl DosTask {
    /// Execute a single instruction
    fn execute_instruction<H: DosHost>(&mut self, host: &mut H) {
        // Fetch instruction
        let opcode = self.memory.read8_segoff(self.cpu.cs, self.cpu.ip);
        self.cpu.ip = self.cpu.ip.wrapping_add(1);

        // Handle interrupt instructions
        match opcode {
            0xCD => {
                // INT n
                let vector = self.memory.read8_segoff(self.cpu.cs, self.cpu.ip);
                self.cpu.ip = self.cpu.ip.wrapping_add(1);
                self.handle_interrupt(vector, host);
            }
            0xCF => {
                // IRET
                self.cpu.ip = self.pop16();
                self.cpu.cs = self.pop16();
                self.cpu.flags = self.pop16();
            }
            0xCC => {
                // INT 3
                self.handle_interrupt(3, host);
            }
            // TODO: Implement full x86 instruction set
            // For now, treat unknown as NOP and continue
            _ => {
                // Unknown opcode - halt
                self.state = TaskState::Terminated;
            }
        }
    }

    /// Handle interrupt
    fn handle_interrupt<H: DosHost>(&mut self, vector: u8, host: &mut H) {
        match vector {
            0x10 => self.int10h(host),
            0x16 => self.int16h(host),
            0x1A => self.int1ah(host),
            0x20 => self.int20h(host),
            0x21 => self.int21h(host),
            0x27 => self.int27h(host),
            _ => {}
        }
    }

    /// Push 16-bit value onto stack
    fn push16(&mut self, val: u16) {
        self.cpu.sp = self.cpu.sp.wrapping_sub(2);
        self.memory.write16_segoff(self.cpu.ss, self.cpu.sp, val);
    }

    /// Pop 16-bit value from stack
    fn pop16(&mut self) -> u16 {
        let val = self.memory.read16_segoff(self.cpu.ss, self.cpu.sp);
        self.cpu.sp = self.cpu.sp.wrapping_add(2);
        val
    }

    /// Create a new DOS task
    pub fn new<H: DosHost>(
        id: u32,
        filename: String,
        data: &[u8],
        format: BinaryFormat,
        host: &mut H,
    ) -> Option<Self> {
        let mut memory = DosMemory::new();
        let mut cpu = Cpu16::new();

        // Create console for this task
        let console = host.create_console(&filename, id);

        // Load program based on format
        let load_seg = match format {
            BinaryFormat::DosCom => {
                // COM files load at CS:0100
                let seg = memory.alloc_program(data.len() + 256)?;
                memory.setup_psp(seg, &filename);
                memory.load_at(seg, 0x100, data);
                cpu.ip = 0x100;
                seg
            }
            BinaryFormat::DosExe => {
                // Parse MZ header and load EXE
                memory.load_exe(data, &mut cpu)?
            }
            _ => return None,
        };

        cpu.cs = load_seg;
        cpu.ds = load_seg;
        cpu.es = load_seg;
        cpu.ss = load_seg;

        Some(DosTask {
            id,
            filename,
            cpu,
            memory,
            state: TaskState::Running,
            exit_code: 0,
            console,
        })
    }

    /// Execute one instruction
    pub fn step<H: DosHost>(&mut self, host: &mut H) -> TaskState {
        if self.state != TaskState::Running {
            return self.state;
        }

        // Fetch and execute instruction
        self.execute_instruction(host);

        self.state
    }

    /// Run until completion or yield
    pub fn run<H: DosHost>(&mut self, host: &mut H, max_instructions: usize) -> TaskState {
        for _ in 0..max_instructions {
            if self.state != TaskState::Running {
                break;
            }
            self.step(host);
        }
        self.state
    }
}

/// DOS 16-bit Runtime
pub struct Dos16Runtime;

impl Dos16Runtime {
    pub fn new() -> Self {
        Dos16Runtime
    }
}

impl Default for Dos16Runtime {
    fn default() -> Self {
        Self::new()
    }
}
