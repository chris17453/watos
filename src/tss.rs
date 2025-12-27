//! Task State Segment (TSS) for Ring 3 → Ring 0 transitions
//! 
//! The TSS holds the kernel stack pointer that the CPU switches to
//! when transitioning from Ring 3 (user mode) to Ring 0 (kernel mode)
//! during interrupts and syscalls.

use core::mem::size_of;

/// TSS structure for x86-64
/// 
/// When an interrupt/syscall occurs in Ring 3, the CPU:
/// 1. Looks up RSP0 from the TSS
/// 2. Switches to that stack
/// 3. Pushes user SS, RSP, RFLAGS, CS, RIP
/// 4. Jumps to interrupt handler in Ring 0
#[repr(C, packed)]
pub struct TaskStateSegment {
    reserved1: u32,
    
    /// Ring 0 stack pointer - used when entering kernel from user mode
    pub rsp0: u64,
    
    /// Ring 1 stack pointer (unused in most systems)
    pub rsp1: u64,
    
    /// Ring 2 stack pointer (unused in most systems)
    pub rsp2: u64,
    
    reserved2: u64,
    
    /// Interrupt Stack Table pointers (for special interrupts)
    pub ist1: u64,
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

    /// Set the Ring 0 stack pointer
    /// This stack is used when entering kernel mode from user mode
    pub fn set_kernel_stack(&mut self, stack_ptr: u64) {
        self.rsp0 = stack_ptr;
    }
}

/// GDT entry for TSS descriptor (16 bytes in 64-bit mode)
#[repr(C, packed)]
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
            access: 0x89,  // Present, Ring 0, TSS Available
            granularity: ((limit >> 16) & 0x0F) as u8,
            base_high: ((base >> 24) & 0xFF) as u8,
            base_upper: (base >> 32) as u32,
            reserved: 0,
        }
    }
}

/// Global TSS instance
static mut TSS: TaskStateSegment = TaskStateSegment::new();

/// Initialize TSS with kernel stack
pub fn init_tss(kernel_stack: u64) {
    unsafe {
        TSS.set_kernel_stack(kernel_stack);
    }
}

/// Get TSS descriptor for GDT
pub fn get_tss_descriptor() -> TssDescriptor {
    unsafe {
        TssDescriptor::new(&TSS)
    }
}

/// Update TSS kernel stack (for context switches)
pub fn update_kernel_stack(stack_ptr: u64) {
    unsafe {
        TSS.rsp0 = stack_ptr;
    }
}

/// Get TSS base address for loading into TR
pub fn get_tss_address() -> u64 {
    unsafe { &TSS as *const _ as u64 }
}
