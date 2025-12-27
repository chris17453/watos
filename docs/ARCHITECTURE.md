# WATOS Kernel Architecture

## Overview

WATOS is a bare-metal 64-bit operating system written in Rust. It boots via UEFI, runs in x86-64 long mode, and provides a modular crate-based architecture with Ring 0/Ring 3 privilege separation.

```
UEFI Firmware → BOOTX64.EFI (bootloader) → kernel.bin → User Applications
```

## Crate Structure

```
crates/
├── boot/                   # UEFI bootloader
│
├── core/                   # Foundation - NO internal deps
│   ├── arch/               #   CPU: GDT, TSS, IDT, PIC, ports
│   ├── mem/                #   Heap, paging, physical allocator
│   └── syscall/            #   Syscall ABI definitions
│
├── drivers/                # Hardware drivers
│   ├── traits/             #   BlockDevice, NicDevice, etc.
│   ├── bus/                #   Bus drivers
│   │   └── pci/            #     PCI enumeration
│   ├── storage/            #   Storage hardware
│   │   └── ahci/           #     SATA → BlockDevice
│   ├── network/            #   Network hardware
│   │   └── e1000/          #     Intel NIC → NicDevice
│   ├── input/              #   Input hardware
│   │   └── ps2/            #     Keyboard → InputDevice
│   ├── video/              #   Video hardware
│   │   └── vga/            #     (future)
│   └── audio/              #   Audio hardware
│       └── ac97/           #     (future)
│
├── storage/                # Storage subsystem
│   ├── vfs/                #   THE API: open/read/write/close
│   ├── fat/                #   FAT format driver
│   └── wfs/                #   WFS format driver
│
├── network/                # Network subsystem
│   └── stack/              #   TCP/IP implementation
│
├── sys/                    # Kernel services
│   ├── console/            #   Virtual console management
│   ├── process/            #   Process management
│   └── runtime/            #   Binary format detection
│
├── emu/                    # Emulation
│   └── dos16/              #   DOS 16-bit emulator
│
└── apps/                   # User applications
    ├── gwbasic/
    └── echo/
```

## Abstraction Layers

```
┌──────────────────────────────────────────────────────────────┐
│  Applications (apps/)                                         │
├──────────────────────────────────────────────────────────────┤
│  Syscall Interface (core/syscall)                            │
├───────────────────────┬──────────────────────────────────────┤
│  VFS (storage/vfs)    │  Network Stack (network/stack)       │
├───────────────────────┼──────────────────────────────────────┤
│  FAT, WFS             │  TCP/IP                              │
│  (storage/fat, wfs)   │                                      │
├───────────────────────┴──────────────────────────────────────┤
│  Driver Traits (drivers/traits)                              │
│    BlockDevice, NicDevice, InputDevice, VideoDevice, etc.    │
├───────────────────────┬──────────────────────────────────────┤
│  AHCI, NVMe           │  e1000, RTL8139                      │
│  (drivers/storage/)   │  (drivers/network/)                  │
├───────────────────────┴──────────────────────────────────────┤
│  Core (core/arch, core/mem)                                  │
├──────────────────────────────────────────────────────────────┤
│  Hardware                                                     │
└──────────────────────────────────────────────────────────────┘
```

## Driver Traits

All hardware drivers implement traits defined in `drivers/traits/`:

| Trait | Implemented By | Used By |
|-------|---------------|---------|
| BlockDevice | ahci, nvme | vfs, fat, wfs |
| NicDevice | e1000, rtl8139 | network/stack |
| InputDevice | ps2, usb-hid | sys/console |
| VideoDevice | vga, gop | sys/console |
| AudioDevice | ac97, hda | (future) |

## Debug Features

Enable debug output at compile time:

```toml
# In Cargo.toml
watos-driver-traits = { path = "...", features = ["debug-storage"] }
```

Available features:
- `debug-all` - Enable all debug output
- `debug-storage` - BlockDevice operations
- `debug-network` - NicDevice operations
- `debug-input` - InputDevice operations
- `debug-video` - VideoDevice operations
- `debug-audio` - AudioDevice operations
- `debug-bus` - Bus enumeration

## Boot Process

### 1. UEFI Bootloader (`crates/boot/`)

1. Acquires GOP for framebuffer
2. Allocates memory at 0x100000 for kernel
3. Copies `kernel.bin` into memory
4. Creates BootInfo at 0x80000
5. Exits boot services
6. Jumps to kernel

### 2. Kernel Entry (`src/main.rs`)

```rust
#[no_mangle]
pub extern "C" fn _start() -> ! {
    // 1. Init heap
    ALLOCATOR.lock().init(HEAP_START, HEAP_SIZE);

    // 2. Init architecture
    watos_arch::init(kernel_stack);

    // 3. TODO: Init subsystems

    // 4. Halt
    loop { hlt(); }
}
```

## Core Architecture (`crates/core/arch/`)

### GDT Segments

| Selector | Ring | Type |
|----------|------|------|
| 0x08 | 0 | Kernel Code |
| 0x10 | 0 | Kernel Data |
| 0x18 | 3 | User Code |
| 0x20 | 3 | User Data |
| 0x28 | 0 | TSS |

### TSS

- **RSP0**: Kernel stack for Ring 3 → Ring 0
- **IST1**: Double fault stack

### IDT

| Vector | Source | Handler |
|--------|--------|---------|
| 0-31 | CPU exceptions | Halt |
| 32 | Timer | Tick counter |
| 33 | Keyboard | Buffer scancode |
| 0x80 | Syscall | Dispatch |

## Memory Map

```
0x000000 - 0x100000     Reserved (UEFI)
0x100000 - 0x180000     Kernel binary
0x200000 - 0x600000     Kernel heap (4MB)
0x080000 - 0x080100     BootInfo
```

## Syscall Interface

User code uses `int 0x80`:

| Register | Purpose |
|----------|---------|
| EAX | Syscall number |
| RDI | Arg 1 |
| RSI | Arg 2 |
| RDX | Arg 3 |
| RAX | Return |

## Build Commands

```bash
./scripts/build.sh              # Build all
./scripts/boot_test.sh          # Test boot
./scripts/boot_test.sh -i       # Interactive
```

## Dependency Flow

```
apps/ → syscall
    ↓
sys/ → storage/, network/
    ↓
storage/, network/ → drivers/traits
    ↓
drivers/*/ → drivers/traits
    ↓
core/
```
