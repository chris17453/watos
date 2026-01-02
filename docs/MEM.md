# Virtual Memory ... full spec + tasks (x86_64 + AArch64)

---

## 0) Deliverables
You are done with "VM Phase 2" when you have:
- Per-process address spaces (exec + fork) with correct isolation
- Demand-zero anon paging
- COW fork
- mmap/munmap/mprotect
- Kernel higher-half mapping + direct map + vmalloc
- Correct TLB invalidation (single-core), and shootdowns (SMP phase)
- Deterministic debug tools to inspect translations and regions

---

## 1) Invariants (write these down and enforce them)
- Kernel VA range is never user-accessible.
- User VA range never aliases kernel physical pages via USER=1 mapping.
- W^X: no mapping is writable and executable at the same time.
- NX on by default; executable only where explicitly required.
- Null page unmapped; guard pages for stacks.
- Every map/unmap/protect that changes translation or perms performs required TLB invalidation.
- Page table memory itself is never user-accessible.
- Kernel never directly dereferences user pointers; only copyin/copyout paths may touch user pages.
- Refcounted frames: shared frames are never freed early; refcount never underflows.

Tasks
- [ ] Encode these as assertions in debug builds
- [ ] Add a "VM invariant check" function callable at runtime

---

## 2) Address space layout (policy, not hardware)
Pick constants now. Do not improvise later.

### 2.1 x86_64 (48-bit canonical, 4-level)
- User: `0x0000_0000_0000_0000 .. 0x0000_7fff_ffff_ffff`
- Kernel: `0xffff_8000_0000_0000 .. 0xffff_ffff_ffff_ffff`

Kernel subranges (define explicit bases/sizes)
- Kernel image: `KERNEL_BASE .. KERNEL_END`
- Direct map: `DIRECT_BASE .. DIRECT_BASE + max_phys`
- Vmalloc: `VMALLOC_BASE .. VMALLOC_END`
- Fixmap: `FIXMAP_BASE .. FIXMAP_END`
- Vmemmap (optional): `VMEMMAP_BASE .. VMEMMAP_END`

### 2.2 AArch64 (typical 48-bit VA, 4-level, 4 KiB granule)
- TTBR0: user space (low range)
- TTBR1: kernel space (high range)

Define user/kernel split via TCR:
- user top implied by T0SZ
- kernel base implied by T1SZ

Kernel subranges (same concepts)
- Kernel image
- Direct map
- Vmalloc
- Fixmap
- Optional vmemmap

Tasks
- [ ] Choose 4 KiB granule on AArch64
- [ ] Define a shared "vm_layout.h" with all constants per arch
- [ ] Add validation that VMAs never exceed user range

---

## 3) Physical memory subsystem (frames)
### 3.1 Inputs
- Firmware memory map (UEFI) -> usable RAM regions
- Reserved regions:
  - kernel image + initrd
  - boot-time page tables
  - firmware reserved
  - MMIO apertures

### 3.2 Allocator
Minimum:
- bootstrap bump allocator for early boot
- real allocator for runtime (buddy recommended)

### 3.3 Frame metadata (must have)
- refcount (atomic once SMP)
- flags: free, reserved, slab, page_table, anon, file, wired
- order (if buddy)
- optional owner tag (pid or subsystem id) for debugging

Tasks
- [ ] parse_memory_map() -> physical_ranges
- [ ] reserve_range(pa_start, pa_end, reason)
- [ ] alloc_frame(order=0) / free_frame(order=0)
- [ ] frame_ref_inc / frame_ref_dec_free
- [ ] page_zero(frame) and page_copy(dst, src)
- [ ] expose /proc-like debug counters later; for now kernel log commands

---

## 4) Virtual region tracking (VMA layer)
### 4.1 VMA types
- anon private (heap, stack, mmap anon)
- file-backed private (exec segments, mmap file private) (later)
- file-backed shared (mmap shared) (later)
- special mappings: vdso/vvar (optional), shared memory, device mappings

### 4.2 Required operations
- find(addr) -> vma
- insert(range, prot, flags, backing)
- remove(range) (partial remove requires split)
- protect(range) (partial protect requires split)
- merge adjacent compatible VMAs
- find_gap(size, align, hint)

Data structure
- RB-tree or interval tree for find(addr)
- linked list for iteration and merge

Tasks
- [ ] vma_find(as, va)
- [ ] vma_insert(as, vma)
- [ ] vma_remove_range(as, start, end)
- [ ] vma_protect_range(as, start, end, prot)
- [ ] vma_find_gap(as, size, align, hint)
- [ ] vma_dump(as) debug output

---

## 5) Page tables (hardware translation layer)

### 5.1 Required generic API
- pt_walk(as, va, create=false) -> (pte_ptr, level)
- pt_map_page(as, va, pa, flags)
- pt_unmap_page(as, va)
- pt_protect_page(as, va, flags)
- pt_map_range(as, va, pa, len, flags)
- pt_unmap_range(as, va, len)
- pt_clone_user_cow(parent_as) -> child_as (or clone into new root)
- pt_destroy_user(as)
- as_activate(as) (switch root + asid)

### 5.2 Flags you must model (arch-neutral)
- present/valid
- user
- write
- exec (or NX)
- accessed (optional now, useful later)
- dirty (optional now, useful later)
- global (kernel only)
- memory type (normal vs device)
- software flags:
  - cow
  - wired/pinned
  - no_dump (optional)

Tasks
- [ ] define common vm_flags -> arch_pte_bits mapping
- [ ] prevent illegal combinations (USER on kernel range, W+X, exec on device maps)
- [ ] implement page table allocation using frames tagged page_table

---

## 6) Fault handling (the real VM)
### 6.1 Fault reasons (normalize per arch)
- translation fault (not-present)
- permission fault (read/write/exec)
- alignment fault
- access-flag fault (if you use AF on AArch64)
- reserved-bit / malformed entry

### 6.2 Common fault handler flow (must implement)
1. capture: pid, pc, sp, va, reason, access type (R/W/X), user/kernel mode
2. if kernel-mode fault:
   - if in copyin/copyout region: recover and return error
   - else: panic with dump (bug)
3. if user-mode:
   - vma = vma_find(as, va)
   - if none: signal/kill
4. if translation fault:
   - if anon: alloc frame, zero, map with vma prot
   - if file-backed: bring from page cache (phase 4)
5. if permission fault:
   - if write and COW: allocate new frame, copy, remap writable, dec old ref
   - else: signal/kill

Tasks
- [ ] fault_decode_x86_64() -> common_fault
- [ ] fault_decode_aarch64() -> common_fault
- [ ] anon_demand_zero_fault()
- [ ] cow_write_fault()
- [ ] copyin/copyout fault recovery hooks
- [ ] fault ring-buffer logger (fixed size, per-cpu later)

---

## 7) TLB management (correctness critical)
### 7.1 Required ops
- invalidate single VA in an AS
- invalidate whole AS
- invalidate kernel global mappings (rare)
- SMP shootdown for any AS-local invalidation when other cores may run it

### 7.2 Rules
- Map/unmap/protect require invalidation before returning to user.
- On AArch64, TLBI sequences require barriers.
- On x86_64, INVLPG is enough per-page; CR3 reload flushes per-AS; global pages need special handling.

Tasks
- [ ] tlb_invalidate_page(as, va)
- [ ] tlb_invalidate_as(as)
- [ ] tlb_invalidate_kernel_global()
- [ ] smp_tlb_shootdown(as, va or range)
- [ ] debug counters for tlb ops

---

## 8) Context switch integration
### 8.1 x86_64
- Switch CR3 to new PML4 physical address.
- Optionally use PCID later.

### 8.2 AArch64
- Switch TTBR0_EL1 for user, TTBR1_EL1 generally constant (kernel).
- Use ASIDs to avoid full flush on switch.

Tasks
- [ ] as_activate_x86_64(as)
- [ ] as_activate_aarch64(as)
- [ ] asid allocator (AArch64 phase 2, optional phase 1 if you just flush)

---

## 9) User-facing VM syscalls / primitives (minimum)
- brk/sbrk
- mmap/munmap (anon first)
- mprotect
- madvise (optional)
- mincore (optional)
- setrlimit/getrlimit (optional but helpful for OOM)

Tasks
- [ ] sys_brk updates heap VMA and maps on fault (preferred) or immediate map
- [ ] sys_mmap_anon creates VMA and returns VA (no immediate map required)
- [ ] sys_munmap removes mappings and VMAs
- [ ] sys_mprotect changes prot, updates PTEs, invalidates TLB

---

## 10) fork/exec/exit interactions (must be explicit)
### 10.1 exec
- new address space
- map ELF:
  - text RX
  - rodata R
  - data RW
  - bss RW (demand-zero ok)
- stack RW with guard page(s)
- initial heap VMA
- set entry point + initial user stack content

### 10.2 fork (COW)
- copy VMA metadata
- clone page tables:
  - mark private writable pages as read-only in both
  - set COW flag in metadata or spare PTE bits
  - increment frame refcount
- child inherits mappings and continues

### 10.3 exit
- decrement refcounts for mapped frames
- free page tables
- free VMA structures

Tasks
- [ ] elf_map_segments(as, elf)
- [ ] build_initial_user_stack(as, argv, envp, auxv)
- [ ] fork_clone_vmas(parent, child)
- [ ] fork_clone_pt_cow(parent, child)
- [ ] as_destroy(as)

---

## 11) Kernel memory model (must build all 3)
### 11.1 Direct map
- A linear VA window covering all RAM:
  - `kva = DIRECT_BASE + pa`
- Used for:
  - quick access to frames
  - backing small allocations
  - page table memory access

Tasks
- [ ] set DIRECT_BASE and map all RAM (or map discovered RAM ranges)
- [ ] implement pa_to_kva / kva_to_pa helpers
- [ ] use large pages later for performance

### 11.2 Slab/kmalloc
- Small allocations (objects) backed by frames
- Uses direct map pages

Tasks
- [ ] implement slab caches (size classes) or simple segregated allocator
- [ ] support allocations from interrupt context if needed (later)

### 11.3 Vmalloc
- Virtually contiguous allocations backed by non-contiguous frames
- Needed for:
  - large kernel buffers
  - module-like mappings
  - mapping large IO buffers

Tasks
- [ ] vmalloc_va_allocator (bitmap or RB-tree)
- [ ] vmalloc_map_pages(range, frames)
- [ ] vfree

### 11.4 Fixmap / temp map
- Reserved VA range for temporary mappings:
  - early boot page table construction
  - occasional mapping of a single PA

Tasks
- [ ] fixmap_map(pa, flags) -> va
- [ ] fixmap_unmap(va)

---

## 12) Memory types / attributes (RAM vs device)
You must handle device mappings correctly.

### x86_64
- PAT / PCD / PWT bits for cacheability
- ioremap should map as uncached or WC depending

### AArch64
- MAIR_EL1 defines memory types
- PTE AttrIndx selects type
- Shareability bits set correctly
- Device memory must be mapped as Device-nGnRnE (or your chosen device type)

Tasks
- [ ] define memory types: NORMAL, DEVICE, WC(optional)
- [ ] implement ioremap(pa, size, type) in vmalloc/fixmap region
- [ ] verify device mappings never executable

---

## 13) Large pages / block mappings (phase 4+)
x86_64
- 2 MiB (PD PS bit), 1 GiB (PDPT PS bit)

AArch64
- block mappings at higher levels depending on granule

Tasks
- [ ] huge page mapping API
- [ ] split/merge logic and fallback to 4 KiB pages
- [ ] correct TLB invalidation for block mappings
- [ ] use huge pages for direct map first

---

## 14) SMP specifics (phase 3)
- Per-cpu page fault stacks (kernel)
- Per-cpu TLB shootdown handling
- Locks:
  - VMA tree lock per process
  - page table lock per process (or finer-grained)
  - frame allocator lock (or per-cpu caches later)
- Atomic refcounts for frames
- Barriers:
  - AArch64 requires careful DSB/ISB around TLBI

Tasks
- [ ] implement IPI mechanism for shootdowns
- [ ] implement shootdown ack/wait
- [ ] make refcounts atomic
- [ ] define locking order to avoid deadlocks

---

## 15) Hardening knobs (optional but recommended)
x86_64
- CR0.WP on
- NXE on
- SMEP/SMAP later
- KPTI later

AArch64
- PXN/UXN always
- PAN later
- ASIDs early if possible

Tasks
- [ ] enable NX everywhere
- [ ] enforce W^X
- [ ] later: implement SMEP/SMAP or PAN equivalents

---

## 16) Debug tools and tests (mandatory)
### 16.1 Debug commands
- vm_dump_vmas(pid)
- vm_dump_pte(pid, va)
- vm_dump_fault_log()
- vm_dump_frame(pa or frame_id)
- vm_check_invariants()

Tasks
- [ ] implement dumps
- [ ] add panic path that prints: pc, sp, va, pte chain, vma

### 16.2 Regression tests (userland)
- demand-zero: map 1 GiB anon, touch sparse pages
- COW: fork, write half pages, validate isolation
- mmap stress: 10k regions, random unmap/protect
- NX: write code to data page and attempt exec
- boundary: attempt read/write kernel VA in user mode
- TLB: map/unmap same VA repeatedly and verify no stale access

Tasks
- [ ] add a vmtest binary that runs all tests at boot
- [ ] add a kernel boot arg to run vmtests automatically

---

# ARCH NOTES ... x86_64

## X1) Page table entry bits you must support
- P (present)
- RW
- US
- NX (requires EFER.NXE)
- G (global) for kernel mappings
- A (accessed) and D (dirty) if you track them

Tasks
- [ ] enable EFER.NXE
- [ ] enable CR0.WP
- [ ] define pte helpers for set/clear/test bits

## X2) Fault decode inputs
- CR2 provides fault VA
- error code provides:
  - P (present)
  - W/R
  - U/S
  - I/D (instruction fetch) if supported
  - reserved-bit violation

Tasks
- [ ] normalize error code -> common fault type

## X3) TLB ops
- INVLPG for a VA
- CR3 reload for flush
- Global flush if you use G bit

Tasks
- [ ] implement tlb ops now, PCID later

---

# ARCH NOTES ... AArch64

## A1) Registers you must configure
- MAIR_EL1 (memory attribute table)
- TCR_EL1 (sizes, granule, cacheability, shareability)
- TTBR0_EL1 (user root)
- TTBR1_EL1 (kernel root)
- SCTLR_EL1 (MMU enable + cache policy)

Tasks
- [ ] choose T0SZ/T1SZ values that match your VA split
- [ ] define MAIR entries for NORMAL and DEVICE
- [ ] enable MMU with correct barriers (DSB/ISB)

## A2) Descriptor bits you must support
- valid/table/page
- AP bits (EL0 access + R/W)
- UXN/PXN
- AttrIndx (MAIR selector)
- SH (shareability)
- AF (access flag) ... either manage it or set to avoid AF faults initially

Tasks
- [ ] pick a policy: set AF on map to avoid AF faults at first

## A3) Fault decode inputs
- FAR_EL1 is fault VA
- ESR_EL1 provides EC/ISS to classify translation vs permission vs access-flag faults

Tasks
- [ ] normalize ESR -> common fault type

## A4) TLB ops and barriers
- TLBI by VA+ASID and by ASID
- Required barriers:
  - DSB ishst/ish around TLBI
  - ISB after

Tasks
- [ ] implement tlb ops with correct barrier sequence
- [ ] implement ASID allocator or flush-all on switch (phase 1)

---

## Phase breakdown (assignable)
### Phase 1 ... paging on, anon demand-zero, one process
- [ ] frame allocator bootstrap
- [ ] page table build + MMU enable (both arch)
- [ ] kernel higher-half + direct map
- [ ] VMA minimal + page fault anon
- [ ] copyin/copyout recovery
- [ ] tlb invalidation single-core

### Phase 2 ... exec/fork/COW/mmap
- [ ] VMA full ops
- [ ] brk + mmap + mprotect
- [ ] exec loader
- [ ] fork COW
- [ ] slab + vmalloc

### Phase 3 ... SMP correctness
- [ ] shootdowns
- [ ] atomic refcounts
- [ ] locks

### Phase 4 ... file-backed paging + cache + optional swap
- [ ] page cache
- [ ] file-backed VMAs
- [ ] demand paging from FS
- [ ] writeback (optional)
- [ ] huge pages (optional)

