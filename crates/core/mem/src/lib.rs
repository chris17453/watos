//! WATOS Memory Management
//!
//! Unified memory management for WATOS:
//! - `layout` - Single source of truth for all memory addresses
//! - `pmm` - Physical Memory Manager (allocates 4KB pages)
//! - `vmm` - Virtual Memory Manager (per-process address spaces)
//! - `paging` - Low-level x86_64 page table operations
//! - `heap` - Kernel heap allocation
//!
//! # Architecture
//!
//! ```text
//! layout.rs  ─── Defines all memory addresses (NO magic numbers elsewhere)
//!    │
//!    ├── pmm.rs  ─── Physical page allocator (bitmap-based)
//!    │
//!    ├── paging.rs ─── x86_64 page tables (PML4/PDP/PD/PT)
//!    │
//!    └── vmm.rs ─── High-level VMM per process (wraps paging + pmm)
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use watos_mem::{layout, pmm, vmm};
//!
//! // Initialize PMM (usually from UEFI memory map)
//! pmm::init(layout::PHYS_ALLOCATOR_START, available_ram);
//!
//! // Create process with VMM
//! let mut vmm = vmm::VirtualMemoryManager::new();
//! vmm.map_user_code(layout::VIRT_USER_CODE, phys_addr, size, true, true)?;
//! vmm.map_user_stack(None)?;
//! ```

#![no_std]

extern crate alloc;

// Core modules
pub mod layout;
pub mod pmm;
pub mod paging;
pub mod vmm;
pub mod heap;
pub mod user_access;

// Re-export commonly used items
pub use layout::{PAGE_SIZE, LARGE_PAGE_SIZE};
pub use layout::{VIRT_USER_CODE, VIRT_USER_HEAP, VIRT_USER_STACK_TOP};
pub use layout::{PHYS_KERNEL_HEAP, PHYS_KERNEL_HEAP_SIZE, PHYS_ALLOCATOR_START};
pub use heap::{init as init_heap, ALLOCATOR};
pub use paging::{ProcessPageTable, PageTable};
pub use paging::flags as page_flags;
pub use vmm::VirtualMemoryManager;
pub use user_access::{validate_user_ptr, read_user_string, copy_from_user, copy_to_user, UserAccessError};
pub use user_access::{stac, clac, with_user_access};
