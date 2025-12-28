//! ELF64 Parser and Loader
//!
//! Parses ELF64 executables and loads them into memory.

/// ELF64 header
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Header {
    pub magic: [u8; 4],       // 0x7F 'E' 'L' 'F'
    pub class: u8,            // 1 = 32-bit, 2 = 64-bit
    pub endian: u8,           // 1 = little, 2 = big
    pub version: u8,          // ELF version
    pub os_abi: u8,           // OS/ABI identification
    pub abi_version: u8,      // ABI version
    pub _padding: [u8; 7],    // Padding
    pub etype: u16,           // Object file type
    pub machine: u16,         // Machine type
    pub version2: u32,        // Object file version
    pub entry: u64,           // Entry point address
    pub phoff: u64,           // Program header offset
    pub shoff: u64,           // Section header offset
    pub flags: u32,           // Processor-specific flags
    pub ehsize: u16,          // ELF header size
    pub phentsize: u16,       // Program header entry size
    pub phnum: u16,           // Number of program headers
    pub shentsize: u16,       // Section header entry size
    pub shnum: u16,           // Number of section headers
    pub shstrndx: u16,        // Section name string table index
}

/// ELF64 program header
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Phdr {
    pub ptype: u32,           // Segment type
    pub flags: u32,           // Segment flags
    pub offset: u64,          // Offset in file
    pub vaddr: u64,           // Virtual address in memory
    pub paddr: u64,           // Physical address (unused)
    pub filesz: u64,          // Size in file
    pub memsz: u64,           // Size in memory
    pub align: u64,           // Alignment
}

// Program header types
pub const PT_NULL: u32 = 0;
pub const PT_LOAD: u32 = 1;
pub const PT_DYNAMIC: u32 = 2;
pub const PT_INTERP: u32 = 3;

// Program header flags
pub const PF_X: u32 = 1;        // Execute
pub const PF_W: u32 = 2;        // Write  
pub const PF_R: u32 = 4;        // Read
pub const PT_NOTE: u32 = 4;

// ELF machine types
pub const EM_X86_64: u16 = 0x3E;

// ELF types
pub const ET_EXEC: u16 = 2;  // Executable
pub const ET_DYN: u16 = 3;   // Shared object (PIE)

/// Parsed ELF64 information
pub struct Elf64 {
    pub entry: u64,
    pub phdrs: &'static [Elf64Phdr],
    pub is_pie: bool,
}

impl Elf64 {
    /// Parse an ELF64 binary
    pub fn parse(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() < core::mem::size_of::<Elf64Header>() {
            return Err("File too small for ELF header");
        }

        // Safety: We verified the size
        let header = unsafe {
            &*(data.as_ptr() as *const Elf64Header)
        };

        // Validate magic
        if header.magic != [0x7F, b'E', b'L', b'F'] {
            return Err("Invalid ELF magic");
        }

        // Check 64-bit
        if header.class != 2 {
            return Err("Not a 64-bit ELF");
        }

        // Check little-endian
        if header.endian != 1 {
            return Err("Not little-endian");
        }

        // Check x86-64
        if header.machine != EM_X86_64 {
            return Err("Not x86-64");
        }

        // Check executable type
        let is_pie = header.etype == ET_DYN;
        if header.etype != ET_EXEC && header.etype != ET_DYN {
            return Err("Not an executable");
        }

        // Get program headers
        let ph_offset = header.phoff as usize;
        let ph_count = header.phnum as usize;
        let ph_size = header.phentsize as usize;

        if ph_offset + ph_count * ph_size > data.len() {
            return Err("Program headers outside file");
        }

        // Create slice of program headers
        let phdrs = unsafe {
            core::slice::from_raw_parts(
                data.as_ptr().add(ph_offset) as *const Elf64Phdr,
                ph_count
            )
        };

        Ok(Elf64 {
            entry: header.entry,
            phdrs,
            is_pie,
        })
    }

    /// Load program segments into memory
    pub fn load_segments(&self, data: &[u8], load_base: u64) -> Result<(), &'static str> {
        // Find the lowest vaddr to calculate offset
        let min_vaddr = self.phdrs.iter()
            .filter(|p| p.ptype == PT_LOAD)
            .map(|p| p.vaddr)
            .min()
            .unwrap_or(0);

        for phdr in self.phdrs {
            if phdr.ptype != PT_LOAD {
                continue;
            }

            // Always relocate: calculate offset from min_vaddr and add to load_base
            // This handles both PIE and non-PIE binaries
            let offset = phdr.vaddr - min_vaddr;
            let dest_addr = load_base + offset;

            let src_offset = phdr.offset as usize;
            let file_size = phdr.filesz as usize;
            let mem_size = phdr.memsz as usize;

            // Validate source range
            if src_offset + file_size > data.len() {
                return Err("Segment outside file");
            }

            // Debug: print segment info
            unsafe {
                let port: u16 = 0x3F8;
                let msg = b"SEG: off=";
                for &b in msg { core::arch::asm!("out dx, al", in("dx") port, in("al") b, options(nostack)); }
                let hex = b"0123456789ABCDEF";
                for i in (0..8).rev() {
                    let n = ((src_offset as u64 >> (i*4)) & 0xF) as usize;
                    core::arch::asm!("out dx, al", in("dx") port, in("al") hex[n], options(nostack));
                }
                let msg2 = b" dest=";
                for &b in msg2 { core::arch::asm!("out dx, al", in("dx") port, in("al") b, options(nostack)); }
                for i in (0..16).rev() {
                    let n = ((dest_addr >> (i*4)) & 0xF) as usize;
                    core::arch::asm!("out dx, al", in("dx") port, in("al") hex[n], options(nostack));
                }
                let msg3 = b" sz=";
                for &b in msg3 { core::arch::asm!("out dx, al", in("dx") port, in("al") b, options(nostack)); }
                for i in (0..8).rev() {
                    let n = ((file_size as u64 >> (i*4)) & 0xF) as usize;
                    core::arch::asm!("out dx, al", in("dx") port, in("al") hex[n], options(nostack));
                }
                core::arch::asm!("out dx, al", in("dx") port, in("al") b'\r', options(nostack));
                core::arch::asm!("out dx, al", in("dx") port, in("al") b'\n', options(nostack));
            }

            // Copy segment data
            unsafe {
                let dest = dest_addr as *mut u8;
                let src = data.as_ptr().add(src_offset);

                let port: u16 = 0x3F8;
                let hex = b"0123456789ABCDEF";

                // Write test value and immediately read back
                *dest = 0xAB;
                let written = *dest;
                let msg = b"W_TEST: ";
                for &b in msg { core::arch::asm!("out dx, al", in("dx") port, in("al") b, options(nostack)); }
                core::arch::asm!("out dx, al", in("dx") port, in("al") hex[(written >> 4) as usize], options(nostack));
                core::arch::asm!("out dx, al", in("dx") port, in("al") hex[(written & 0xF) as usize], options(nostack));
                // Also print dest address
                let msg2 = b" @";
                for &b in msg2 { core::arch::asm!("out dx, al", in("dx") port, in("al") b, options(nostack)); }
                for i in (0..16).rev() {
                    let n = ((dest_addr >> (i*4)) & 0xF) as usize;
                    core::arch::asm!("out dx, al", in("dx") port, in("al") hex[n], options(nostack));
                }
                core::arch::asm!("out dx, al", in("dx") port, in("al") b'\r', options(nostack));
                core::arch::asm!("out dx, al", in("dx") port, in("al") b'\n', options(nostack));

                // Copy first 4 bytes with verbose debugging
                let msg = b"CPY: ";
                for &b in msg { core::arch::asm!("out dx, al", in("dx") port, in("al") b, options(nostack)); }
                for i in 0..4 {
                    let src_byte = core::ptr::read_volatile(src.add(i));
                    core::ptr::write_volatile(dest.add(i), src_byte);
                    let dst_byte = core::ptr::read_volatile(dest.add(i));
                    // Print: src->dst
                    core::arch::asm!("out dx, al", in("dx") port, in("al") hex[(src_byte >> 4) as usize], options(nostack));
                    core::arch::asm!("out dx, al", in("dx") port, in("al") hex[(src_byte & 0xF) as usize], options(nostack));
                    core::arch::asm!("out dx, al", in("dx") port, in("al") b'>', options(nostack));
                    core::arch::asm!("out dx, al", in("dx") port, in("al") hex[(dst_byte >> 4) as usize], options(nostack));
                    core::arch::asm!("out dx, al", in("dx") port, in("al") hex[(dst_byte & 0xF) as usize], options(nostack));
                    core::arch::asm!("out dx, al", in("dx") port, in("al") b' ', options(nostack));
                }
                core::arch::asm!("out dx, al", in("dx") port, in("al") b'\r', options(nostack));
                core::arch::asm!("out dx, al", in("dx") port, in("al") b'\n', options(nostack));

                // Copy rest
                for i in 4..file_size {
                    core::ptr::write_volatile(dest.add(i), core::ptr::read_volatile(src.add(i)));
                }

                // Zero BSS (memory beyond file data)
                if mem_size > file_size {
                    core::ptr::write_bytes(dest.add(file_size), 0, mem_size - file_size);
                }
            }
        }

        Ok(())
    }

    /// Load ELF segments with memory protection via page tables
    ///
    /// Allocates fresh physical pages for each segment to ensure process isolation.
    pub fn load_segments_protected(&self, data: &[u8], load_base: u64, page_table: &mut watos_mem::paging::ProcessPageTable) -> Result<(), &'static str> {
        use watos_mem::paging::{flags as page_flags, PAGE_SIZE};
        use super::{debug_serial, debug_hex};

        unsafe {
            debug_serial(b"load_segments: phdrs ptr=0x");
            debug_hex(self.phdrs.as_ptr() as u64);
            debug_serial(b" len=");
            debug_hex(self.phdrs.len() as u64);
            debug_serial(b"\r\n");
        }

        let min_vaddr = self.phdrs.iter()
            .filter(|p| p.ptype == PT_LOAD)
            .map(|p| p.vaddr)
            .min()
            .unwrap_or(0);

        unsafe {
            debug_serial(b"load_segments: min_vaddr=0x");
            debug_hex(min_vaddr);
            debug_serial(b"\r\n");
        }

        for phdr in self.phdrs {
            if phdr.ptype != PT_LOAD {
                continue;
            }

            let src_offset = phdr.offset as usize;
            let file_size = phdr.filesz as usize;
            let mem_size = phdr.memsz as usize;

            // Calculate virtual address:
            // - For PIE: relocate relative to load_base
            // - For non-PIE: use original vaddr (code has absolute addresses)
            let virt_addr = if self.is_pie {
                load_base + (phdr.vaddr - min_vaddr)
            } else {
                phdr.vaddr
            };

            unsafe {
                debug_serial(b"  segment: vaddr=0x");
                debug_hex(phdr.vaddr);
                debug_serial(b" dest=0x");
                debug_hex(virt_addr);
                debug_serial(b" offset=0x");
                debug_hex(phdr.offset);
                debug_serial(b" filesz=0x");
                debug_hex(phdr.filesz);
                debug_serial(b"\r\n");
            }

            // Validate source
            if src_offset + file_size > data.len() {
                return Err("Segment outside file");
            }

            // Calculate pages needed
            let page_start = virt_addr & !(PAGE_SIZE as u64 - 1);
            let page_end = (virt_addr + mem_size as u64 + PAGE_SIZE as u64 - 1) & !(PAGE_SIZE as u64 - 1);
            let num_pages = ((page_end - page_start) / PAGE_SIZE as u64) as usize;

            // Determine page flags from ELF flags
            let mut flags = page_flags::PRESENT;
            if phdr.flags & PF_W != 0 {
                flags |= page_flags::WRITABLE;
            }

            unsafe {
                debug_serial(b"  copying from 0x");
                debug_hex(data.as_ptr() as u64 + src_offset as u64);
                debug_serial(b" to 0x");
                debug_hex(virt_addr);
                debug_serial(b"\r\n  copying ");
                debug_hex(file_size as u64);
                debug_serial(b" bytes...\r\n");
            }

            // For each page needed:
            // 1. Check if page is already mapped (from previous segment)
            // 2. Allocate a fresh physical page
            // 3. Copy existing page contents if overlapping (preserve previous segment data)
            // 4. Copy new segment data to it
            // 5. Map virtual address to physical page
            for i in 0..num_pages {
                let page_virt = page_start + (i as u64 * PAGE_SIZE as u64);

                // Check if this page is already mapped from a previous segment
                let existing_phys = page_table.lookup(page_virt);

                // Allocate physical page
                let phys_page = watos_mem::phys::alloc_page()
                    .ok_or("Out of physical memory")?;

                if i == 0 {
                    unsafe {
                        debug_serial(b"  phys page[0]=0x");
                        debug_hex(phys_page);
                        debug_serial(b" -> virt=0x");
                        debug_hex(page_virt);
                        if existing_phys.is_some() {
                            debug_serial(b" (overlap from 0x");
                            debug_hex(existing_phys.unwrap());
                            debug_serial(b")");
                        }
                        debug_serial(b"\r\n");
                    }
                }

                // Calculate what part of the segment goes in this page
                let page_offset = if page_virt < virt_addr { 0 } else { page_virt - virt_addr };
                let dest_in_page = if page_virt < virt_addr { (virt_addr - page_virt) as usize } else { 0 };

                // If this page was already mapped, copy existing contents first
                // Otherwise, zero the page
                unsafe {
                    if let Some(old_phys) = existing_phys {
                        // Copy existing page contents to preserve data from previous segment
                        core::ptr::copy_nonoverlapping(
                            old_phys as *const u8,
                            phys_page as *mut u8,
                            PAGE_SIZE
                        );
                    } else {
                        // Zero the new page
                        core::ptr::write_bytes(phys_page as *mut u8, 0, PAGE_SIZE);
                    }
                }

                // Copy file data if this page contains any
                if page_offset < file_size as u64 {
                    let copy_start = page_offset as usize;
                    let copy_end = ((page_offset as usize + PAGE_SIZE - dest_in_page).min(file_size)).min(copy_start + PAGE_SIZE - dest_in_page);
                    let copy_len = copy_end.saturating_sub(copy_start);

                    if copy_len > 0 {
                        unsafe {
                            let src = data.as_ptr().add(src_offset + copy_start);
                            let dest = (phys_page as *mut u8).add(dest_in_page);

                            // Debug: if this is the .got segment (small size), print source value
                            if file_size < 0x20 && i == 0 {
                                debug_serial(b"  GOT debug: src=0x");
                                debug_hex(src as u64);
                                debug_serial(b" val=0x");
                                debug_hex(*(src as *const u64));
                                debug_serial(b" dest=0x");
                                debug_hex(dest as u64);
                                debug_serial(b"\r\n");
                            }

                            core::ptr::copy_nonoverlapping(src, dest, copy_len);

                            // Debug: verify the copy worked for .got segment
                            if file_size < 0x20 && i == 0 {
                                debug_serial(b"  GOT after: val=0x");
                                debug_hex(*(dest as *const u64));
                                debug_serial(b"\r\n");
                            }
                        }
                    }
                }

                // Zero BSS portion: memory beyond file_size up to mem_size
                // This handles the case where memsz > filesz (.bss section)
                if mem_size > file_size {
                    // Calculate what BSS portion falls in this page
                    let bss_start_in_seg = file_size as u64;
                    let bss_end_in_seg = mem_size as u64;

                    // This page covers segment offsets from page_offset to page_offset + PAGE_SIZE
                    let page_end_offset = page_offset + (PAGE_SIZE - dest_in_page) as u64;

                    // BSS in this page: overlap of [bss_start_in_seg, bss_end_in_seg) and [page_offset, page_end_offset)
                    let bss_in_page_start = bss_start_in_seg.max(page_offset);
                    let bss_in_page_end = bss_end_in_seg.min(page_end_offset);

                    if bss_in_page_end > bss_in_page_start {
                        let zero_offset_in_page = dest_in_page + (bss_in_page_start - page_offset) as usize;
                        let zero_len = (bss_in_page_end - bss_in_page_start) as usize;
                        unsafe {
                            core::ptr::write_bytes(
                                (phys_page as *mut u8).add(zero_offset_in_page),
                                0,
                                zero_len
                            );
                        }
                    }
                }

                // Map virtual page to physical page in process's page table
                page_table.map_user_page(page_virt, phys_page, flags)?;
            }
        }

        Ok(())
    }
}
