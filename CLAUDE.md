# WATOS Project - Claude Code Instructions

## Project Overview

WATOS is a bare-metal 64-bit operating system kernel with UEFI bootloader, written in Rust. It features a modular crate-based architecture with Ring 0/Ring 3 privilege separation.

See `docs/ARCHITECTURE.md` for detailed technical documentation.

## Architecture

```
UEFI Firmware -> BOOTX64.EFI (crates/boot/) -> kernel.bin (src/main.rs)
```

### Crate Structure
```
crates/
├── boot/               # UEFI bootloader
├── core/               # Foundation (arch, mem, syscall)
├── drivers/            # Hardware drivers
│   ├── traits/         #   Device traits (BlockDevice, NicDevice, etc.)
│   ├── bus/pci/        #   PCI enumeration
│   ├── storage/ahci/   #   SATA controller
│   ├── network/e1000/  #   Intel NIC
│   ├── input/ps2/      #   Keyboard/mouse
│   ├── video/          #   (future)
│   └── audio/          #   (future)
├── storage/            # Storage subsystem (vfs, fat, wfs)
├── network/stack/      # TCP/IP stack
├── sys/                # Kernel services (console, process, runtime)
├── emu/dos16/          # DOS emulator
└── apps/               # Applications (gwbasic, echo)
```

### Key Principle
The kernel entry (`src/main.rs`) is minimal (~50 lines). All functionality lives in crates.

## Branch Workflow

**IMPORTANT**: The `main` branch is protected. All development work must follow this workflow:

### Branch Structure
- `main` - Protected production branch (stable releases only)
- `uat` - User acceptance testing branch (pre-release testing)
- `dev` - Active development branch (integration)

### Development Process
1. **Create feature branch from dev**:
   ```bash
   git checkout dev
   git pull origin dev
   git checkout -b feature/your-feature-name
   ```

2. **Make changes and test**:
   ```bash
   ./scripts/test.sh
   ./scripts/boot_test.sh
   ```

3. **Commit and push to dev**:
   ```bash
   git add .
   git commit -m "Description of changes"
   git checkout dev
   git merge feature/your-feature-name
   git push origin dev
   ```

4. **Promote to UAT for testing**:
   ```bash
   git checkout uat
   git merge dev
   git push origin uat
   # Run full test suite
   ```

5. **Promote to main after UAT approval**:
   ```bash
   git checkout main
   git merge uat
   git push origin main
   ```

### Claude Code Usage
When working with Claude Code:
- Always work on the `dev` branch or feature branches
- Create PRs to merge changes from `dev` -> `uat` -> `main`
- Never commit directly to `main` or `uat`

## Build System

### Quick Commands
```bash
./scripts/build.sh              # Build everything
./scripts/boot_test.sh          # Run in QEMU
./scripts/boot_test.sh -i       # Interactive QEMU
./scripts/test.sh               # Run all tests
```

### Debug Features
Enable debug output for specific subsystems:
```bash
# In driver Cargo.toml
[features]
debug = ["watos-driver-traits/debug-storage"]
```

Available: `debug-storage`, `debug-network`, `debug-input`, `debug-video`, `debug-audio`, `debug-bus`

### Build Artifacts
| File | Description |
|------|-------------|
| BOOTX64.EFI | UEFI bootloader |
| kernel.bin | Raw kernel binary |
| uefi_test/ | Bootable disk structure |
| output/watos.img | WFS data disk |

## File Locations

| Purpose | Location |
|---------|----------|
| Kernel entry | `src/main.rs` |
| Bootloader | `crates/boot/` |
| Core crates | `crates/core/` |
| Driver traits | `crates/drivers/traits/` |
| Hardware drivers | `crates/drivers/{bus,storage,network,input,video,audio}/` |
| Storage subsystem | `crates/storage/` |
| Network stack | `crates/network/stack/` |
| System services | `crates/sys/` |
| Architecture docs | `docs/ARCHITECTURE.md` |
| Test logs | `ai-temp/logs/` |

## Code Conventions

- `no_std` bare-metal Rust
- Edition 2021
- No floating-point or SIMD
- Custom allocator via `linked_list_allocator`
- Serial debug output via `watos_arch::serial_write()`
- All hardware drivers implement traits from `drivers/traits/`

## Testing Notes

- Boot tests use QEMU with OVMF firmware
- Serial output captured to `ai-temp/logs/`
- Tests look for "WATOS" in boot output
- Default timeout is 30 seconds
