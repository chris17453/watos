//! Heap Allocator
//!
//! Provides the global heap allocator for WATOS using linked_list_allocator.
//! The heap must be initialized early in kernel startup before any allocations.
//!
//! Note: The `global-allocator` feature must be enabled to use this crate's
//! allocator as the global allocator. Otherwise, the main kernel provides one.

use linked_list_allocator::LockedHeap;

/// Heap allocator instance
///
/// This can be the kernel's primary heap allocator if `global-allocator` feature
/// is enabled. It must be initialized with `init()` before any heap allocations.
#[cfg_attr(feature = "global-allocator", global_allocator)]
pub static ALLOCATOR: LockedHeap = LockedHeap::empty();

/// Default heap configuration
pub mod config {
    /// Default heap start address (3MB mark)
    pub const DEFAULT_HEAP_START: usize = 0x300000;

    /// Default heap size (4 MiB)
    pub const DEFAULT_HEAP_SIZE: usize = 4 * 1024 * 1024;

    /// Maximum heap size (256 MiB)
    pub const MAX_HEAP_SIZE: usize = 256 * 1024 * 1024;
}

/// Initialize the heap allocator
///
/// # Safety
///
/// This function must be called exactly once, before any heap allocations.
/// The memory region [heap_start, heap_start + heap_size) must be:
/// - Valid physical memory
/// - Not used by any other part of the system
/// - Properly mapped in the page tables
///
/// # Arguments
///
/// * `heap_start` - Starting address of the heap region
/// * `heap_size` - Size of the heap region in bytes
///
/// # Example
///
/// ```rust,ignore
/// unsafe {
///     watos_mem::heap::init(0x300000, 4 * 1024 * 1024);
/// }
/// ```
pub unsafe fn init(heap_start: usize, heap_size: usize) {
    ALLOCATOR.lock().init(heap_start as *mut u8, heap_size);
}

/// Initialize heap with default configuration
///
/// # Safety
///
/// Same requirements as `init()`. Uses default heap start and size.
pub unsafe fn init_default() {
    init(config::DEFAULT_HEAP_START, config::DEFAULT_HEAP_SIZE);
}

/// Get current heap usage statistics
pub fn stats() -> HeapStats {
    let allocator = ALLOCATOR.lock();
    HeapStats {
        used: allocator.used(),
        free: allocator.free(),
        total: allocator.size(),
    }
}

/// Heap usage statistics
#[derive(Debug, Clone, Copy)]
pub struct HeapStats {
    /// Bytes currently allocated
    pub used: usize,
    /// Bytes available for allocation
    pub free: usize,
    /// Total heap size
    pub total: usize,
}

impl HeapStats {
    /// Get usage percentage (0-100)
    pub fn usage_percent(&self) -> u8 {
        if self.total == 0 {
            return 0;
        }
        ((self.used * 100) / self.total) as u8
    }
}
