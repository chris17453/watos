# WATOS Project - Claude Code Instructions

## Project Overview

WATOS (DOS64) is a bare-metal 64-bit operating system kernel with UEFI bootloader, written in Rust. It provides a DOS-compatible command shell interface while running in 64-bit long mode.

## Architecture

```
UEFI Firmware -> BOOTX64.EFI (bootloader) -> kernel.bin (DOS64 kernel)
```

### Key Components
- **src/bootloader/**: UEFI application that loads and executes the kernel
- **src/**: Main kernel source
  - `main.rs`: Kernel entry point, DOS shell implementation
  - `cpu/`: Virtual CPU emulation (registers, execution modes)
  - `memory/`: Heap allocator and memory management
  - `vga.rs`: VGA text mode driver
  - `keyboard.rs`: PS/2 keyboard handler
  - `interrupts.rs`: IDT and interrupt handling
  - `dos/`: DOS API (INT 21h) compatibility layer
  - `io/`: I/O port operations and network manager

## Build System

### Quick Commands
```bash
# Check dependencies
./scripts/check_deps.sh

# Build everything
./scripts/build.sh

# Run all tests
./scripts/test.sh

# Run quick test (build + boot only)
./scripts/test.sh --quick

# Run QEMU boot test
./scripts/boot_test.sh

# Interactive QEMU session
./scripts/boot_test.sh --interactive
```

### Build Requirements
- Rust nightly toolchain
- Components: rust-src, llvm-tools-preview
- Target: x86_64-unknown-uefi
- QEMU + OVMF (for testing)

### Build Artifacts
- `BOOTX64.EFI`: UEFI bootloader
- `kernel.bin`: Raw kernel binary
- `uefi_test/`: Bootable UEFI structure

## Development Workflow

1. Make changes to kernel or bootloader source
2. Run `./scripts/build.sh` to compile
3. Run `./scripts/test.sh --quick` to validate
4. Use `./scripts/boot_test.sh --interactive` for debugging

## Current Task Tracking

See `ai-temp/CURRENT_TASK.md` for active development tasks.

## Code Conventions

- This is a `no_std` bare-metal project
- Uses Rust nightly features: `naked_functions`, `asm_const`, etc.
- No floating-point or SIMD (disabled in cargo config)
- Custom allocator via `linked_list_allocator`
- VGA text output via `print!`/`println!` macros

## Testing Notes

- Boot tests use QEMU with OVMF firmware
- Serial output is captured to `ai-temp/logs/`
- Tests look for "DOS64" or shell prompt in output
- Default timeout is 30 seconds

## File Locations

| Purpose | Location |
|---------|----------|
| Kernel source | `src/` |
| Bootloader source | `src/bootloader/` |
| Build scripts | `scripts/` |
| Task tracking | `ai-temp/` |
| Test logs | `ai-temp/logs/` |
| Build output | `target/` |
