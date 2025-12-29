# WATOS - A Bare-Metal Operating System

WATOS is a 64-bit operating system kernel written in Rust with UEFI bootloader support. It features a modular crate-based architecture with Ring 0/Ring 3 privilege separation and boots directly on x86-64 hardware.

## Features

- **UEFI Boot**: Native UEFI bootloader (BOOTX64.EFI)
- **64-bit Kernel**: Written entirely in Rust (no_std)
- **Modular Design**: Crate-based architecture for drivers, filesystems, and services
- **Hardware Drivers**: AHCI (SATA), E1000 (Intel NIC), PS/2 (keyboard/mouse)
- **Filesystems**: FAT32 and custom WFS (WATOS File System)
- **TCP/IP Stack**: Network protocol implementation
- **Process Management**: Ring 0/Ring 3 privilege separation
- **DOS Emulator**: 16-bit DOS application compatibility layer
- **Applications**: GWBASIC interpreter, system utilities

## Prerequisites

### Recommended Setup (All Distributions)

**Install Rust via rustup:**
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Add bare-metal targets
rustup target add x86_64-unknown-none
rustup target add x86_64-unknown-uefi

# Add LLVM tools for rust-objcopy
rustup component add llvm-tools-preview
```

**QEMU for testing (optional but recommended):**

RHEL/Fedora:
```bash
sudo dnf install -y qemu-kvm edk2-ovmf

# For graphical window in interactive mode (optional):
sudo dnf install -y virt-viewer
```

Ubuntu/Debian:
```bash
sudo apt install -y qemu-system-x86 ovmf
```

### Alternative: System Packages (RHEL/Fedora Only - Limited)

**Note:** This method only works for the kernel build. The UEFI bootloader requires rustup.

```bash
# Rust compiler and tools
sudo dnf install -y rust cargo

# Rust standard library for kernel target only
sudo dnf install -y rust-std-static-x86_64-unknown-none

# Object file tool (choose one)
sudo dnf install -y llvm  # or binutils

# QEMU for testing
sudo dnf install -y qemu-kvm edk2-ovmf

# Then build with: ./scripts/build.sh --no-uefi
```

## Quick Start

### Build Everything

```bash
./scripts/build.sh
```

This builds:
- `BOOTX64.EFI` - UEFI bootloader
- `kernel.bin` - Kernel binary
- `uefi_test/` - Bootable directory structure
- `output/watos.img` - WFS data disk image
- All native WATOS applications

### Run in QEMU

```bash
# Standard boot test (headless)
./scripts/boot_test.sh

# Interactive mode with graphical window
./scripts/boot_test.sh --interactive

# Note: On RHEL/Fedora, install virt-viewer for graphical window:
#   sudo dnf install virt-viewer
```

### Run Tests

```bash
./scripts/test.sh
```

## Build Options

```bash
# Debug build (default is release)
./scripts/build.sh --debug

# Clean build
./scripts/build.sh --clean

# Skip UEFI structure creation
./scripts/build.sh --no-uefi

# Verbose output
./scripts/build.sh --verbose
```

## Project Structure

```
.
├── crates/
│   ├── boot/               # UEFI bootloader
│   ├── core/               # Kernel foundation (arch, mem, syscall)
│   ├── drivers/            # Hardware drivers (PCI, AHCI, E1000, PS/2)
│   ├── storage/            # Filesystems (VFS, FAT, WFS)
│   ├── network/            # TCP/IP stack
│   ├── sys/                # System services (console, process, runtime)
│   ├── emu/                # DOS 16-bit emulator
│   └── apps/               # Native applications
├── scripts/                # Build and test scripts
├── docs/                   # Architecture documentation
├── tools/                  # Build tools (mkfs.wfs)
└── src/                    # Kernel entry point
```

## Development Workflow

### Branch Structure

- `main` - Protected production branch
- `uat` - User acceptance testing branch
- `dev` - Active development branch

### Workflow

1. Create feature branches from `dev`
2. Merge features to `dev` for integration
3. Promote stable `dev` to `uat` for testing
4. Promote tested `uat` to `main` for release

## Documentation

- [Architecture Overview](docs/ARCHITECTURE.md) - Detailed system architecture
- [Build Instructions](CLAUDE.md) - Claude Code specific instructions
- [Filesystem Design](docs/FILESYSTEM_QUICK_REFERENCE.md) - VFS and filesystem details
- [GW-BASIC Graphics](docs/GWBASIC_GRAPHICS.md) - Graphics programming guide

## Applications

WATOS includes several native applications in `/apps/system/`:

- `echo` - Print arguments to console
- `date` - Display system date/time
- `clear` - Clear screen
- `uname` - System information
- `uptime` - System uptime
- `ps` - Process list
- `drives` - Storage devices
- `ls` - List directory
- `pwd` - Print working directory
- `cd` - Change directory
- `mkdir` - Create directory

Plus:
- `GWBASIC.EXE` - GW-BASIC interpreter
- `SYSTEM/TERM.EXE` - Terminal emulator (auto-start)

## Troubleshooting

### "rustup: command not found" after installation

Add cargo bin to your PATH:
```bash
source $HOME/.cargo/env
# Or add this to your ~/.bashrc or ~/.zshrc:
export PATH="$HOME/.cargo/bin:$PATH"
```

### Build fails with "can't find crate for `core`"

Install the required Rust targets:
```bash
rustup target add x86_64-unknown-none x86_64-unknown-uefi
```

If using system Rust (RHEL/Fedora only):
```bash
sudo dnf install -y rust-std-static-x86_64-unknown-none
# Note: x86_64-unknown-uefi not available, use --no-uefi flag
```

### Build fails with "No objcopy tool found"

Install LLVM tools via rustup (recommended):
```bash
rustup component add llvm-tools-preview
```

Or install LLVM from your package manager:
```bash
# RHEL/Fedora
sudo dnf install -y llvm

# Ubuntu/Debian
sudo apt install -y llvm
```

### QEMU fails to boot

Install QEMU and OVMF UEFI firmware:

RHEL/Fedora:
```bash
sudo dnf install -y qemu-kvm edk2-ovmf
```

Ubuntu/Debian:
```bash
sudo apt install -y qemu-system-x86 ovmf
```

### Interactive mode doesn't open a window (RHEL/Fedora)

On RHEL/Fedora, `qemu-kvm` doesn't support graphical displays (GTK/SDL). Install a VNC viewer for graphical window support:

```bash
sudo dnf install -y virt-viewer
```

The boot script will automatically:
1. Start QEMU with VNC server
2. Launch the VNC viewer window
3. Close QEMU when you close the window

Without a VNC viewer, you can still:
- Use headless mode: `./scripts/boot_test.sh` (check logs in `ai-temp/logs/`)
- Connect manually: `vncviewer localhost:5900` (while QEMU is running)

## License

See LICENSE file for details.

## Contributing

1. Fork the repository
2. Create a feature branch from `dev`
3. Make your changes
4. Test with `./scripts/test.sh`
5. Submit a pull request to `dev`
