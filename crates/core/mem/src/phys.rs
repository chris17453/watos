//! Physical Memory Allocator
//!
//! Simple bitmap-based physical page allocator.
//! Tracks which 4KB physical pages are free or in use.

use spin::Mutex;
use crate::paging::PAGE_SIZE;

/// Maximum physical memory supported (1GB)
const MAX_PHYS_MEMORY: usize = 1024 * 1024 * 1024;

/// Number of pages we can track
const MAX_PAGES: usize = MAX_PHYS_MEMORY / PAGE_SIZE;

/// Bitmap size in u64 words
const BITMAP_SIZE: usize = MAX_PAGES / 64;

/// Physical page allocator
static PHYS_ALLOCATOR: Mutex<PhysAllocator> = Mutex::new(PhysAllocator::new());

/// Bitmap-based physical page allocator
struct PhysAllocator {
    /// Bitmap: 1 = free, 0 = used
    bitmap: [u64; BITMAP_SIZE],
    /// Total pages available
    total_pages: usize,
    /// Free pages remaining
    free_pages: usize,
    /// Next page to check (for faster allocation)
    next_free: usize,
}

impl PhysAllocator {
    const fn new() -> Self {
        PhysAllocator {
            bitmap: [0; BITMAP_SIZE],
            total_pages: 0,
            free_pages: 0,
            next_free: 0,
        }
    }

    /// Initialize with available memory range
    fn init(&mut self, start: u64, size: u64) {
        let start_page = (start as usize) / PAGE_SIZE;
        let num_pages = (size as usize) / PAGE_SIZE;

        // Mark pages as free
        for i in 0..num_pages {
            let page = start_page + i;
            if page < MAX_PAGES {
                let word = page / 64;
                let bit = page % 64;
                self.bitmap[word] |= 1 << bit;
            }
        }

        self.total_pages = num_pages.min(MAX_PAGES - start_page);
        self.free_pages = self.total_pages;
        self.next_free = start_page;
    }

    /// Allocate a single physical page
    fn alloc_page(&mut self) -> Option<u64> {
        if self.free_pages == 0 {
            return None;
        }

        // Search starting from next_free hint
        for i in 0..BITMAP_SIZE {
            let idx = (self.next_free / 64 + i) % BITMAP_SIZE;
            if self.bitmap[idx] != 0 {
                // Found a word with free pages
                let bit = self.bitmap[idx].trailing_zeros() as usize;
                self.bitmap[idx] &= !(1 << bit);
                self.free_pages -= 1;

                let page = idx * 64 + bit;
                self.next_free = page + 1;

                return Some((page * PAGE_SIZE) as u64);
            }
        }

        None
    }

    /// Free a physical page
    fn free_page(&mut self, phys_addr: u64) {
        let page = (phys_addr as usize) / PAGE_SIZE;
        if page >= MAX_PAGES {
            return;
        }

        let word = page / 64;
        let bit = page % 64;

        // Only increment free count if page was actually used
        if (self.bitmap[word] & (1 << bit)) == 0 {
            self.bitmap[word] |= 1 << bit;
            self.free_pages += 1;

            // Update hint if this is earlier
            if page < self.next_free {
                self.next_free = page;
            }
        }
    }

    /// Allocate contiguous physical pages
    fn alloc_contiguous(&mut self, count: usize) -> Option<u64> {
        if count == 0 || self.free_pages < count {
            return None;
        }

        // Simple linear search for contiguous free pages
        let mut run_start = 0;
        let mut run_length = 0;

        for page in 0..MAX_PAGES {
            let word = page / 64;
            let bit = page % 64;

            if (self.bitmap[word] & (1 << bit)) != 0 {
                // Page is free
                if run_length == 0 {
                    run_start = page;
                }
                run_length += 1;

                if run_length == count {
                    // Found enough contiguous pages, mark them as used
                    for i in 0..count {
                        let p = run_start + i;
                        let w = p / 64;
                        let b = p % 64;
                        self.bitmap[w] &= !(1 << b);
                    }
                    self.free_pages -= count;
                    return Some((run_start * PAGE_SIZE) as u64);
                }
            } else {
                // Page is used, reset run
                run_length = 0;
            }
        }

        None
    }
}

/// Initialize physical memory allocator
///
/// # Arguments
///
/// * `start` - Starting physical address of available memory
/// * `size` - Size of available memory in bytes
pub fn init(start: u64, size: u64) {
    PHYS_ALLOCATOR.lock().init(start, size);
}

/// Allocate a single physical page (4KB)
///
/// Returns the physical address of the allocated page, or None if out of memory.
pub fn alloc_page() -> Option<u64> {
    PHYS_ALLOCATOR.lock().alloc_page()
}

/// Free a physical page
///
/// # Arguments
///
/// * `phys_addr` - Physical address of page to free (must be page-aligned)
pub fn free_page(phys_addr: u64) {
    PHYS_ALLOCATOR.lock().free_page(phys_addr);
}

/// Allocate contiguous physical pages
///
/// # Arguments
///
/// * `count` - Number of contiguous pages needed
///
/// Returns the physical address of the first page, or None if not available.
pub fn alloc_contiguous(count: usize) -> Option<u64> {
    PHYS_ALLOCATOR.lock().alloc_contiguous(count)
}

/// Get physical memory statistics
pub fn stats() -> PhysStats {
    let alloc = PHYS_ALLOCATOR.lock();
    PhysStats {
        total_pages: alloc.total_pages,
        free_pages: alloc.free_pages,
        used_pages: alloc.total_pages - alloc.free_pages,
    }
}

/// Physical memory statistics
#[derive(Debug, Clone, Copy)]
pub struct PhysStats {
    /// Total pages managed
    pub total_pages: usize,
    /// Free pages available
    pub free_pages: usize,
    /// Pages currently in use
    pub used_pages: usize,
}

impl PhysStats {
    /// Total memory in bytes
    pub fn total_bytes(&self) -> usize {
        self.total_pages * PAGE_SIZE
    }

    /// Free memory in bytes
    pub fn free_bytes(&self) -> usize {
        self.free_pages * PAGE_SIZE
    }

    /// Used memory in bytes
    pub fn used_bytes(&self) -> usize {
        self.used_pages * PAGE_SIZE
    }
}
