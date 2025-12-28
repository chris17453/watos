//! x86_64 Paging
//!
//! Provides page table management for x86_64 4-level paging:
//! - PML4 -> PDP -> PD -> PT
//! - 4KB and 2MB page support
//! - User/kernel space separation
//!
//! # Memory Layout
//!
//! ```text
//! 0x0000_0000_0000_0000 - 0x0000_7FFF_FFFF_FFFF : User space (lower half)
//! 0xFFFF_8000_0000_0000 - 0xFFFF_FFFF_FFFF_FFFF : Kernel space (higher half)
//! ```

use alloc::vec::Vec;
use alloc::boxed::Box;

/// Page size (4KB)
pub const PAGE_SIZE: usize = 0x1000;

/// Large page size (2MB)
pub const LARGE_PAGE_SIZE: usize = 0x200000;

/// Kernel space starts at -2GB (canonical high address)
pub const KERNEL_SPACE_START: u64 = 0xFFFF_FFFF_8000_0000;

/// User space maximum (lower 128TB in canonical addressing)
pub const USER_SPACE_MAX: u64 = 0x0000_7FFF_FFFF_FFFF;

/// Page table entry flags
pub mod flags {
    /// Page is present in memory
    pub const PRESENT: u64 = 1 << 0;
    /// Page is writable
    pub const WRITABLE: u64 = 1 << 1;
    /// Page is accessible from user mode (Ring 3)
    pub const USER: u64 = 1 << 2;
    /// Write-through caching
    pub const WRITE_THROUGH: u64 = 1 << 3;
    /// Disable caching
    pub const NO_CACHE: u64 = 1 << 4;
    /// Page has been accessed
    pub const ACCESSED: u64 = 1 << 5;
    /// Page has been written to
    pub const DIRTY: u64 = 1 << 6;
    /// This is a 2MB/1GB huge page (in PD/PDP entries)
    pub const HUGE_PAGE: u64 = 1 << 7;
    /// Page is global (not flushed on CR3 switch)
    pub const GLOBAL: u64 = 1 << 8;
    /// Disable execution (NX bit)
    pub const NO_EXECUTE: u64 = 1 << 63;

    /// Mask for extracting physical address from entry
    pub const ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;
}

/// 4-level page table structure
///
/// Each table contains 512 entries, each 8 bytes.
/// Tables must be 4KB aligned.
#[repr(C, align(4096))]
pub struct PageTable {
    entries: [u64; 512],
}

impl PageTable {
    /// Create a new empty page table
    pub const fn new() -> Self {
        PageTable {
            entries: [0; 512],
        }
    }

    /// Get entry at index
    #[inline]
    pub fn get_entry(&self, index: usize) -> u64 {
        self.entries[index]
    }

    /// Set entry at index
    #[inline]
    pub fn set_entry(&mut self, index: usize, entry: u64) {
        self.entries[index] = entry;
    }

    /// Get physical address of this table
    #[inline]
    pub fn physical_addr(&self) -> u64 {
        self as *const _ as u64
    }

    /// Check if entry is present
    #[inline]
    pub fn is_present(&self, index: usize) -> bool {
        (self.entries[index] & flags::PRESENT) != 0
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        for entry in &mut self.entries {
            *entry = 0;
        }
    }
}

impl Default for PageTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-process page table manager
///
/// Manages a complete 4-level page table hierarchy for a process.
/// Automatically maps kernel space for interrupt handling.
pub struct ProcessPageTable {
    /// Root PML4 table
    pml4: PageTable,
    /// Allocated sub-tables (for cleanup)
    allocated_tables: Vec<*mut PageTable>,
}

impl ProcessPageTable {
    /// Create new page table for a process
    ///
    /// The kernel space is automatically mapped (identity + high canonical).
    pub fn new() -> Self {
        let mut ppt = ProcessPageTable {
            pml4: PageTable::new(),
            allocated_tables: Vec::new(),
        };

        // Map kernel space (required for interrupts/syscalls)
        ppt.map_kernel_space();

        ppt
    }

    /// Map kernel space into this page table
    ///
    /// Maps kernel memory regions to:
    /// 1. Identity mapping (virt = phys) for kernel code/data access
    /// 2. High canonical addresses for proper kernel space
    ///
    /// Covers:
    /// - First 8MB: kernel code, heap, bootloader data, kernel stacks
    /// - 16MB-48MB: physical page allocator region (for kernel access during syscalls)
    ///
    /// NOTE: Currently includes USER flag for shared kernel/user memory (like heap).
    /// This allows user processes to use kernel-allocated memory (SYS_MALLOC).
    fn map_kernel_space(&mut self) {
        // Include USER flag so user code can access kernel heap (for SYS_MALLOC)
        let kernel_flags = flags::PRESENT | flags::WRITABLE | flags::GLOBAL | flags::USER;
        let kernel_only_flags = flags::PRESENT | flags::WRITABLE | flags::GLOBAL;

        // Map first 8MB (4 x 2MB pages) using huge pages
        // This covers kernel code (0x100000), heap, app data, and kernel stacks (0x280000+)
        for i in 0..4 {
            let phys_addr = (i as u64) * LARGE_PAGE_SIZE as u64;

            // Identity mapping for kernel and user access
            self.map_large_page(phys_addr, phys_addr, kernel_flags);

            // High canonical mapping for proper kernel space (kernel only)
            let high_virt = KERNEL_SPACE_START + phys_addr;
            self.map_large_page(high_virt, phys_addr, kernel_only_flags);
        }

        // NOTE: Do NOT map 16MB+ here - that's where user processes are loaded.
        // Kernel accesses to physical pages during syscalls use the KERNEL page table
        // (we switch to it before exec/loading), not the user page table.
    }

    /// Map a 2MB large page
    pub fn map_large_page(&mut self, virt_addr: u64, phys_addr: u64, flags: u64) {
        let pml4_idx = ((virt_addr >> 39) & 0x1FF) as usize;
        let pdp_idx = ((virt_addr >> 30) & 0x1FF) as usize;
        let pd_idx = ((virt_addr >> 21) & 0x1FF) as usize;

        // Hierarchy flags - must include USER if we want user-space access
        let is_user_page = (flags & flags::USER) != 0;
        let hier_flags = flags::PRESENT | flags::WRITABLE | (if is_user_page { flags::USER } else { 0 });

        // Ensure PDP table exists - add USER flag if needed
        if !self.pml4.is_present(pml4_idx) {
            let pdp = self.allocate_table();
            let pdp_phys = pdp as u64;
            self.pml4.set_entry(pml4_idx, pdp_phys | hier_flags);
        } else if is_user_page {
            // Add USER flag to existing entry if needed
            let entry = self.pml4.get_entry(pml4_idx);
            if entry & flags::USER == 0 {
                self.pml4.set_entry(pml4_idx, entry | flags::USER);
            }
        }

        let pdp_phys = self.pml4.get_entry(pml4_idx) & flags::ADDR_MASK;
        let pdp = unsafe { &mut *(pdp_phys as *mut PageTable) };

        // Ensure PD table exists - add USER flag if needed
        if !pdp.is_present(pdp_idx) {
            let pd = self.allocate_table();
            let pd_phys = pd as u64;
            pdp.set_entry(pdp_idx, pd_phys | hier_flags);
        } else if is_user_page {
            let entry = pdp.get_entry(pdp_idx);
            if entry & flags::USER == 0 {
                pdp.set_entry(pdp_idx, entry | flags::USER);
            }
        }

        let pd_phys = pdp.get_entry(pdp_idx) & flags::ADDR_MASK;
        let pd = unsafe { &mut *(pd_phys as *mut PageTable) };

        // Set 2MB page entry
        pd.set_entry(pd_idx, phys_addr | flags | flags::HUGE_PAGE);
    }

    /// Map a 4KB page for user space
    ///
    /// Automatically adds USER flag to allow Ring 3 access.
    pub fn map_user_page(&mut self, virt_addr: u64, phys_addr: u64, flags: u64) -> Result<(), &'static str> {
        if virt_addr > USER_SPACE_MAX {
            return Err("Virtual address outside user space");
        }

        let user_flags = flags | flags::USER;
        self.map_4k_page(virt_addr, phys_addr, user_flags);
        Ok(())
    }

    /// Map a 4KB page
    pub fn map_4k_page(&mut self, virt_addr: u64, phys_addr: u64, flags: u64) {
        let pml4_idx = ((virt_addr >> 39) & 0x1FF) as usize;
        let pdp_idx = ((virt_addr >> 30) & 0x1FF) as usize;
        let pd_idx = ((virt_addr >> 21) & 0x1FF) as usize;
        let pt_idx = ((virt_addr >> 12) & 0x1FF) as usize;

        // Hierarchy flags - must include USER if we want user-space access
        let is_user_page = (flags & flags::USER) != 0;
        let hier_flags = flags::PRESENT | flags::WRITABLE | (if is_user_page { flags::USER } else { 0 });

        // Ensure PDP exists - if entry exists but lacks USER flag, add it
        if !self.pml4.is_present(pml4_idx) {
            let pdp = self.allocate_table();
            self.pml4.set_entry(pml4_idx, pdp as u64 | hier_flags);
        } else if is_user_page {
            // Add USER flag to existing entry if needed
            let entry = self.pml4.get_entry(pml4_idx);
            if entry & flags::USER == 0 {
                self.pml4.set_entry(pml4_idx, entry | flags::USER);
            }
        }

        let pdp_phys = self.pml4.get_entry(pml4_idx) & flags::ADDR_MASK;
        let pdp = unsafe { &mut *(pdp_phys as *mut PageTable) };

        // Ensure PD exists - add USER flag if needed
        if !pdp.is_present(pdp_idx) {
            let pd = self.allocate_table();
            pdp.set_entry(pdp_idx, pd as u64 | hier_flags);
        } else if is_user_page {
            let entry = pdp.get_entry(pdp_idx);
            if entry & flags::USER == 0 {
                pdp.set_entry(pdp_idx, entry | flags::USER);
            }
        }

        let pd_phys = pdp.get_entry(pdp_idx) & flags::ADDR_MASK;
        let pd = unsafe { &mut *(pd_phys as *mut PageTable) };

        // Ensure PT exists - handle huge pages specially
        let pd_entry = pd.get_entry(pd_idx);
        if !pd.is_present(pd_idx) {
            let pt = self.allocate_table();
            pd.set_entry(pd_idx, pt as u64 | hier_flags);
        } else if (pd_entry & flags::HUGE_PAGE) != 0 {
            // Split huge page (2MB) into 512 x 4KB pages
            // Get the physical base address of the huge page
            let huge_phys_base = pd_entry & flags::ADDR_MASK;
            let old_flags = pd_entry & !flags::ADDR_MASK & !flags::HUGE_PAGE;

            // Allocate a new page table
            let pt = self.allocate_table();
            let pt_ptr = unsafe { &mut *(pt as *mut PageTable) };

            // Map all 512 4KB pages to match the original huge page
            for i in 0..512 {
                let page_phys = huge_phys_base + (i as u64 * PAGE_SIZE as u64);
                pt_ptr.set_entry(i, page_phys | old_flags);
            }

            // Replace the huge page entry with the PT pointer
            pd.set_entry(pd_idx, pt as u64 | hier_flags);
        } else if is_user_page {
            let entry = pd.get_entry(pd_idx);
            if entry & flags::USER == 0 {
                pd.set_entry(pd_idx, entry | flags::USER);
            }
        }

        let pt_phys = pd.get_entry(pd_idx) & flags::ADDR_MASK;
        let pt = unsafe { &mut *(pt_phys as *mut PageTable) };

        // Set final page entry
        pt.set_entry(pt_idx, phys_addr | flags);
    }

    /// Look up the physical address for a virtual address
    /// Returns None if the page is not mapped
    pub fn lookup(&self, virt_addr: u64) -> Option<u64> {
        let pml4_idx = ((virt_addr >> 39) & 0x1FF) as usize;
        let pdp_idx = ((virt_addr >> 30) & 0x1FF) as usize;
        let pd_idx = ((virt_addr >> 21) & 0x1FF) as usize;
        let pt_idx = ((virt_addr >> 12) & 0x1FF) as usize;

        if !self.pml4.is_present(pml4_idx) {
            return None;
        }

        let pdp_phys = self.pml4.get_entry(pml4_idx) & flags::ADDR_MASK;
        let pdp = unsafe { &*(pdp_phys as *const PageTable) };

        if !pdp.is_present(pdp_idx) {
            return None;
        }

        let pd_phys = pdp.get_entry(pdp_idx) & flags::ADDR_MASK;
        let pd = unsafe { &*(pd_phys as *const PageTable) };

        if !pd.is_present(pd_idx) {
            return None;
        }

        // Check for huge page (2MB)
        if (pd.get_entry(pd_idx) & flags::HUGE_PAGE) != 0 {
            let huge_phys = pd.get_entry(pd_idx) & flags::ADDR_MASK;
            return Some(huge_phys + (virt_addr & 0x1FFFFF)); // offset within 2MB page
        }

        let pt_phys = pd.get_entry(pd_idx) & flags::ADDR_MASK;
        let pt = unsafe { &*(pt_phys as *const PageTable) };

        let pt_entry = pt.get_entry(pt_idx);
        if (pt_entry & flags::PRESENT) == 0 {
            return None;
        }

        Some(pt_entry & flags::ADDR_MASK)
    }

    /// Unmap a 4KB page
    pub fn unmap_4k_page(&mut self, virt_addr: u64) -> Option<u64> {
        let pml4_idx = ((virt_addr >> 39) & 0x1FF) as usize;
        let pdp_idx = ((virt_addr >> 30) & 0x1FF) as usize;
        let pd_idx = ((virt_addr >> 21) & 0x1FF) as usize;
        let pt_idx = ((virt_addr >> 12) & 0x1FF) as usize;

        if !self.pml4.is_present(pml4_idx) {
            return None;
        }

        let pdp_phys = self.pml4.get_entry(pml4_idx) & flags::ADDR_MASK;
        let pdp = unsafe { &*(pdp_phys as *const PageTable) };

        if !pdp.is_present(pdp_idx) {
            return None;
        }

        let pd_phys = pdp.get_entry(pdp_idx) & flags::ADDR_MASK;
        let pd = unsafe { &*(pd_phys as *const PageTable) };

        if !pd.is_present(pd_idx) {
            return None;
        }

        let pt_phys = pd.get_entry(pd_idx) & flags::ADDR_MASK;
        let pt = unsafe { &mut *(pt_phys as *mut PageTable) };

        let old_entry = pt.get_entry(pt_idx);
        if (old_entry & flags::PRESENT) == 0 {
            return None;
        }

        pt.set_entry(pt_idx, 0);

        // Invalidate TLB for this address
        unsafe {
            core::arch::asm!(
                "invlpg [{}]",
                in(reg) virt_addr,
                options(nostack, preserves_flags)
            );
        }

        Some(old_entry & flags::ADDR_MASK)
    }

    /// Allocate a new page table
    fn allocate_table(&mut self) -> *mut PageTable {
        let table = Box::into_raw(Box::new(PageTable::new()));
        self.allocated_tables.push(table);
        table
    }

    /// Get physical address of PML4 (for loading into CR3)
    pub fn pml4_phys_addr(&self) -> u64 {
        self.pml4.physical_addr()
    }

    /// Activate this page table (load into CR3)
    ///
    /// # Safety
    ///
    /// This changes the active page table. The caller must ensure:
    /// - Kernel code remains accessible after the switch
    /// - The page table correctly maps all required memory
    pub unsafe fn activate(&self) {
        load_cr3(self.pml4_phys_addr());
    }
}

impl Default for ProcessPageTable {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ProcessPageTable {
    fn drop(&mut self) {
        // Free all allocated sub-tables
        for &table_ptr in &self.allocated_tables {
            unsafe {
                let _ = Box::from_raw(table_ptr);
            }
        }
    }
}

/// Load a page table address into CR3
///
/// # Safety
///
/// The physical address must point to a valid PML4 table.
#[inline]
pub unsafe fn load_cr3(pml4_phys: u64) {
    core::arch::asm!(
        "mov cr3, {}",
        in(reg) pml4_phys,
        options(nostack, preserves_flags)
    );
}

/// Get current CR3 value
#[inline]
pub fn get_cr3() -> u64 {
    let cr3: u64;
    unsafe {
        core::arch::asm!(
            "mov {}, cr3",
            out(reg) cr3,
            options(nostack, preserves_flags)
        );
    }
    cr3
}

/// Check if paging is enabled (CR0.PG bit)
#[inline]
pub fn is_enabled() -> bool {
    let cr0: u64;
    unsafe {
        core::arch::asm!(
            "mov {}, cr0",
            out(reg) cr0,
            options(nostack, preserves_flags)
        );
    }
    (cr0 & (1 << 31)) != 0
}

/// Flush entire TLB by reloading CR3
#[inline]
pub fn flush_tlb() {
    let cr3 = get_cr3();
    unsafe {
        load_cr3(cr3);
    }
}

/// Invalidate TLB entry for a specific address
#[inline]
pub fn invlpg(virt_addr: u64) {
    unsafe {
        core::arch::asm!(
            "invlpg [{}]",
            in(reg) virt_addr,
            options(nostack, preserves_flags)
        );
    }
}
