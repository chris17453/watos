//! Virtual Memory Manager (VMM)
//!
//! High-level virtual memory management for processes.
//! Wraps page table operations with layout-aware logic.
//! All virtual address decisions reference layout.rs.

use crate::layout::{
    PAGE_SIZE, PAGE_SIZE_U64,
    VIRT_USER_CODE, VIRT_USER_HEAP, VIRT_USER_STACK_TOP,
    VIRT_USER_STACK_BASE, VIRT_USER_GUARD, VIRT_USER_MAX,
    page_align_up, pages_needed,
};
use crate::paging::{ProcessPageTable, flags};
use crate::pmm;

/// Virtual Memory Manager for a process
///
/// Manages the virtual address space for a single process.
/// Tracks mapped regions and provides high-level mapping APIs.
pub struct VirtualMemoryManager {
    /// Underlying page table
    page_table: ProcessPageTable,
    /// Current heap break (end of heap)
    heap_break: u64,
    /// Stack is pre-allocated at creation
    stack_mapped: bool,
}

/// Result of mapping user code
pub struct UserCodeMapping {
    /// Virtual address where code was mapped
    pub virt_addr: u64,
    /// Number of pages mapped
    pub page_count: usize,
}

/// Result of mapping user stack
pub struct UserStackMapping {
    /// Top of stack (RSP should start here)
    pub stack_top: u64,
    /// Bottom of stack (lowest valid address)
    pub stack_bottom: u64,
    /// Size in bytes
    pub size: u64,
}

impl VirtualMemoryManager {
    /// Create new VMM for a process
    ///
    /// Automatically maps kernel space (required for interrupts/syscalls).
    /// User code, heap, and stack are mapped separately.
    pub fn new() -> Self {
        VirtualMemoryManager {
            page_table: ProcessPageTable::new(),
            heap_break: VIRT_USER_HEAP,
            stack_mapped: false,
        }
    }

    /// Map user code/data segment
    ///
    /// Maps physical pages at the specified virtual address in user space.
    /// Typically used by ELF loader to map executable segments.
    ///
    /// # Arguments
    /// * `virt_addr` - Virtual address to map at (must be >= VIRT_USER_CODE)
    /// * `phys_addr` - Physical address of the data
    /// * `size` - Size in bytes (will be rounded up to pages)
    /// * `writable` - Whether the region should be writable
    /// * `executable` - Whether the region should be executable
    ///
    /// # Returns
    /// Information about the mapping, or error if invalid
    pub fn map_user_code(
        &mut self,
        virt_addr: u64,
        phys_addr: u64,
        size: usize,
        writable: bool,
        executable: bool,
    ) -> Result<UserCodeMapping, &'static str> {
        // Validate virtual address
        if virt_addr < VIRT_USER_CODE {
            return Err("Virtual address below user code region");
        }
        if virt_addr >= VIRT_USER_HEAP {
            return Err("Virtual address in heap/stack region");
        }

        let page_count = pages_needed(size);
        let mut flags = flags::PRESENT | flags::USER;

        if writable {
            flags |= flags::WRITABLE;
        }
        if !executable {
            flags |= flags::NO_EXECUTE;
        }

        // Map each page
        for i in 0..page_count {
            let virt = virt_addr + (i as u64 * PAGE_SIZE_U64);
            let phys = phys_addr + (i as u64 * PAGE_SIZE_U64);

            self.page_table.map_4k_page(virt, phys, flags);
        }

        Ok(UserCodeMapping {
            virt_addr,
            page_count,
        })
    }

    /// Map user code with freshly allocated physical pages
    ///
    /// Allocates physical pages and maps them at the virtual address.
    /// Returns the physical address of the first page.
    pub fn map_user_code_alloc(
        &mut self,
        virt_addr: u64,
        size: usize,
        writable: bool,
        executable: bool,
    ) -> Result<(UserCodeMapping, u64), &'static str> {
        if virt_addr < VIRT_USER_CODE || virt_addr >= VIRT_USER_HEAP {
            return Err("Invalid user code address");
        }

        let page_count = pages_needed(size);
        let mut flags = flags::PRESENT | flags::USER;

        if writable {
            flags |= flags::WRITABLE;
        }
        if !executable {
            flags |= flags::NO_EXECUTE;
        }

        // Allocate and map pages
        let mut first_phys = 0u64;
        for i in 0..page_count {
            let phys = pmm::alloc_page().ok_or("Out of physical memory")?;
            if i == 0 {
                first_phys = phys;
            }

            let virt = virt_addr + (i as u64 * PAGE_SIZE_U64);
            self.page_table.map_4k_page(virt, phys, flags);
            self.page_table.track_phys_page(phys);
        }

        Ok((
            UserCodeMapping {
                virt_addr,
                page_count,
            },
            first_phys,
        ))
    }

    /// Map user stack
    ///
    /// Allocates and maps the user stack region.
    /// Stack grows down from VIRT_USER_STACK_TOP.
    /// A guard page is placed at VIRT_USER_GUARD.
    ///
    /// # Arguments
    /// * `pages` - Number of 4KB pages for the stack (default: use full stack region)
    pub fn map_user_stack(&mut self, pages: Option<usize>) -> Result<UserStackMapping, &'static str> {
        if self.stack_mapped {
            return Err("Stack already mapped");
        }

        let stack_pages = pages.unwrap_or_else(|| {
            ((VIRT_USER_STACK_TOP - VIRT_USER_STACK_BASE) / PAGE_SIZE_U64) as usize
        });

        let stack_size = (stack_pages as u64) * PAGE_SIZE_U64;
        let stack_bottom = VIRT_USER_STACK_TOP - stack_size;

        // Ensure we don't overlap with guard page
        if stack_bottom <= VIRT_USER_GUARD {
            return Err("Stack too large, overlaps guard page");
        }

        let flags = flags::PRESENT | flags::USER | flags::WRITABLE | flags::NO_EXECUTE;

        // Map stack pages (bottom to top, stack grows down)
        for i in 0..stack_pages {
            let phys = pmm::alloc_page().ok_or("Out of physical memory for stack")?;
            let virt = stack_bottom + (i as u64 * PAGE_SIZE_U64);

            self.page_table.map_4k_page(virt, phys, flags);
            self.page_table.track_phys_page(phys);
        }

        // Note: Guard page at VIRT_USER_GUARD is intentionally NOT mapped.
        // Accessing it will cause a page fault -> stack overflow detection.

        self.stack_mapped = true;

        Ok(UserStackMapping {
            stack_top: VIRT_USER_STACK_TOP,
            stack_bottom,
            size: stack_size,
        })
    }

    /// Extend user heap (sbrk-like)
    ///
    /// Allocates and maps additional pages for the heap.
    /// Returns the old heap break (start of new allocation).
    ///
    /// # Arguments
    /// * `increment` - Number of bytes to add (rounded up to pages)
    pub fn extend_heap(&mut self, increment: usize) -> Result<u64, &'static str> {
        let old_break = self.heap_break;
        let pages = pages_needed(increment);
        let new_break = old_break + (pages as u64 * PAGE_SIZE_U64);

        // Check we don't overlap with guard page
        if new_break > VIRT_USER_GUARD {
            return Err("Heap would overlap with stack guard");
        }

        let flags = flags::PRESENT | flags::USER | flags::WRITABLE | flags::NO_EXECUTE;

        for i in 0..pages {
            let phys = pmm::alloc_page().ok_or("Out of physical memory for heap")?;
            let virt = old_break + (i as u64 * PAGE_SIZE_U64);

            self.page_table.map_4k_page(virt, phys, flags);
            self.page_table.track_phys_page(phys);
        }

        self.heap_break = new_break;
        Ok(old_break)
    }

    /// Get current heap break
    pub fn heap_break(&self) -> u64 {
        self.heap_break
    }

    /// Set heap break directly (for sbrk syscall)
    ///
    /// If new_break > current, allocates pages.
    /// If new_break < current, does nothing (we don't free pages).
    pub fn set_heap_break(&mut self, new_break: u64) -> Result<u64, &'static str> {
        let aligned = page_align_up(new_break);

        if aligned <= self.heap_break {
            // Shrinking or same - just update break
            self.heap_break = aligned;
            return Ok(self.heap_break);
        }

        // Growing - allocate pages
        let pages = ((aligned - self.heap_break) / PAGE_SIZE_U64) as usize;
        let flags = flags::PRESENT | flags::USER | flags::WRITABLE | flags::NO_EXECUTE;

        for i in 0..pages {
            let phys = pmm::alloc_page().ok_or("Out of physical memory")?;
            let virt = self.heap_break + (i as u64 * PAGE_SIZE_U64);

            self.page_table.map_4k_page(virt, phys, flags);
            self.page_table.track_phys_page(phys);
        }

        self.heap_break = aligned;
        Ok(self.heap_break)
    }

    /// Check if a virtual address is valid (mapped) in user space
    pub fn is_valid_user_addr(&self, addr: u64) -> bool {
        if addr >= VIRT_USER_MAX || addr < VIRT_USER_CODE {
            return false;
        }
        self.page_table.lookup(addr).is_some()
    }

    /// Look up physical address for a virtual address
    pub fn lookup(&self, virt_addr: u64) -> Option<u64> {
        self.page_table.lookup(virt_addr)
    }

    /// Get CR3 value for this address space
    pub fn cr3(&self) -> u64 {
        self.page_table.pml4_phys_addr()
    }

    /// Activate this address space (load into CR3)
    ///
    /// # Safety
    /// Caller must ensure this is safe to do (kernel code still accessible).
    pub unsafe fn activate(&self) {
        self.page_table.activate();
    }

    /// Get mutable reference to underlying page table
    ///
    /// For advanced operations not covered by VMM API.
    pub fn page_table_mut(&mut self) -> &mut ProcessPageTable {
        &mut self.page_table
    }

    /// Get reference to underlying page table
    pub fn page_table(&self) -> &ProcessPageTable {
        &self.page_table
    }

    /// Consume VMM and return the underlying page table
    pub fn into_page_table(self) -> ProcessPageTable {
        self.page_table
    }
}

impl Default for VirtualMemoryManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Quick helper to get standard user code address
pub fn user_code_base() -> u64 {
    VIRT_USER_CODE
}

/// Quick helper to get standard user stack top
pub fn user_stack_top() -> u64 {
    VIRT_USER_STACK_TOP
}

/// Quick helper to get user heap start
pub fn user_heap_base() -> u64 {
    VIRT_USER_HEAP
}
