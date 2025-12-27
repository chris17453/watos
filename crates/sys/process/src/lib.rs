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

// Process memory starts at 16MB, outside kernel's 8MB identity-mapped region
const PROCESS_MEM_BASE: u64 = 0x1000000;
const PROCESS_MEM_SIZE: u64 = 0x100000;  // 1MB per process
const PROCESS_STACK_SIZE: u64 = 0x10000; // 64KB stack

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

                PARENT_CONTEXT = SavedContext {
                    valid: true,
                    parent_pid: pid,
                    parent_pml4: proc.page_table.pml4_phys_addr(),
                    rip: return_rip,
                    cs: user_cs,
                    rflags: 0x202, // IF set
                    rsp: return_rsp,
                    ss: user_ss,
                };
                debug_serial(b"[PROCESS] Saved parent context, PID=");
                debug_hex(pid as u64);
                debug_serial(b" RIP=0x");
                debug_hex(return_rip);
                debug_serial(b" RSP=0x");
                debug_hex(return_rsp);
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
            debug_serial(b"[PROCESS] No parent to resume, halting\r\n");
            loop { core::arch::asm!("hlt"); }
        }

        let ctx = PARENT_CONTEXT;
        PARENT_CONTEXT.valid = false;
        CURRENT_PROCESS = Some(ctx.parent_pid);

        debug_serial(b"[PROCESS] Resuming parent PID=");
        debug_hex(ctx.parent_pid as u64);
        debug_serial(b" PML4=0x");
        debug_hex(ctx.parent_pml4);
        debug_serial(b" RIP=0x");
        debug_hex(ctx.rip);
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
        // Use the saved interrupt frame values
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
pub fn exec(name: &str, data: &[u8]) -> Result<u32, &'static str> {
    // CRITICAL: Switch to kernel page table before loading
    // When called from user process, CR3 points to that process's page table
    // which only has limited mappings. Kernel's UEFI page table has full identity map.
    unsafe {
        if KERNEL_PML4 != 0 {
            debug_serial(b"exec: switching to kernel page table for loading...\r\n");
            watos_mem::paging::load_cr3(KERNEL_PML4);
        }
        debug_serial(b"exec: parsing ELF...\r\n");
    }

    let elf = elf::Elf64::parse(data)?;

    unsafe {
        debug_serial(b"exec: ELF parsed, entry=0x");
        debug_hex(elf.entry);
        debug_serial(b"\r\n");
    }

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

    unsafe { debug_serial(b"exec: creating page table...\r\n"); }
    let mut page_table = ProcessPageTable::new();

    unsafe { debug_serial(b"exec: loading segments...\r\n"); }
    elf.load_segments_protected(data, load_base, &mut page_table)?;
    unsafe { debug_serial(b"exec: segments loaded\r\n"); }

    unsafe { debug_serial(b"exec: mapping stack...\r\n"); }
    let stack_pages = PROCESS_STACK_SIZE / PAGE_SIZE as u64;
    for i in 0..stack_pages {
        let virt_addr = stack_top - (i as u64 + 1) * PAGE_SIZE as u64;
        let phys_addr = stack_top - (i as u64 + 1) * PAGE_SIZE as u64;
        page_table.map_user_page(virt_addr, phys_addr,
            page_flags::PRESENT | page_flags::WRITABLE)?;
    }

    unsafe { debug_serial(b"exec: mapping heap...\r\n"); }
    let heap_pages = (PROCESS_MEM_SIZE - PROCESS_STACK_SIZE - 0x80000) / PAGE_SIZE as u64;
    for i in 0..heap_pages {
        let virt_addr = heap_base + i as u64 * PAGE_SIZE as u64;
        let phys_addr = heap_base + i as u64 * PAGE_SIZE as u64;
        page_table.map_user_page(virt_addr, phys_addr,
            page_flags::PRESENT | page_flags::WRITABLE)?;
    }

    let min_vaddr = elf.phdrs.iter()
        .filter(|p| p.ptype == elf::PT_LOAD)
        .map(|p| p.vaddr)
        .min()
        .unwrap_or(0);
    let entry = load_base + (elf.entry - min_vaddr);

    // Map framebuffer for user access (from boot info at 0x80000)
    // The framebuffer is typically at ~2GB (0x80000000) with size ~4MB
    unsafe {
        let boot_info = &*(0x80000 as *const BootInfo);
        if boot_info.magic == 0x5741544F && boot_info.framebuffer_addr != 0 {
            let fb_addr = boot_info.framebuffer_addr;
            let fb_size = (boot_info.framebuffer_pitch * boot_info.framebuffer_height) as u64;
            let fb_pages = (fb_size + PAGE_SIZE as u64 - 1) / PAGE_SIZE as u64;

            debug_serial(b"exec: mapping framebuffer at 0x");
            debug_hex(fb_addr);
            debug_serial(b" (");
            debug_hex(fb_pages);
            debug_serial(b" pages)\r\n");

            // Map in chunks to reduce debugging output
            for i in 0..fb_pages {
                let addr = fb_addr + i * PAGE_SIZE as u64;
                // Map framebuffer with USER + WRITABLE flags
                // Use map_4k_page directly since map_user_page rejects addresses > USER_SPACE_MAX
                page_table.map_4k_page(addr, addr,
                    page_flags::PRESENT | page_flags::WRITABLE | page_flags::USER);
            }
            debug_serial(b"exec: framebuffer mapped OK\r\n");
        }
    }

    let process = Process {
        id: pid,
        name: String::from(name),
        state: ProcessState::Ready,
        entry_point: entry,
        stack_top,
        heap_base,
        heap_size: heap_pages as usize * PAGE_SIZE,
        page_table,
        handle_table: HandleTable::new(),
    };

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
            }
        }

        debug_serial(b"[PROCESS] Switching to user page table...\r\n");

        // CRITICAL: Switch to a kernel stack within our mapped region (0-8MB)
        // The UEFI stack at ~267MB won't be mapped after CR3 switch!
        // Use a stack area at 0x5FF000 (just below 6MB, page-aligned, in our mapped range)
        let mapped_kernel_stack: u64 = 0x5FF000;
        core::arch::asm!(
            "mov rsp, {stack}",
            "mov rbp, {stack}",
            stack = in(reg) mapped_kernel_stack,
            options(nostack)
        );

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
    }

    unsafe {
        debug_serial(b"[PROCESS] Transitioning to Ring 3...\r\n");

        let user_stack = stack_top - 8;
        let user_cs = watos_arch::gdt::selectors::USER_CODE as u64;
        let user_ds = watos_arch::gdt::selectors::USER_DATA as u64;
        let rflags: u64 = 0x202;

        // Debug: print the values we'll use
        debug_serial(b"  CS=0x");
        debug_hex(user_cs);
        debug_serial(b" DS=0x");
        debug_hex(user_ds);
        debug_serial(b"\r\n  Entry=0x");
        debug_hex(entry);
        debug_serial(b" Stack=0x");
        debug_hex(user_stack);
        debug_serial(b"\r\n");

        core::arch::asm!(
            "mov ds, {0:x}",
            "mov es, {0:x}",
            "mov fs, {0:x}",
            "mov gs, {0:x}",
            in(reg) user_ds,
        );

        core::arch::asm!(
            "push {user_ss}",
            "push {user_rsp}",
            "push {rflags}",
            "push {user_cs}",
            "push {entry}",
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
            "iretq",
            user_ss = in(reg) user_ds,
            user_rsp = in(reg) user_stack,
            rflags = in(reg) rflags,
            user_cs = in(reg) user_cs,
            entry = in(reg) entry,
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
            CURRENT_PROCESS = None;
        }

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
