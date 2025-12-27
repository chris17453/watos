# Unimplemented Features in WATOS

This document catalogs all features that have stub implementations or are marked as TODO/unimplemented in the WATOS codebase. These are placeholders that return fallback values or no-op implementations.

## Graphics Features

### VGA/Framebuffer Operations

**File**: `src/interrupts.rs:571-573`
```rust
fn vga_get_pixel(_x: i32, _y: i32) -> u8 {
    // Would need to read back from framebuffer - not implemented yet
    0
}
```
- **Status**: Returns hardcoded 0
- **Impact**: Cannot read pixel values from the framebuffer
- **Required for**: Graphics programs that need to sample existing pixels (e.g., collision detection)

**File**: `src/main.rs:1694-1698`
```rust
pub extern "C" fn watos_get_pixel(_x: i32, _y: i32) -> u8 {
    0 // Graphics not implemented yet
}
```
- **Status**: Returns hardcoded 0
- **Impact**: Duplicate of above for C interface
- **Required for**: Native applications using pixel sampling

**File**: `src/interrupts.rs:451-455`
```rust
syscall::SYS_VGA_SET_PALETTE => {
    // Set palette: rdi=index, rsi=r, rdx=g, r10=b
    // TODO: implement palette
    0
}
```
- **Status**: No-op syscall
- **Impact**: Cannot modify color palette for indexed color modes
- **Required for**: Games and graphics programs using palette-based graphics

**File**: `crates/gwbasic/src/watos_main.rs:246-250`
```rust
pub extern "C" fn watos_get_pixel(_x: i32, _y: i32) -> u8 {
    0 // Not implemented
}
```
- **Status**: Returns hardcoded 0
- **Impact**: GWBASIC programs cannot read pixels
- **Required for**: BASIC graphics programs using POINT() function

## File I/O Features

### GWBASIC File Operations

**File**: `crates/gwbasic/src/watos_main.rs:174-196`

Multiple file operation stubs:

```rust
pub extern "C" fn watos_file_open(_path: *const u8, _len: usize, _mode: u64) -> i64 {
    -1 // Not implemented - return error
}

pub extern "C" fn watos_file_close(_handle: i64) {
    // Not implemented
}

pub extern "C" fn watos_file_read(_handle: i64, _buf: *mut u8, _len: usize) -> usize {
    0 // Not implemented
}

pub extern "C" fn watos_file_write(_handle: i64, _buf: *const u8, _len: usize) -> usize {
    0 // Not implemented
}
```
- **Status**: All file operations return errors or zero
- **Impact**: GWBASIC programs cannot perform file I/O
- **Required for**: BASIC programs using OPEN, CLOSE, GET, PUT, INPUT#, PRINT#

### GWBASIC Program Management

**File**: `crates/gwbasic/src/interpreter.rs:668-692`

```rust
AstNode::Load(_filename) => {
    // Load program from file - stub implementation
    console_println!("LOAD: Feature not yet fully implemented");
    Ok(())
}

AstNode::Save(_filename) => {
    // Save program to file - stub implementation
    console_println!("SAVE: Feature not yet fully implemented");
    Ok(())
}

AstNode::Merge(_filename) => {
    // Merge program from file - stub implementation
    console_println!("MERGE: Feature not yet fully implemented");
    Ok(())
}

AstNode::Chain(_filename, _line) => {
    // Chain to another program - stub implementation
    console_println!("CHAIN: Feature not yet fully implemented");
    Ok(())
}

AstNode::Cont => {
    // Continue execution - stub implementation
    console_println!("CONT: Feature not yet fully implemented");
    Ok(())
}
```
- **Status**: Print message and return OK without doing anything
- **Impact**: Cannot save/load BASIC programs, chain to other programs, or continue after break
- **Required for**: Basic IDE functionality, program persistence

### DOS16 File Operations

**File**: `src/runtime/dos16.rs:3386`
```rust
fn dos_close_file(&mut self, handle: u16) {
    if (handle as usize) < 20 {
        // If writable and modified, write back to disk (TODO)
        self.file_handles[handle as usize] = None;
    }
}
```
- **Status**: File handles are closed but changes are not flushed to disk
- **Impact**: File modifications may be lost
- **Required for**: Reliable file writing in DOS programs

## DOS Interrupt Handlers

### INT 16h Keyboard Services

**File**: `src/runtime/dos16.rs:2970-2975`
```rust
0x00 | 0x10 => {
    // Get key - return 0 for now (no key)
    // TODO: Hook into actual keyboard input
    self.cpu.ax = 0;
}
```
- **Status**: Always returns "no key available"
- **Impact**: DOS programs cannot read keyboard input via INT 16h
- **Required for**: DOS programs using BIOS keyboard services instead of DOS input

### INT 21h DOS Services

**File**: `src/runtime/dos16.rs:3240-3248`
```rust
0x4E => {
    // Find first file (FCB) - not implemented
    self.cpu.set_flag(FLAG_CF, true);
    self.cpu.ax = 0x12; // No more files
}
0x4F => {
    // Find next file (FCB) - not implemented
    self.cpu.set_flag(FLAG_CF, true);
    self.cpu.ax = 0x12; // No more files
}
```
- **Status**: Always returns "no more files" error
- **Impact**: Cannot enumerate files using FCB-based find functions
- **Required for**: Old DOS programs using FCB file operations (pre-DOS 2.0 style)

### Port I/O Operations

**File**: `src/runtime/dos16.rs:2766-2770`
```rust
// I/O port access (stubbed - can be extended)
fn port_in8(&self, _port: u16) -> u8 { 0 }
fn port_in16(&self, _port: u16) -> u16 { 0 }
fn port_out8(&self, _port: u16, _val: u8) {}
fn port_out16(&self, _port: u16, _val: u16) {}
```
- **Status**: Port reads return 0, writes are discarded
- **Impact**: DOS16 programs cannot access hardware directly
- **Required for**: Programs that need direct hardware access (games, utilities)

## Memory Management

### DOS16 Memory Coalescing

**File**: `src/runtime/dos16.rs:456`
```rust
// Mark as free
self.write_mcb(mcb_seg, mcb_type, MCB_OWNER_FREE, size);

// TODO: Coalesce adjacent free blocks
serial_write_bytes(b"Freed memory at segment ");
```
- **Status**: Memory is freed but adjacent blocks are not merged
- **Impact**: Memory fragmentation will increase over time
- **Required for**: Long-running DOS programs that allocate/free memory frequently

## Native64 Runtime Features

### Syscall Implementations

**File**: `src/runtime/native64.rs:725-761`

Multiple incomplete syscalls:

```rust
syscall::SYS_READ => {
    // arg1 = fd, arg2 = buf ptr, arg3 = max len
    // For now, just return 0 (EOF)
    task.cpu.rax = 0;
}

syscall::SYS_TIMER => {
    // Return timer ticks (placeholder)
    task.cpu.rax = 0;
}

syscall::SYS_SLEEP => {
    // arg1 = milliseconds - just return for now
    task.cpu.rax = 0;
}
```
- **Status**: Return placeholder values
- **Impact**: Native64 ELF programs cannot read input, get accurate time, or sleep
- **Required for**: Interactive native applications

## Disk/Filesystem

### GPT Partition Support

**File**: `src/disk/drives.rs:158-161`
```rust
PartitionTableType::Gpt => {
    // GPT partition table - TODO: parse GPT entries
    // For now, skip GPT disks
}
```
- **Status**: GPT disks are completely ignored
- **Impact**: Cannot mount modern GPT-partitioned disks, only MBR
- **Required for**: Modern disk support, UEFI systems

## Process Management Fallbacks

### Exit Handlers

**File**: `src/main.rs:1951-1952`
```rust
// Fallback: just halt
loop { unsafe { core::arch::asm!("hlt"); } }
```
- **Status**: When process exit fails, system halts
- **Impact**: System hangs instead of recovering
- **Required for**: Robust error handling

**File**: `src/process/mod.rs:256-259`
```rust
// Fallback: just halt
loop {
    core::arch::asm!("hlt");
}
```
- **Status**: Process cleanup fallback is to halt CPU
- **Impact**: Similar to above, system hangs on process errors
- **Required for**: Robust process management

## Summary Statistics

| Category | Count | Priority |
|----------|-------|----------|
| Graphics/VGA | 4 | Medium |
| File I/O | 9 | High |
| DOS Interrupts | 4 | Medium |
| Memory Management | 1 | Low |
| Native64 Runtime | 3 | Medium |
| Disk/Filesystem | 1 | High |
| Process Management | 2 | Low |
| **Total** | **24** | |

## Priority Definitions

- **High**: Critical for basic functionality (file I/O, disk support)
- **Medium**: Important for compatibility (DOS interrupts, graphics, native runtime)
- **Low**: Performance optimizations or edge cases (memory coalescing, error recovery)

## Implementation Recommendations

### Quick Wins (Easy to Implement)
1. VGA palette support (SYS_VGA_SET_PALETTE)
2. Native64 SYS_TIMER implementation
3. DOS16 memory block coalescing

### Medium Complexity
1. Port I/O operations for DOS16 runtime
2. INT 16h keyboard input hooking
3. File handle write-back on close

### Complex (Requires Significant Work)
1. GPT partition table parsing and mounting
2. GWBASIC file operations (needs filesystem integration)
3. GWBASIC program LOAD/SAVE/MERGE/CHAIN
4. Pixel reading from framebuffer (needs buffer format handling)
5. Native64 SYS_READ input handling

## Notes

- Many stubs exist to provide a stable API surface while development continues
- Some features (like FCB file operations) may be low priority due to being legacy DOS functionality
- File I/O stubs in GWBASIC are blockers for many BASIC programs
- Graphics features are needed for games and graphical applications
