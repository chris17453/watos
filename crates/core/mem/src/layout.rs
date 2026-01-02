//! WATOS Memory Layout - Single Source of Truth
//!
//! ALL memory addresses and sizes are defined here.
//! NO magic numbers anywhere else in the codebase.
//!
//! # Physical Memory Layout (what's in RAM)
//!
//! ```text
//! 0x000000 - 0x0FFFFF   Kernel Code (1MB)         Identity mapped
//! 0x100000 - 0x1FFFFF   Boot Data (1MB)           Identity mapped
//! 0x200000 - 0x2FFFFF   Kernel Stacks (1MB)       Identity mapped, 64KB/process
//! 0x300000 - 0x6FFFFF   Kernel Heap (4MB)         Identity mapped
//! 0x700000 - 0x7FFFFF   AHCI DMA (1MB)            Identity mapped
//! 0x800000 - 0xFFFFFF   Reserved (8MB)            Guard zone
//! 0x1000000+            Physical Allocator        User process pages
//! ```
//!
//! # Virtual Address Layout (per process)
//!
//! ```text
//! 0x000000 - 0x3FFFFF   Kernel (4MB)              NOT accessible from Ring 3
//! 0x400000 - 0x7FFFFF   User Code (4MB)           ELF loads here
//! 0x800000 - 0xEFFFFF   User Heap (7MB)           Grows up
//! 0xF00000              Guard Page                Stack overflow trap
//! 0xF01000 - 0xFFFFFF   User Stack (1MB)          Grows down from 0x1000000
//! ```

#![allow(dead_code)]

// =============================================================================
// Page Sizes
// =============================================================================

/// Standard page size (4 KiB)
pub const PAGE_SIZE: usize = 0x1000;
pub const PAGE_SIZE_U64: u64 = PAGE_SIZE as u64;

/// Large page size (2 MiB)
pub const LARGE_PAGE_SIZE: usize = 0x200000;
pub const LARGE_PAGE_SIZE_U64: u64 = LARGE_PAGE_SIZE as u64;

/// Page shift (log2 of PAGE_SIZE)
pub const PAGE_SHIFT: usize = 12;

// =============================================================================
// Physical Memory Regions
// =============================================================================

/// Kernel code region (0 - 1MB)
pub const PHYS_KERNEL_CODE: u64 = 0x000000;
pub const PHYS_KERNEL_CODE_SIZE: u64 = 0x100000;  // 1MB

/// Boot data region (1MB - 2MB)
pub const PHYS_BOOT_DATA: u64 = 0x100000;
pub const PHYS_BOOT_DATA_SIZE: u64 = 0x100000;  // 1MB

/// Kernel stacks region (2MB - 3MB)
/// Each process gets KERNEL_STACK_SIZE bytes
pub const PHYS_KERNEL_STACKS: u64 = 0x200000;
pub const PHYS_KERNEL_STACKS_SIZE: u64 = 0x100000;  // 1MB total

/// Kernel heap region (3MB - 7MB)
pub const PHYS_KERNEL_HEAP: u64 = 0x300000;
pub const PHYS_KERNEL_HEAP_SIZE: u64 = 0x400000;  // 4MB

/// AHCI DMA region (7MB - 8MB)
/// Used for AHCI command lists and FIS buffers
pub const PHYS_AHCI_DMA: u64 = 0x700000;
pub const PHYS_AHCI_DMA_SIZE: u64 = 0x100000;  // 1MB

/// Reserved/guard zone (8MB - 16MB)
pub const PHYS_RESERVED: u64 = 0x800000;
pub const PHYS_RESERVED_SIZE: u64 = 0x800000;  // 8MB

/// Physical allocator start (16MB+)
/// All user process pages are allocated from here
pub const PHYS_ALLOCATOR_START: u64 = 0x1000000;  // 16MB

/// Total identity-mapped region (0 - 8MB)
/// This region is mapped 1:1 (virt = phys) in all page tables
pub const PHYS_IDENTITY_MAP_END: u64 = 0x800000;  // 8MB

// =============================================================================
// Kernel Stack Layout
// =============================================================================

/// Size of each kernel stack (64 KiB per process)
pub const KERNEL_STACK_SIZE: u64 = 0x10000;

/// Maximum number of concurrent processes
pub const MAX_PROCESSES: usize = 16;

/// Get the kernel stack top address for a given PID
/// Stack grows downward, so this is the highest address
#[inline]
pub const fn kernel_stack_top(pid: usize) -> u64 {
    PHYS_KERNEL_STACKS + ((pid as u64 + 1) * KERNEL_STACK_SIZE)
}

/// Get the kernel stack base address for a given PID
#[inline]
pub const fn kernel_stack_base(pid: usize) -> u64 {
    PHYS_KERNEL_STACKS + (pid as u64 * KERNEL_STACK_SIZE)
}

// =============================================================================
// User Virtual Address Layout
// =============================================================================

/// User code/data starts here (4MB)
/// ELF segments are loaded at this virtual address
pub const VIRT_USER_CODE: u64 = 0x400000;

/// User heap starts here (8MB)
pub const VIRT_USER_HEAP: u64 = 0x800000;

/// Guard page for stack overflow detection
pub const VIRT_USER_GUARD: u64 = 0xF00000;

/// User stack base (bottom of stack, just above guard)
pub const VIRT_USER_STACK_BASE: u64 = 0xF01000;

/// User stack top (16MB, stack grows down from here)
pub const VIRT_USER_STACK_TOP: u64 = 0x1000000;

/// User stack size (1MB - 4KB for guard)
pub const VIRT_USER_STACK_SIZE: u64 = VIRT_USER_STACK_TOP - VIRT_USER_STACK_BASE;

/// Maximum user virtual address (anything above 16MB is not used)
pub const VIRT_USER_MAX: u64 = 0x1000000;

// =============================================================================
// Kernel High Canonical Addresses (for syscall access)
// =============================================================================

/// Kernel high address base (maps physical 0-8MB)
/// Used during syscalls to access physical memory
pub const KERNEL_HIGH_BASE: u64 = 0xFFFF_8000_0000_0000;

/// Size of kernel high mapping (8MB, matches identity map)
pub const KERNEL_HIGH_SIZE: u64 = PHYS_IDENTITY_MAP_END;

// =============================================================================
// Helper Functions
// =============================================================================

/// Check if physical address is in the identity-mapped region
#[inline]
pub const fn is_identity_mapped(phys: u64) -> bool {
    phys < PHYS_IDENTITY_MAP_END
}

/// Check if virtual address is in user space (0x400000 - 0x1000000)
#[inline]
pub const fn is_user_virt(virt: u64) -> bool {
    virt >= VIRT_USER_CODE && virt < VIRT_USER_MAX
}

/// Check if virtual address is in kernel space (0 - 0x400000 or high canonical)
#[inline]
pub const fn is_kernel_virt(virt: u64) -> bool {
    virt < VIRT_USER_CODE || virt >= KERNEL_HIGH_BASE
}

/// Align address down to page boundary
#[inline]
pub const fn page_align_down(addr: u64) -> u64 {
    addr & !(PAGE_SIZE_U64 - 1)
}

/// Align address up to page boundary
#[inline]
pub const fn page_align_up(addr: u64) -> u64 {
    (addr + PAGE_SIZE_U64 - 1) & !(PAGE_SIZE_U64 - 1)
}

/// Calculate number of 4KB pages needed for a byte size
#[inline]
pub const fn pages_needed(size: usize) -> usize {
    (size + PAGE_SIZE - 1) / PAGE_SIZE
}

/// Get page frame number from physical address
#[inline]
pub const fn pfn_from_phys(phys: u64) -> usize {
    (phys >> PAGE_SHIFT) as usize
}

/// Get physical address from page frame number
#[inline]
pub const fn phys_from_pfn(pfn: usize) -> u64 {
    (pfn as u64) << PAGE_SHIFT
}

/// Convert physical address to kernel high virtual address
#[inline]
pub const fn phys_to_high_virt(phys: u64) -> u64 {
    KERNEL_HIGH_BASE + phys
}

/// Convert kernel high virtual address to physical
#[inline]
pub const fn high_virt_to_phys(virt: u64) -> u64 {
    virt - KERNEL_HIGH_BASE
}
