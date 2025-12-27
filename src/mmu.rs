//! Memory Management Unit (MMU) - Minimal paging for user/kernel separation
//!
//! Provides per-process page tables for ELF64 execution with user/kernel isolation.

use alloc::vec::Vec;
use alloc::boxed::Box;

/// Page size (4KB)
pub const PAGE_SIZE: usize = 0x1000;

/// Kernel space starts at -2GB (0xFFFFFFFF80000000)
pub const KERNEL_SPACE_START: u64 = 0xFFFFFFFF80000000;

/// User space maximum (first 2GB)
pub const USER_SPACE_MAX: u64 = 0x80000000;

/// Page table entry flags
#[allow(dead_code)]
pub mod page_flags {
    pub const PRESENT: u64 = 1 << 0;
    pub const WRITABLE: u64 = 1 << 1;
    pub const USER: u64 = 1 << 2;
    pub const WRITE_THROUGH: u64 = 1 << 3;
    pub const NO_CACHE: u64 = 1 << 4;
    pub const ACCESSED: u64 = 1 << 5;
    pub const DIRTY: u64 = 1 << 6;
    pub const HUGE_PAGE: u64 = 1 << 7;
    pub const GLOBAL: u64 = 1 << 8;
    pub const NO_EXECUTE: u64 = 1 << 63;
}

/// 4-level page table structure (PML4 -> PDP -> PD -> PT)
#[repr(align(4096))]
pub struct PageTable {
    entries: [u64; 512],
}

impl PageTable {
    pub fn new() -> Self {
        PageTable {
            entries: [0; 512],
        }
    }

    pub fn get_entry(&self, index: usize) -> u64 {
        self.entries[index]
    }

    pub fn set_entry(&mut self, index: usize, entry: u64) {
        self.entries[index] = entry;
    }

    pub fn physical_addr(&self) -> u64 {
        self as *const _ as u64
    }
}

/// Per-process page table manager
pub struct ProcessPageTable {
    pml4: PageTable,
    allocated_tables: Vec<*mut PageTable>,
}

impl ProcessPageTable {
    /// Create new page table for a process
    pub fn new() -> Self {
        let mut ppt = ProcessPageTable {
            pml4: PageTable::new(),
            allocated_tables: Vec::new(),
        };

        // Map kernel space (identity mapping for kernel code/data)
        // This ensures kernel remains accessible in user processes
        ppt.map_kernel_space();

        ppt
    }

    /// Map kernel space into process page table (shared across all processes)
    /// 
    /// For Ring 3 user mode, kernel must be mapped but NOT writable from user mode:
    /// - Kernel code/data: Read-only from user (no USER flag = kernel-only access)
    /// - This allows kernel to remain accessible for interrupts/syscalls
    /// - User code cannot write to kernel memory (enforced by CPU)
    fn map_kernel_space(&mut self) {
        // Kernel flags: Present, Writable, but NO USER flag
        // This makes kernel accessible in Ring 0 but NOT Ring 3
        let kernel_flags = page_flags::PRESENT | page_flags::WRITABLE | page_flags::GLOBAL;
        
        // Map first 1GB of physical memory in TWO locations:
        // 1. Identity mapping (virt = phys) - for kernel execution
        // 2. High virtual addresses - canonical kernel space
        // 
        // NOTE: Without USER flag, these are kernel-only mappings
        // User code at Ring 3 cannot access these addresses
        for i in 0..512 {  // 512 * 2MB = 1GB
            let phys_addr = i * 0x200000; // 2MB increments
            
            // Identity mapping for kernel compatibility
            self.map_large_page(phys_addr, phys_addr, kernel_flags);
            
            // High virtual mapping for canonical kernel space  
            let high_virt_addr = KERNEL_SPACE_START + phys_addr;
            self.map_large_page(high_virt_addr, phys_addr, kernel_flags);
        }
    }

    /// Map a 2MB large page
    fn map_large_page(&mut self, virt_addr: u64, phys_addr: u64, flags: u64) {
        let pml4_index = ((virt_addr >> 39) & 0x1FF) as usize;
        let pdp_index = ((virt_addr >> 30) & 0x1FF) as usize;
        let pd_index = ((virt_addr >> 21) & 0x1FF) as usize;

        // Get or create PDP table
        if self.pml4.get_entry(pml4_index) == 0 {
            let pdp_table = self.allocate_table();
            let pdp_phys = pdp_table as *const _ as u64;
            self.pml4.set_entry(pml4_index, pdp_phys | page_flags::PRESENT | page_flags::WRITABLE);
        }

        let pdp_phys = self.pml4.get_entry(pml4_index) & 0xFFFFFFFFFFFFF000;
        let pdp_table = unsafe { &mut *(pdp_phys as *mut PageTable) };

        // Get or create PD table
        if pdp_table.get_entry(pdp_index) == 0 {
            let pd_table = self.allocate_table();
            let pd_phys = pd_table as *const _ as u64;
            pdp_table.set_entry(pdp_index, pd_phys | page_flags::PRESENT | page_flags::WRITABLE);
        }

        let pd_phys = pdp_table.get_entry(pdp_index) & 0xFFFFFFFFFFFFF000;
        let pd_table = unsafe { &mut *(pd_phys as *mut PageTable) };

        // Set large page entry in PD
        pd_table.set_entry(pd_index, phys_addr | flags | page_flags::HUGE_PAGE);
    }

    /// Map a 4KB page for user space
    pub fn map_user_page(&mut self, virt_addr: u64, phys_addr: u64, flags: u64) -> Result<(), &'static str> {
        if virt_addr >= USER_SPACE_MAX {
            return Err("Virtual address outside user space");
        }

        let user_flags = flags | page_flags::USER;
        self.map_4k_page(virt_addr, phys_addr, user_flags);
        Ok(())
    }

    /// Map a 4KB page
    fn map_4k_page(&mut self, virt_addr: u64, phys_addr: u64, flags: u64) {
        let pml4_index = ((virt_addr >> 39) & 0x1FF) as usize;
        let pdp_index = ((virt_addr >> 30) & 0x1FF) as usize;
        let pd_index = ((virt_addr >> 21) & 0x1FF) as usize;
        let pt_index = ((virt_addr >> 12) & 0x1FF) as usize;

        // Get or create PDP table
        if self.pml4.get_entry(pml4_index) == 0 {
            let pdp_table = self.allocate_table();
            let pdp_phys = pdp_table as *const _ as u64;
            self.pml4.set_entry(pml4_index, pdp_phys | page_flags::PRESENT | page_flags::WRITABLE | page_flags::USER);
        }

        let pdp_phys = self.pml4.get_entry(pml4_index) & 0xFFFFFFFFFFFFF000;
        let pdp_table = unsafe { &mut *(pdp_phys as *mut PageTable) };

        // Get or create PD table
        if pdp_table.get_entry(pdp_index) == 0 {
            let pd_table = self.allocate_table();
            let pd_phys = pd_table as *const _ as u64;
            pdp_table.set_entry(pdp_index, pd_phys | page_flags::PRESENT | page_flags::WRITABLE | page_flags::USER);
        }

        let pd_phys = pdp_table.get_entry(pdp_index) & 0xFFFFFFFFFFFFF000;
        let pd_table = unsafe { &mut *(pd_phys as *mut PageTable) };

        // Get or create PT table
        if pd_table.get_entry(pd_index) == 0 {
            let pt_table = self.allocate_table();
            let pt_phys = pt_table as *const _ as u64;
            pd_table.set_entry(pd_index, pt_phys | page_flags::PRESENT | page_flags::WRITABLE | page_flags::USER);
        }

        let pt_phys = pd_table.get_entry(pd_index) & 0xFFFFFFFFFFFFF000;
        let pt_table = unsafe { &mut *(pt_phys as *mut PageTable) };

        // Set page entry
        pt_table.set_entry(pt_index, phys_addr | flags);
    }

    /// Allocate a new page table
    fn allocate_table(&mut self) -> *mut PageTable {
        // For now, use simple heap allocation
        // In a real OS, this would use a physical page allocator
        let table = Box::into_raw(Box::new(PageTable::new()));
        self.allocated_tables.push(table);
        table
    }

    /// Get physical address of PML4 table (for loading into CR3)
    pub fn pml4_phys_addr(&self) -> u64 {
        self.pml4.physical_addr()
    }
}

impl Drop for ProcessPageTable {
    fn drop(&mut self) {
        // Clean up allocated page tables
        for &table_ptr in &self.allocated_tables {
            unsafe {
                let _ = Box::from_raw(table_ptr);
            }
        }
    }
}

/// Enable paging with given PML4 address
pub unsafe fn enable_paging(pml4_phys: u64) {
    // Load PML4 into CR3
    core::arch::asm!(
        "mov cr3, {}",
        in(reg) pml4_phys,
        options(nostack, preserves_flags)
    );
}

/// Get current CR3 value
pub unsafe fn get_current_cr3() -> u64 {
    let cr3: u64;
    core::arch::asm!(
        "mov {}, cr3",
        out(reg) cr3,
        options(nostack, preserves_flags)
    );
    cr3
}

/// Check if paging is enabled
pub fn is_paging_enabled() -> bool {
    unsafe {
        let cr0: u64;
        core::arch::asm!(
            "mov {}, cr0",
            out(reg) cr0,
            options(nostack, preserves_flags)
        );
        (cr0 & (1 << 31)) != 0 // PG bit
    }
}

/// Initialize MMU subsystem
pub fn init() {
    // Paging should already be enabled by bootloader/early kernel
    // This function can be used for additional MMU setup if needed
    
    // Verify paging is enabled
    if !is_paging_enabled() {
        panic!("Paging not enabled - MMU cannot function");
    }
}