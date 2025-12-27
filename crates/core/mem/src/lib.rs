//! WATOS Memory Management
//!
//! This crate provides memory management primitives for WATOS:
//! - Heap allocation via linked_list_allocator
//! - Page table management for x86_64
//! - Physical memory allocation
//!
//! # Usage
//!
//! ```rust,ignore
//! use watos_mem::{heap, paging};
//!
//! // Initialize heap allocator
//! unsafe {
//!     heap::init(0x300000, 4 * 1024 * 1024);
//! }
//!
//! // Create process page table
//! let mut page_table = paging::ProcessPageTable::new();
//! page_table.map_user_page(0x400000, physical_addr, paging::PAGE_USER | paging::PAGE_WRITABLE);
//! ```

#![no_std]

extern crate alloc;

pub mod heap;
pub mod paging;
pub mod phys;

// Re-export commonly used items
pub use heap::{init as init_heap, ALLOCATOR};
pub use paging::{ProcessPageTable, PageTable, PAGE_SIZE};
pub use paging::flags as page_flags;
