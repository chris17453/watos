//! Process Management for WATOS
//!
//! Handles loading and executing native x86-64 ELF binaries with memory protection.

#![no_std]

extern crate alloc;
use alloc::string::String;
use alloc::collections::BTreeMap;
use watos_mem::paging::{ProcessPageTable, flags as page_flags, PAGE_SIZE};

pub mod elf;

/// Boot info passed from bootloader at 0x80000
#[repr(C)]
#[derive(Clone, Copy)]
struct BootInfo {
    magic: u32,
    framebuffer_addr: u64,
    framebuffer_width: u32,
    framebuffer_height: u32,
    framebuffer_pitch: u32,
    framebuffer_bpp: u32,
    pixel_format: u32,
    init_app_addr: u64,
    init_app_size: u64,
}

// Simple serial output for debugging (port 0x3F8)
#[allow(dead_code)]
unsafe fn debug_serial(s: &[u8]) {
    for &byte in s {
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

/// Handle type for kernel objects
pub type Handle = u32;

/// File open modes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OpenMode {
    ReadOnly = 0,
    WriteOnly = 1,
    Append = 2,
    ReadWrite = 3,
}

/// Console kinds
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConsoleKind {
    Stdin,
    Stdout,
    Stderr,
}

/// Console kernel object
#[derive(Debug)]
pub struct ConsoleObject {
    pub kind: ConsoleKind,
}

/// File kernel object
#[derive(Debug)]
pub struct FileObject {
    pub path: String,
    pub mode: OpenMode,
    pub position: u64,
    pub fs_id: u32,
}

/// Kernel objects that can be stored in handle tables
#[derive(Debug)]
pub enum KernelObject {
    File(FileObject),
    Console(ConsoleObject),
}

/// Per-process handle table
#[derive(Debug)]
pub struct HandleTable {
    objects: BTreeMap<Handle, KernelObject>,
    next_handle: Handle,
}

impl HandleTable {
    pub fn new() -> Self {
        HandleTable {
            objects: BTreeMap::new(),
            next_handle: 1,
        }
    }

    fn allocate_handle(&mut self) -> Handle {
        let handle = self.next_handle;
        self.next_handle += 1;
        handle
    }

    pub fn add_console_handle(&mut self, kind: ConsoleKind) -> Handle {
        let handle = self.allocate_handle();
        self.objects.insert(handle, KernelObject::Console(ConsoleObject { kind }));
        handle
    }

    pub fn open_file(&mut self, path: &str, mode: OpenMode, fs_id: u32) -> Handle {
        let handle = self.allocate_handle();
        self.objects.insert(handle, KernelObject::File(FileObject {
            path: String::from(path),
            mode,
            position: 0,
            fs_id,
        }));
        handle
    }

    pub fn close(&mut self, handle: Handle) -> bool {
        self.objects.remove(&handle).is_some()
    }

    pub fn get(&self, handle: Handle) -> Option<&KernelObject> {
        self.objects.get(&handle)
    }

    pub fn get_mut(&mut self, handle: Handle) -> Option<&mut KernelObject> {
        self.objects.get_mut(&handle)
    }

    pub fn get_file(&self, handle: Handle) -> Option<&FileObject> {
        match self.objects.get(&handle) {
            Some(KernelObject::File(f)) => Some(f),
            _ => None,
        }
    }

    pub fn get_file_mut(&mut self, handle: Handle) -> Option<&mut FileObject> {
        match self.objects.get_mut(&handle) {
            Some(KernelObject::File(f)) => Some(f),
            _ => None,
        }
    }

    pub fn get_console(&self, handle: Handle) -> Option<&ConsoleObject> {
        match self.objects.get(&handle) {
            Some(KernelObject::Console(c)) => Some(c),
            _ => None,
        }
    }
}

impl Default for HandleTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Process state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProcessState {
    Ready,
    Running,
    Terminated(i32),
}

/// Process control block
pub struct Process {
    pub id: u32,
    pub name: String,
    pub args: String,  // Command line arguments
    pub state: ProcessState,
    pub entry_point: u64,
    pub stack_top: u64,
    pub heap_base: u64,
    pub heap_size: usize,
    pub page_table: ProcessPageTable,
    pub handle_table: HandleTable,
}

const MAX_PROCESSES: usize = 16;
static mut PROCESSES: [Option<Process>; MAX_PROCESSES] = [
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
];
static mut NEXT_PID: u32 = 1;
static mut CURRENT_PROCESS: Option<u32> = None;
static mut KERNEL_PML4: u64 = 0;

// Process memory layout (per process):
//   base + 0x000000: Code/data (up to 1MB)
//   base + 0x100000: Heap (1MB)
//   base + 0x1F0000: Stack (64KB, grows down from base + 0x200000)
// Total: 2MB per process, with 1.5MB gap before next process's code
const PROCESS_MEM_BASE: u64 = 0x1000000;
const PROCESS_MEM_SIZE: u64 = 0x400000;  // 4MB spacing between processes
const PROCESS_STACK_SIZE: u64 = 0x100000; // 1MB stack

static mut KERNEL_RSP: u64 = 0;
static mut KERNEL_RBP: u64 = 0;

/// Saved parent context for exec-and-wait semantics
/// Includes the full interrupt frame from the syscall
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SavedContext {
    pub valid: bool,
    pub parent_pid: u32,
    pub parent_pml4: u64,
    // Interrupt frame values (what IRETQ needs)
    pub rip: u64,    // Return address
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,    // User stack pointer at time of syscall
    pub ss: u64,
}

static mut PARENT_CONTEXT: SavedContext = SavedContext {
    valid: false,
    parent_pid: 0,
    parent_pml4: 0,
    rip: 0,
    cs: 0,
    rflags: 0,
    rsp: 0,
    ss: 0,
};

/// Save the current process context before spawning a child
/// The return_* parameters are from the interrupt frame on the kernel stack
pub fn save_parent_context_with_frame(return_rip: u64, return_rsp: u64) {
    unsafe {
        if let Some(pid) = CURRENT_PROCESS {
            if let Some(proc) = PROCESSES.iter().find_map(|p| p.as_ref().filter(|p| p.id == pid)) {
                let user_cs = watos_arch::gdt::selectors::USER_CODE as u64;
                let user_ss = watos_arch::gdt::selectors::USER_DATA as u64;
                let parent_pml4 = proc.page_table.pml4_phys_addr();

                PARENT_CONTEXT = SavedContext {
                    valid: true,
                    parent_pid: pid,
                    parent_pml4,
                    rip: return_rip,
                    cs: user_cs,
                    rflags: 0x202, // IF set
                    rsp: return_rsp,
                    ss: user_ss,
                };

                debug_serial(b"[PROCESS] Saved parent context, PID=");
                debug_hex(pid as u64);
                debug_serial(b" RIP=");
                debug_hex(return_rip);
                debug_serial(b" RSP=");
                debug_hex(return_rsp);
                debug_serial(b" PML4=");
                debug_hex(parent_pml4);
                debug_serial(b"\r\n");
            }
        }
    }
}

/// Legacy function - uses entry point as return address (restarts parent)
pub fn save_parent_context() {
    unsafe {
        if let Some(pid) = CURRENT_PROCESS {
            if let Some(proc) = PROCESSES.iter().find_map(|p| p.as_ref().filter(|p| p.id == pid)) {
                let user_cs = watos_arch::gdt::selectors::USER_CODE as u64;
                let user_ss = watos_arch::gdt::selectors::USER_DATA as u64;

                PARENT_CONTEXT = SavedContext {
                    valid: true,
                    parent_pid: pid,
                    parent_pml4: proc.page_table.pml4_phys_addr(),
                    rip: proc.entry_point,
                    cs: user_cs,
                    rflags: 0x202,
                    rsp: proc.stack_top - 8,
                    ss: user_ss,
                };
                debug_serial(b"[PROCESS] Saved parent context (restart mode), PID=");
                debug_hex(pid as u64);
                debug_serial(b"\r\n");
            }
        }
    }
}

/// Check if there's a parent to return to
pub fn has_parent_context() -> bool {
    unsafe { PARENT_CONTEXT.valid }
}

/// Resume the parent process after child exits
pub fn resume_parent() -> ! {
    unsafe {
        if !PARENT_CONTEXT.valid {
            debug_serial(b"[PROCESS] ERROR: No parent to resume, halting\r\n");
            loop { core::arch::asm!("hlt"); }
        }

        let ctx = PARENT_CONTEXT;
        PARENT_CONTEXT.valid = false;
        CURRENT_PROCESS = Some(ctx.parent_pid);

        debug_serial(b"[PROCESS] Resuming parent PID=");
        debug_hex(ctx.parent_pid as u64);
        debug_serial(b" RIP=");
        debug_hex(ctx.rip);
        debug_serial(b" RSP=");
        debug_hex(ctx.rsp);
        debug_serial(b" PML4=");
        debug_hex(ctx.parent_pml4);
        debug_serial(b"\r\n");

        // Restore parent's kernel stack for interrupts
        const KERNEL_STACK_SIZE: usize = 0x10000;
        let kernel_stack_base = 0x280000 + (ctx.parent_pid as usize * KERNEL_STACK_SIZE);
        let kernel_stack_top = kernel_stack_base + KERNEL_STACK_SIZE;
        watos_arch::tss::set_kernel_stack(kernel_stack_top as u64);

        // Switch back to parent's page table
        watos_mem::paging::load_cr3(ctx.parent_pml4);

        let user_ds = watos_arch::gdt::selectors::USER_DATA as u64;

        // Return to parent with exec success (RAX = 0)
        core::arch::asm!(
            // Set up segments
            "mov ax, {ds:x}",
            "mov ds, ax",
            "mov es, ax",
            "mov fs, ax",
            "mov gs, ax",

            // Set RAX to 0 (exec success)
            "xor rax, rax",

            // Set up IRETQ frame with saved values
            "push {ss}",        // SS
            "push {rsp}",       // RSP (user stack at time of syscall)
            "push {rflags}",    // RFLAGS
            "push {cs}",        // CS
            "push {rip}",       // RIP (return address after syscall)

            "iretq",

            ds = in(reg) user_ds as u16,
            ss = in(reg) ctx.ss,
            rsp = in(reg) ctx.rsp,
            rflags = in(reg) ctx.rflags,
            cs = in(reg) ctx.cs,
            rip = in(reg) ctx.rip,
            options(noreturn)
        );
    }
}

fn allocate_process_memory(pid: u32) -> (u64, u64, u64) {
    let base = PROCESS_MEM_BASE + (pid as u64 - 1) * PROCESS_MEM_SIZE;
    let stack_top = base + PROCESS_MEM_SIZE;
    let heap_base = base + 0x80000;
    (base, stack_top, heap_base)
}

/// Load and execute an ELF64 binary
/// args is the full command line (program name + arguments)
pub fn exec(name: &str, data: &[u8], args: &str) -> Result<u32, &'static str> {
    // CRITICAL: Copy name and args BEFORE switching page tables!
    // The strings may point to user space which becomes invalid after CR3 switch.
    let name_copy = String::from(name);
    let args_copy = String::from(args);

    // CRITICAL: Switch to kernel page table before loading
    // When called from user process, CR3 points to that process's page table
    // which only has limited mappings. Kernel's UEFI page table has full identity map.
    unsafe {
        if KERNEL_PML4 != 0 {
            watos_mem::paging::load_cr3(KERNEL_PML4);
        }
    }

    let elf = elf::Elf64::parse(data)?;

    let pid = unsafe {
        let p = NEXT_PID;
        NEXT_PID += 1;
        p
    };

    let (load_base, stack_top, heap_base) = allocate_process_memory(pid);

    let mut page_table = ProcessPageTable::new();
    elf.load_segments_protected(data, load_base, &mut page_table)?;

    // Map stack with guard pages for overflow detection
    // Stack layout: [GUARD PAGE] [STACK PAGES...] [TOP]
    // Guard page is mapped but NOT present - accessing it causes page fault
    let stack_pages = PROCESS_STACK_SIZE / PAGE_SIZE as u64;
    let stack_base = stack_top - PROCESS_STACK_SIZE;
    let guard_page = stack_base - PAGE_SIZE as u64;

    for i in 0..stack_pages {
        let virt_addr = stack_top - (i as u64 + 1) * PAGE_SIZE as u64;
        let phys_addr = watos_mem::phys::alloc_page()
            .ok_or("Out of physical memory for stack")? as u64;
        unsafe { core::ptr::write_bytes(phys_addr as *mut u8, 0, PAGE_SIZE); }
        page_table.map_user_page(virt_addr, phys_addr,
            page_flags::PRESENT | page_flags::WRITABLE)?;
    }

    // Map guard page as NOT PRESENT - will trigger page fault on stack overflow
    page_table.map_user_page(guard_page, 0, 0)?;

    // Allocate a reasonable initial heap (256KB)
    let heap_pages = 64u64;
    for i in 0..heap_pages {
        let virt_addr = heap_base + i * PAGE_SIZE as u64;
        let phys_addr = watos_mem::phys::alloc_page()
            .ok_or("Out of physical memory for heap")? as u64;
        unsafe { core::ptr::write_bytes(phys_addr as *mut u8, 0, PAGE_SIZE); }
        page_table.map_user_page(virt_addr, phys_addr,
            page_flags::PRESENT | page_flags::WRITABLE)?;
    }

    let min_vaddr = elf.phdrs.iter()
        .filter(|p| p.ptype == elf::PT_LOAD)
        .map(|p| p.vaddr)
        .min()
        .unwrap_or(0);

    // For non-PIE: use ELF entry directly (absolute address)
    // For PIE: relocate entry relative to load_base
    let entry = if elf.is_pie {
        load_base + (elf.entry - min_vaddr)
    } else {
        elf.entry
    };

    // Map framebuffer for user access (from boot info at 0x80000)
    unsafe {
        let boot_info = &*(0x80000 as *const BootInfo);
        if boot_info.magic == 0x5741544F && boot_info.framebuffer_addr != 0 {
            let fb_addr = boot_info.framebuffer_addr;
            let fb_size = (boot_info.framebuffer_pitch * boot_info.framebuffer_height) as u64;
            let fb_pages = (fb_size + PAGE_SIZE as u64 - 1) / PAGE_SIZE as u64;

            for i in 0..fb_pages {
                let addr = fb_addr + i * PAGE_SIZE as u64;
                page_table.map_4k_page(addr, addr,
                    page_flags::PRESENT | page_flags::WRITABLE | page_flags::USER);
            }
        }
    }

    let process = Process {
        id: pid,
        name: name_copy,
        args: args_copy.clone(),
        state: ProcessState::Ready,
        entry_point: entry,
        stack_top,
        heap_base,
        heap_size: (heap_pages as usize) * PAGE_SIZE,
        page_table,
        handle_table: HandleTable::new(),
    };

    // Debug: show what args are being stored
    unsafe {
        debug_serial(b"[PROCESS] exec: storing args='");
        debug_serial(args_copy.as_bytes());
        debug_serial(b"' len=");
        debug_hex(args_copy.len() as u64);
        debug_serial(b"\r\n");
    }

    unsafe {
        for slot in PROCESSES.iter_mut() {
            if slot.is_none() {
                *slot = Some(process);
                break;
            }
        }
    }

    run_process(pid)?;

    Ok(pid)
}

fn run_process(pid: u32) -> Result<(), &'static str> {
    let (entry, stack_top, pml4_addr) = unsafe {
        let proc = PROCESSES.iter()
            .find_map(|p| p.as_ref().filter(|p| p.id == pid))
            .ok_or("Process not found")?;

        CURRENT_PROCESS = Some(pid);
        (proc.entry_point, proc.stack_top, proc.page_table.pml4_phys_addr())
    };

    unsafe {
        debug_serial(b"[PROCESS] Starting PID=");
        debug_hex(pid as u64);
        debug_serial(b" in Ring 3 user mode\r\n");
        debug_serial(b"[PROCESS] Entry=0x");
        debug_hex(entry);
        debug_serial(b" Stack=0x");
        debug_hex(stack_top);
        debug_serial(b" PML4=0x");
        debug_hex(pml4_addr);
        debug_serial(b"\r\n");
    }

    unsafe {
        // Save the current kernel stack so we can return on process exit
        let mut saved_rsp: u64 = 0;
        let mut saved_rbp: u64 = 0;
        core::arch::asm!(
            "mov {}, rsp",
            "mov {}, rbp",
            out(reg) saved_rsp,
            out(reg) saved_rbp,
            options(nostack, preserves_flags)
        );
        KERNEL_RSP = saved_rsp;
        KERNEL_RBP = saved_rbp;

        const KERNEL_STACK_SIZE: usize = 0x10000;
        let kernel_stack_base = 0x280000 + (pid as usize * KERNEL_STACK_SIZE);
        let kernel_stack_top = kernel_stack_base + KERNEL_STACK_SIZE;

        debug_serial(b"[PROCESS] Kernel stack for interrupts: 0x");
        debug_hex(kernel_stack_top as u64);
        debug_serial(b"\r\n");

        watos_arch::tss::set_kernel_stack(kernel_stack_top as u64);
    }

    unsafe {
        debug_serial(b"[PROCESS] Dumping PML4 entry 0...\r\n");

        // Get actual PML4 pointer and dump first entry
        let pml4_ptr = pml4_addr as *const u64;
        let pml4_0 = *pml4_ptr;
        debug_serial(b"  PML4[0]=0x");
        debug_hex(pml4_0);
        debug_serial(b"\r\n");

        // Follow to PDP
        if pml4_0 & 1 != 0 {
            let pdp_addr = pml4_0 & 0x000F_FFFF_FFFF_F000;
            let pdp_ptr = pdp_addr as *const u64;
            let pdp_0 = *pdp_ptr;
            debug_serial(b"  PDP[0]=0x");
            debug_hex(pdp_0);
            debug_serial(b"\r\n");

            // Follow to PD
            if pdp_0 & 1 != 0 {
                let pd_addr = pdp_0 & 0x000F_FFFF_FFFF_F000;
                let pd_ptr = pd_addr as *const u64;
                let pd_0 = *pd_ptr;
                debug_serial(b"  PD[0]=0x");
                debug_hex(pd_0);
                debug_serial(b" (huge=");
                if pd_0 & 0x80 != 0 { debug_serial(b"Y"); } else { debug_serial(b"N"); }
                debug_serial(b")\r\n");

                // Also dump PD[8] (for code at 0x1000000) and PD[9] (for stack at 0x13FF000)
                let pd_8 = *pd_ptr.add(8);
                debug_serial(b"  PD[8]=0x");
                debug_hex(pd_8);
                if pd_8 & 1 != 0 {
                    // Follow to PT
                    let pt_addr = pd_8 & 0x000F_FFFF_FFFF_F000;
                    let pt_ptr = pt_addr as *const u64;
                    let pt_0 = *pt_ptr;
                    debug_serial(b" -> PT[0]=0x");
                    debug_hex(pt_0);
                    // PT[43] is for virtual 0x102B000 (GOT page)
                    let pt_43 = *pt_ptr.add(43);
                    debug_serial(b" PT[43]=0x");
                    debug_hex(pt_43);
                }
                debug_serial(b"\r\n");

                let pd_9 = *pd_ptr.add(9);
                debug_serial(b"  PD[9]=0x");
                debug_hex(pd_9);
                if pd_9 & 1 != 0 {
                    // Follow to PT for stack page
                    let pt_addr = pd_9 & 0x000F_FFFF_FFFF_F000;
                    let pt_ptr = pt_addr as *const u64;
                    // Stack at 0x13FF000 -> PT index 511
                    let pt_511 = *pt_ptr.add(511);
                    debug_serial(b" -> PT[511]=0x");
                    debug_hex(pt_511);
                }
                debug_serial(b"\r\n");
            }
        }

        debug_serial(b"[PROCESS] Switching to user page table...\r\n");

        // Switch to a kernel stack within the mapped 8MB region
        // This is necessary because CR3 switch will make our current stack inaccessible
        let mapped_kernel_stack: u64 = 0x5FF000;  // Within first 8MB, mapped in process page table
        debug_serial(b"[DEBUG] Switching to mapped kernel stack at 0x5FF000\r\n");
        core::arch::asm!(
            "mov rsp, {stack}",
            "mov rbp, {stack}",
            stack = in(reg) mapped_kernel_stack,
            options(nostack)
        );

        // Switch to user page table before IRETQ
        debug_serial(b"[DEBUG] Switching to user page table (CR3)\r\n");
        watos_mem::paging::load_cr3(pml4_addr);

        // Print OK and RSP in pure asm - no Rust stack usage at all
        core::arch::asm!(
            "mov dx, 0x3F8",
            "mov al, 0x4F", "out dx, al",  // 'O'
            "mov al, 0x4B", "out dx, al",  // 'K'
            "mov al, 0x20", "out dx, al",  // space
            "mov al, 0x52", "out dx, al",  // 'R'
            "mov al, 0x53", "out dx, al",  // 'S'
            "mov al, 0x50", "out dx, al",  // 'P'
            "mov al, 0x3D", "out dx, al",  // '='

            // Print RSP as 16 hex digits
            "mov rsi, rsp",        // save RSP value
            "mov rcx, 60",         // start shift at 60 (15*4)
            "2:",
            "mov rax, rsi",
            "shr rax, cl",
            "and al, 0x0F",
            "add al, 0x30",        // '0'
            "cmp al, 0x3A",
            "jl 3f",
            "add al, 7",           // 'A'-'0'-10
            "3:",
            "out dx, al",
            "sub rcx, 4",
            "jns 2b",              // loop while shift >= 0

            "mov al, 0x0D", "out dx, al",  // CR
            "mov al, 0x0A", "out dx, al",  // LF
            options(nostack, nomem, preserves_flags)
        );
    }

    unsafe {
        // Print verification message using pure asm to avoid any stack issues
        debug_serial(b"[PROCESS] Verifying user code...\r\n");

        // Read first byte from entry point to verify it's accessible
        let first_byte = *(entry as *const u8);
        debug_serial(b"[PROCESS] First byte: 0x");
        let hex = b"0123456789ABCDEF";
        core::arch::asm!("out dx, al", in("dx") 0x3F8_u16, in("al") hex[(first_byte >> 4) as usize], options(nostack));
        core::arch::asm!("out dx, al", in("dx") 0x3F8_u16, in("al") hex[(first_byte & 0xF) as usize], options(nostack));
        debug_serial(b" OK\r\n");

        // Skip stack test after CR3 switch - calling debug_hex uses the Rust stack
        // which may cause issues after we switched stacks
        debug_serial(b"[PROCESS] Skipping stack test after CR3 switch\r\n");
    }

    unsafe {
        debug_serial(b"[PROCESS] Transitioning to Ring 3...\r\n");

        // Prepare IRETQ values - use constants to avoid stack-heavy operations
        let user_stack = stack_top - 8;
        let user_cs = watos_arch::gdt::selectors::USER_CODE as u64;
        let user_ds = watos_arch::gdt::selectors::USER_DATA as u64;
        let rflags: u64 = 0x202;

        debug_serial(b"[PROCESS] Executing IRETQ to Ring 3...\r\n");

        // Build the IRETQ frame using PUSH instructions on current stack
        // This is the traditional and most reliable approach
        // Stack grows down, so push in reverse order: SS, RSP, RFLAGS, CS, RIP
        core::arch::asm!(
            // Disable interrupts
            "cli",

            // Push IRETQ frame in reverse order (stack grows down)
            "push {ss}",      // SS
            "push {rsp_val}", // RSP
            "push {rflags}",  // RFLAGS
            "push {cs}",      // CS
            "push {rip}",     // RIP

            // Debug: Print the RSP value before IRETQ
            "mov dx, 0x3F8",
            "mov al, 0x49", // 'I'
            "out dx, al",
            "mov al, 0x52", // 'R'
            "out dx, al",
            "mov al, 0x45", // 'E'
            "out dx, al",
            "mov al, 0x54", // 'T'
            "out dx, al",
            "mov al, 0x51", // 'Q'
            "out dx, al",
            "mov al, 0x0D",
            "out dx, al",
            "mov al, 0x0A",
            "out dx, al",

            // Zero GPRs (except RSP which points to the frame)
            "xor rax, rax",
            "xor rbx, rbx",
            "xor rcx, rcx",
            "xor rdx, rdx",
            "xor rsi, rsi",
            "xor rdi, rdi",
            "xor rbp, rbp",
            "xor r8, r8",
            "xor r9, r9",
            "xor r10, r10",
            "xor r11, r11",
            "xor r12, r12",
            "xor r13, r13",
            "xor r14, r14",
            "xor r15, r15",

            // IRETQ - reads frame from [RSP]
            "iretq",
            ss = in(reg) user_ds,
            rsp_val = in(reg) user_stack,
            rflags = in(reg) rflags,
            cs = in(reg) user_cs,
            rip = in(reg) entry,
            options(noreturn)
        );
    }
}

#[no_mangle]
pub extern "C" fn process_exit_to_kernel(code: i32) -> ! {
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
            // Drop the process slot now that it has exited to release resources
            for slot in PROCESSES.iter_mut() {
                if let Some(ref p) = slot {
                    if p.id == pid {
                        *slot = None;
                        break;
                    }
                }
            }
            CURRENT_PROCESS = None;
        }

        // Always restore kernel paging before returning to the kernel stack
        restore_kernel_paging();

        // Clear the saved parent context to avoid stale resumes
        PARENT_CONTEXT.valid = false;

        if KERNEL_RSP != 0 {
            core::arch::asm!(
                "mov rsp, {rsp}",
                "mov rbp, {rbp}",
                "ret",
                rsp = in(reg) KERNEL_RSP,
                rbp = in(reg) KERNEL_RBP,
                options(noreturn)
            );
        }

        debug_serial(b"ERROR: process_exit_to_kernel failed\r\n");
        CURRENT_PROCESS = None;
        loop {
            core::arch::asm!("hlt", options(nostack, nomem));
        }
    }
}

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

pub fn current_pid() -> Option<u32> {
    unsafe { CURRENT_PROCESS }
}

pub fn current_handle_table() -> Option<&'static mut HandleTable> {
    unsafe {
        if let Some(pid) = CURRENT_PROCESS {
            for slot in PROCESSES.iter_mut() {
                if let Some(ref mut p) = slot {
                    if p.id == pid {
                        return Some(&mut p.handle_table);
                    }
                }
            }
        }
        None
    }
}

/// Get the arguments for the current process
/// Returns the number of bytes copied into the buffer
pub fn get_current_args(buf: &mut [u8]) -> usize {
    unsafe {
        debug_serial(b"[PROCESS] get_current_args: CURRENT_PROCESS=");
        if let Some(pid) = CURRENT_PROCESS {
            debug_hex(pid as u64);
            debug_serial(b"\r\n");
            for slot in PROCESSES.iter() {
                if let Some(ref p) = slot {
                    if p.id == pid {
                        let args = p.args.as_bytes();
                        debug_serial(b"[PROCESS] Found process args='");
                        debug_serial(args);
                        debug_serial(b"' len=");
                        debug_hex(args.len() as u64);
                        debug_serial(b"\r\n");
                        let copy_len = args.len().min(buf.len());
                        buf[..copy_len].copy_from_slice(&args[..copy_len]);
                        return copy_len;
                    }
                }
            }
            debug_serial(b"[PROCESS] Process not found in table!\r\n");
        } else {
            debug_serial(b"None\r\n");
        }
        0
    }
}

pub fn init() {
    unsafe {
        KERNEL_PML4 = watos_mem::paging::get_cr3();
        debug_serial(b"Process subsystem initialized, kernel PML4: 0x");
        debug_hex(KERNEL_PML4);
        debug_serial(b"\r\n");
    }
}

unsafe fn restore_kernel_paging() {
    if KERNEL_PML4 != 0 {
        debug_serial(b"Restoring kernel page table...\r\n");
        watos_mem::paging::load_cr3(KERNEL_PML4);
    }
}

/// Get the kernel's PML4 address for CR3 switching during syscalls
/// Returns 0 if not initialized
pub fn get_kernel_pml4() -> u64 {
    unsafe { KERNEL_PML4 }
}

pub fn cleanup() {
    unsafe {
        restore_kernel_paging();

        for slot in PROCESSES.iter_mut() {
            if let Some(ref p) = slot {
                if matches!(p.state, ProcessState::Terminated(_)) {
                    *slot = None;
                }
            }
        }
    }
}
