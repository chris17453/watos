# WATOS Core Utilities

Native 64-bit utilities written in Rust for the WATOS operating system.

## Overview

These utilities are compiled as ELF64 binaries that run directly on WATOS using the native64 runtime. They use WATOS syscalls (INT 0x80) for system interaction.

## Building

Prerequisites:
- Rust nightly toolchain
- `x86_64-unknown-none` target installed

To build all utilities:
```bash
cd tools/coreutils
make
```

This will create `.EXE` files (ELF64 format) in `rootfs/BIN/`.

To clean:
```bash
make clean
```

## Utilities

### ECHO.EXE
Display text to console.

**Usage:** `ECHO <text>`

**Status:** Basic implementation (args parsing TODO)

### CAT.EXE
Concatenate and display files.

**Usage:** `CAT <file1> [file2] ...`

**Status:** Stub (file I/O TODO)

### COPY.EXE
Copy files from source to destination.

**Usage:** `COPY <source> <dest>`

**Status:** Stub (file I/O TODO)

### DEL.EXE
Delete a file.

**Usage:** `DEL <filename>`

**Status:** Stub (file I/O TODO)

### REN.EXE
Rename a file.

**Usage:** `REN <oldname> <newname>`

**Status:** Stub (file I/O TODO)

## Implementation Details

### no_std Environment
All utilities are built with `#![no_std]` and `#![no_main]` to run directly on WATOS without requiring a standard library.

### Syscalls
Uses WATOS syscalls via INT 0x80:
- **SYS_EXIT (0)** - Exit program
- **SYS_WRITE (1)** - Write to stdout
- **SYS_READ (2)** - Read from stdin
- **SYS_OPEN (3)** - Open file (TODO)
- **SYS_CLOSE (4)** - Close file (TODO)

### Entry Point
Each utility has a `_start()` function that serves as the entry point:

```rust
#[no_mangle]
pub extern "C" fn _start() -> ! {
    // Utility code here
    exit(0)
}
```

### Linker Script
A custom linker script (`linker.ld`) places the code at 0x100000 (1MB), standard for ELF executables.

## Testing

After building, test the utilities in WATOS:

1. Build WATOS: `./scripts/build.sh`
2. Run WATOS: `./scripts/boot_test.sh --interactive`
3. In WATOS shell:
   ```
   run echo.exe
   run cat.exe
   ```

## Future Work

- [ ] Implement command-line argument parsing
- [ ] Add file I/O syscalls (SYS_OPEN, SYS_CLOSE, SYS_READ file, SYS_WRITE file)
- [ ] Implement full functionality for COPY, DEL, REN, CAT
- [ ] Add MORE utility for paginated display
- [ ] Add MKDIR, RMDIR, CD for directory operations
- [ ] Add LS utility (list files, replacement for DIR)
- [ ] Add FIND utility for searching
- [ ] Add system utilities (DATE, TIME, MEM, etc.)

## Architecture Notes

WATOS utilities run in the native64 runtime which:
- Interprets x86-64 instructions
- Provides syscall interface via INT 0x80
- Manages task memory and execution
- Handles process lifecycle

This approach provides:
- **Safety**: Memory isolation per task
- **Portability**: ELF64 standard format
- **Simplicity**: Direct syscall interface
- **Performance**: Native 64-bit execution

## Comparison to DOS COM/EXE Format

Unlike the original DOS approach:
- **Not** 16-bit x86 assembly
- **Not** using DOS INT 21h interrupts
- **Not** limited to 64KB segments
- Uses native 64-bit instructions and calling conventions
- Uses WATOS-specific syscalls
- Can leverage Rust's safety features
