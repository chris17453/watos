//! Task State Segment (TSS) for Ring 3 → Ring 0 transitions
//!
//! The TSS holds the kernel stack pointer that the CPU switches to
//! when transitioning from user mode (Ring 3) to kernel mode (Ring 0).

use core::mem::size_of;

/// Stack size for double fault handler (separate from main kernel stack)
const DOUBLE_FAULT_STACK_SIZE: usize = 4096;

/// Double fault stack - must be separate to handle stack overflow
static mut DOUBLE_FAULT_STACK: [u8; DOUBLE_FAULT_STACK_SIZE] = [0; DOUBLE_FAULT_STACK_SIZE];

/// TSS structure for x86-64
#[repr(C, packed)]
pub struct TaskStateSegment {
    reserved1: u32,
    /// Ring 0 stack pointer - used when entering kernel from user mode
    pub rsp0: u64,
    /// Ring 1 stack pointer (unused)
    pub rsp1: u64,
    /// Ring 2 stack pointer (unused)
    pub rsp2: u64,
    reserved2: u64,
    /// Interrupt Stack Table entry 1 (for double fault)
    pub ist1: u64,
    /// IST entries 2-7
    pub ist2: u64,
    pub ist3: u64,
    pub ist4: u64,
    pub ist5: u64,
    pub ist6: u64,
    pub ist7: u64,
    reserved3: u64,
    reserved4: u16,
    /// I/O permission bitmap offset
    pub iomap_base: u16,
}

impl TaskStateSegment {
    pub const fn new() -> Self {
        TaskStateSegment {
            reserved1: 0,
            rsp0: 0,
            rsp1: 0,
            rsp2: 0,
            reserved2: 0,
            ist1: 0,
            ist2: 0,
            ist3: 0,
            ist4: 0,
            ist5: 0,
            ist6: 0,
            ist7: 0,
            reserved3: 0,
            reserved4: 0,
            iomap_base: size_of::<TaskStateSegment>() as u16,
        }
    }
}

/// TSS descriptor for GDT (16 bytes in 64-bit mode)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct TssDescriptor {
    limit_low: u16,
    base_low: u16,
    base_mid: u8,
    access: u8,
    granularity: u8,
    base_high: u8,
    base_upper: u32,
    reserved: u32,
}

impl TssDescriptor {
    pub fn new(tss: &TaskStateSegment) -> Self {
        let base = tss as *const _ as u64;
        let limit = size_of::<TaskStateSegment>() as u64 - 1;

        TssDescriptor {
            limit_low: (limit & 0xFFFF) as u16,
            base_low: (base & 0xFFFF) as u16,
            base_mid: ((base >> 16) & 0xFF) as u8,
            access: 0x89, // Present, Ring 0, TSS Available
            granularity: ((limit >> 16) & 0x0F) as u8,
            base_high: ((base >> 24) & 0xFF) as u8,
            base_upper: (base >> 32) as u32,
            reserved: 0,
        }
    }

    /// Get as two u64 values for embedding in GDT
    pub fn as_u64_pair(&self) -> (u64, u64) {
        unsafe {
            let ptr = self as *const _ as *const u64;
            (*ptr, *ptr.add(1))
        }
    }
}

/// Global TSS instance
static mut TSS: TaskStateSegment = TaskStateSegment::new();

/// Initialize TSS with kernel stack and IST entries
pub fn init(kernel_stack: u64) {
    unsafe {
        // Set kernel stack for Ring 3 -> Ring 0 transitions
        TSS.rsp0 = kernel_stack;

        // Set IST1 for double fault handler (critical!)
        // This ensures double faults use a known-good stack
        let df_stack_top = DOUBLE_FAULT_STACK.as_ptr() as u64 + DOUBLE_FAULT_STACK_SIZE as u64;
        TSS.ist1 = df_stack_top;

        crate::serial_write(b"[TSS] RSP0=0x");
        crate::serial_hex(kernel_stack);
        crate::serial_write(b" IST1=0x");
        crate::serial_hex(df_stack_top);
        crate::serial_write(b"\r\n");
    }
}

/// Get TSS descriptor for GDT
pub fn descriptor() -> TssDescriptor {
    unsafe { TssDescriptor::new(&TSS) }
}

/// Update kernel stack (for process context switches)
pub fn set_kernel_stack(stack_ptr: u64) {
    unsafe {
        TSS.rsp0 = stack_ptr;
    }
}

/// Get current kernel stack
pub fn get_kernel_stack() -> u64 {
    unsafe { TSS.rsp0 }
}
