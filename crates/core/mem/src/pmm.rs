//! Physical Memory Manager (PMM)
//!
//! Single allocator for all physical memory. Uses a bitmap to track 4KB pages.
//! Initializes from UEFI memory map to detect all available RAM.

use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Mutex;

use crate::layout::{
    PAGE_SIZE, PAGE_SIZE_U64, PHYS_ALLOCATOR_START,
    page_align_up, pfn_from_phys, phys_from_pfn,
};

/// Maximum supported RAM (128MB for now - can be increased)
const MAX_RAM_SIZE: u64 = 128 * 1024 * 1024;

/// Maximum pages we can track
const MAX_PAGES: usize = (MAX_RAM_SIZE / PAGE_SIZE_U64) as usize;

/// Bitmap words needed (64 pages per u64)
const BITMAP_WORDS: usize = (MAX_PAGES + 63) / 64;

/// Physical Memory Manager
pub struct PhysicalMemoryManager {
    /// Bitmap: 1 = free, 0 = allocated
    bitmap: [u64; BITMAP_WORDS],
    /// Start of allocatable region
    start_addr: u64,
    /// Total pages in allocatable region
    total_pages: usize,
    /// Free pages count
    free_pages: AtomicUsize,
    /// Initialized flag
    initialized: bool,
}

/// PMM statistics
#[derive(Debug, Clone, Copy)]
pub struct PmmStats {
    pub total_pages: usize,
    pub free_pages: usize,
    pub used_pages: usize,
    pub start_addr: u64,
    pub total_bytes: usize,
    pub free_bytes: usize,
}

/// Global PMM instance
static PMM: Mutex<PhysicalMemoryManager> = Mutex::new(PhysicalMemoryManager {
    bitmap: [0; BITMAP_WORDS],
    start_addr: PHYS_ALLOCATOR_START,
    total_pages: 0,
    free_pages: AtomicUsize::new(0),
    initialized: false,
});

impl PhysicalMemoryManager {
    /// Initialize PMM with explicit region (fallback if no UEFI map)
    ///
    /// # Arguments
    /// * `start` - Start physical address (will be aligned up to page boundary)
    /// * `size` - Size in bytes of available memory
    pub fn init(&mut self, start: u64, size: u64) {
        let aligned_start = page_align_up(start);
        let pages = (size / PAGE_SIZE_U64) as usize;

        // Limit to what we can track
        let pages = pages.min(MAX_PAGES);

        self.start_addr = aligned_start;
        self.total_pages = pages;

        // Mark all pages as free (set bits to 1)
        for i in 0..pages {
            let word = i / 64;
            let bit = i % 64;
            self.bitmap[word] |= 1u64 << bit;
        }

        self.free_pages.store(pages, Ordering::SeqCst);
        self.initialized = true;
    }

    /// Initialize from UEFI memory map
    ///
    /// Scans memory descriptors and adds all usable regions above PHYS_ALLOCATOR_START
    pub fn init_from_memory_map(&mut self, entries: &[(u64, u64, u32)]) {
        // Memory type constants (from UEFI spec)
        const EFI_CONVENTIONAL_MEMORY: u32 = 7;
        const EFI_BOOT_SERVICES_CODE: u32 = 3;
        const EFI_BOOT_SERVICES_DATA: u32 = 4;

        self.start_addr = PHYS_ALLOCATOR_START;
        self.total_pages = 0;

        // Clear bitmap
        for word in self.bitmap.iter_mut() {
            *word = 0;
        }

        let mut total_free = 0usize;

        for &(phys_start, num_pages, mem_type) in entries {
            // Only use conventional memory and boot services memory
            // (boot services memory is free after ExitBootServices)
            if mem_type != EFI_CONVENTIONAL_MEMORY
                && mem_type != EFI_BOOT_SERVICES_CODE
                && mem_type != EFI_BOOT_SERVICES_DATA {
                continue;
            }

            let region_end = phys_start + num_pages * PAGE_SIZE_U64;

            // Skip regions entirely below our allocator start
            if region_end <= PHYS_ALLOCATOR_START {
                continue;
            }

            // Clip region to start at PHYS_ALLOCATOR_START
            let region_start = phys_start.max(PHYS_ALLOCATOR_START);
            let usable_pages = ((region_end - region_start) / PAGE_SIZE_U64) as usize;

            // Add pages to bitmap
            for i in 0..usable_pages {
                let phys = region_start + (i as u64 * PAGE_SIZE_U64);
                let pfn = pfn_from_phys(phys - PHYS_ALLOCATOR_START);

                if pfn < MAX_PAGES {
                    let word = pfn / 64;
                    let bit = pfn % 64;
                    self.bitmap[word] |= 1u64 << bit;
                    total_free += 1;

                    if pfn >= self.total_pages {
                        self.total_pages = pfn + 1;
                    }
                }
            }
        }

        self.free_pages.store(total_free, Ordering::SeqCst);
        self.initialized = true;
    }

    /// Allocate a single 4KB page
    ///
    /// Returns physical address or None if out of memory
    pub fn alloc_page(&mut self) -> Option<u64> {
        if !self.initialized {
            return None;
        }

        // Find first free page (first set bit)
        for word_idx in 0..(self.total_pages + 63) / 64 {
            let word = self.bitmap[word_idx];
            if word != 0 {
                // Found a word with free pages
                let bit = word.trailing_zeros() as usize;
                let pfn = word_idx * 64 + bit;

                if pfn < self.total_pages {
                    // Clear the bit (mark as allocated)
                    self.bitmap[word_idx] &= !(1u64 << bit);
                    self.free_pages.fetch_sub(1, Ordering::SeqCst);

                    let phys = self.start_addr + phys_from_pfn(pfn);
                    return Some(phys);
                }
            }
        }

        None
    }

    /// Free a previously allocated page
    ///
    /// # Safety
    /// Caller must ensure addr was returned by alloc_page and hasn't been freed
    pub fn free_page(&mut self, addr: u64) {
        if !self.initialized || addr < self.start_addr {
            return;
        }

        let pfn = pfn_from_phys(addr - self.start_addr);
        if pfn >= self.total_pages {
            return;
        }

        let word = pfn / 64;
        let bit = pfn % 64;

        // Set the bit (mark as free)
        self.bitmap[word] |= 1u64 << bit;
        self.free_pages.fetch_add(1, Ordering::SeqCst);
    }

    /// Allocate contiguous pages (for DMA buffers)
    ///
    /// Returns physical address of first page or None
    pub fn alloc_contiguous(&mut self, count: usize) -> Option<u64> {
        if !self.initialized || count == 0 {
            return None;
        }

        // Simple linear search for contiguous run
        let mut run_start = 0;
        let mut run_len = 0;

        for pfn in 0..self.total_pages {
            let word = pfn / 64;
            let bit = pfn % 64;

            if self.bitmap[word] & (1u64 << bit) != 0 {
                // Page is free
                if run_len == 0 {
                    run_start = pfn;
                }
                run_len += 1;

                if run_len >= count {
                    // Found enough contiguous pages - allocate them
                    for i in 0..count {
                        let p = run_start + i;
                        let w = p / 64;
                        let b = p % 64;
                        self.bitmap[w] &= !(1u64 << b);
                    }
                    self.free_pages.fetch_sub(count, Ordering::SeqCst);

                    return Some(self.start_addr + phys_from_pfn(run_start));
                }
            } else {
                // Page is allocated, reset run
                run_len = 0;
            }
        }

        None
    }

    /// Get PMM statistics
    pub fn stats(&self) -> PmmStats {
        let free = self.free_pages.load(Ordering::SeqCst);
        PmmStats {
            total_pages: self.total_pages,
            free_pages: free,
            used_pages: self.total_pages.saturating_sub(free),
            start_addr: self.start_addr,
            total_bytes: self.total_pages * PAGE_SIZE,
            free_bytes: free * PAGE_SIZE,
        }
    }
}

// =============================================================================
// Public API (global accessor functions)
// =============================================================================

/// Initialize PMM with explicit region
pub fn init(start: u64, size: u64) {
    PMM.lock().init(start, size);
}

/// Initialize PMM from UEFI memory map
///
/// entries: slice of (physical_start, page_count, memory_type)
pub fn init_from_memory_map(entries: &[(u64, u64, u32)]) {
    PMM.lock().init_from_memory_map(entries);
}

/// Allocate a single 4KB page
pub fn alloc_page() -> Option<u64> {
    PMM.lock().alloc_page()
}

/// Free a page
pub fn free_page(addr: u64) {
    PMM.lock().free_page(addr);
}

/// Allocate contiguous pages
pub fn alloc_contiguous(count: usize) -> Option<u64> {
    PMM.lock().alloc_contiguous(count)
}

/// Get PMM statistics
pub fn stats() -> PmmStats {
    PMM.lock().stats()
}

/// Check if PMM is initialized
pub fn is_initialized() -> bool {
    PMM.lock().initialized
}
