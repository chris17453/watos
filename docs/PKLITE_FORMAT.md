# PKLITE DOS Executable Compression Format

Reference documentation for the PKLITE executable compression format used by DOS programs.

## Overview

PKLITE is an executable compression utility from PKWARE (makers of PKZIP), released in 1990. It compresses DOS EXE files (to EXE) and COM files (to COM), achieving up to 60% size reduction through LZ77-based algorithms with Huffman coding and self-decompressing stubs.

**Key Points:**
- Files are identifiable by the "PKWARE" copyright string (though sometimes modified)
- Compressed files contain a decompressor stub that restores the original executable in memory at runtime
- Multiple versions exist with different compression schemes and features

## Sources

- [PKLite - ModdingWiki](https://moddingwiki.shikadi.net/wiki/PKLite)
- [PKLITE - Just Solve the File Format Problem](http://fileformats.archiveteam.org/wiki/PKLITE)
- [Notes on PKLITE format - Entropymine](https://entropymine.wordpress.com/2021/12/19/notes-on-pklite-format-part-1/)

---

## File Structure (DOS EXE)

PKLite-compressed executables follow standard DOS EXE format with these components:

1. **Standard MZ header** with PKLite extensions
2. **Decompression routine** in the code block
3. **Compressed executable data**
4. **Compressed relocation table** (different algorithm)
5. **Trailing footer data** (register initialization values)

---

## Header Format

The PKLite header extends the standard MZ header:

| Offset | Size | Description |
|--------|------|-------------|
| 0x00-0x01 | 2 | "MZ" signature |
| 0x02-0x03 | 2 | Bytes on last page |
| 0x04-0x05 | 2 | Pages in file (512 bytes each) |
| 0x06-0x07 | 2 | Number of relocations (usually 0x0000 or 0x0001) |
| 0x08-0x09 | 2 | Header size in paragraphs |
| 0x0A-0x0B | 2 | Min extra paragraphs |
| 0x0C-0x0D | 2 | Max extra paragraphs |
| 0x0E-0x0F | 2 | Initial SS (relative to load) |
| 0x10-0x11 | 2 | Initial SP |
| 0x12-0x13 | 2 | Checksum |
| 0x14-0x15 | 2 | Initial IP |
| 0x16-0x17 | 2 | Initial CS (relative to load) |
| 0x18-0x19 | 2 | Relocation table offset |
| 0x1A-0x1B | 2 | Overlay number |
| **0x1C-0x1D** | 2 | **Version descriptor** (PKLITE-specific) |
| 0x1E+ | varies | Copyright message |

### Version Descriptor (Offset 0x1C-0x1D)

The 16-bit little-endian version descriptor characterizes the PKLITE version and compression type:

- **Low 12 bits**: Version number (e.g., 0x10F = version 1.15)
- **Bit 0x1000 (12)**: "Extra" compression (Professional version only)
- **Bit 0x2000 (13)**: "Large" compression mode
- **Bit 0x4000 (14)**: "Load-high" feature (v1.00β only)
- **Bit 0x8000 (15)**: Unused

### Compressed Data Offset

At offset `pgHeader × 16 + 0x4E` is a byte indicating the compressed data offset in paragraphs.

Formula: `compressed_data_offset = (pgHeader + byte_at_0x4E - 0x10) × 16`

---

## Compression Algorithm

PKLite uses **LZSS/LZ77 with Huffman coding**:

- "A version of the LZSS algorithm tweaked to operate more at the byte level"
- Pre-defined Huffman codebooks for offset and length values
- Two base compression schemes: "small" and "large" mode
- Large mode is used for larger files

### Compression Modes

| Mode | Description |
|------|-------------|
| Small | Default mode for smaller files |
| Large | For larger files (0x2000 flag set) |
| Extra | Encrypted literals obfuscation (Professional version, 0x1000 flag) |

### Encrypted Variants

- **Encrypted literals**: Simple obfuscation, used with "extra" compression
- **Encrypted offsets**: Used only in late-era v1.20 files
- **Encrypted decompressor**: v1.14+ with -e option; XOR-based or modular addition

---

## Relocation Table Decompression

Two modes for handling relocations:

### Long Mode (extra flag + version ≥ 1.12)

- Reads UINT16LE count values
- Combines msb (upper 16 bits) with lsb (lower 16 bits) into 32-bit addresses
- Increments msb by 0x0FFF per iteration
- Terminates on 0xFFFF count
- Normalizes addresses (original values not preserved, but functionally equivalent)

### Short Mode (default)

- Reads UINT8 count followed by UINT16LE msb
- Constructs 32-bit addresses from msb/lsb pairs
- Preserves original relocation values

---

## Footer Structure

After the compressed relocation table:

| Data Type | Field | Purpose |
|-----------|-------|---------|
| UINT16LE | final_segSS | Stack segment |
| UINT16LE | final_regSP | Stack pointer |
| UINT16LE | final_segCS | Code segment |
| UINT16LE | final_regIP | Instruction pointer |

These values initialize x86 registers after decompression completes.

---

## Decompressor Stub Identification

### DOS EXE Detection Patterns

Byte patterns at the start of the code image segment (offset `16 × header_paragraphs`):

| Pattern | Versions |
|---------|----------|
| `b8 ?? ?? ba ?? ?? 8c db 03 d8 3b 1e 02 00 73` | 1.00-1.05, "1.10" |
| `b8 ?? ?? ba ?? ?? 05 00 00 3b 06 02 00 73` | 1.12-1.13, some "1.20" |
| `b8 ?? ?? ba ?? ?? 05 00 00 3b 06 02 00 72` | 1.14-1.15, some "1.20" |
| `50 b8 ?? ?? ba ?? ?? 05 00 00 3b 06 02 00 72` | 1.50-2.01 |

### DOS COM Detection Patterns

| Pattern | Offset | Versions |
|---------|--------|----------|
| `ba ?? ?? a1 02 00 2d 20 06...` | 36 | 1.00β |
| `b8 ?? ?? ba ?? ?? 3b c4 73 67...` | 44 | 1.00-1.14 |
| `b8 ?? ?? ba ?? ?? 3b c4 73 69...` | 46 | 1.15 |
| `50 b8 ?? ?? ba ?? ?? 3b c4 73 79...` | 46 | 1.50-2.01 |

---

## Decompression Process

1. Stub calculates destination segment for decompressed code
2. Stub copies decompressor code to calculated segment
3. Control transfers to copied decompressor
4. Decompressor reads compressed data from original location
5. Decompresses using LZ77/Huffman algorithm
6. Writes decompressed code to final location
7. Processes relocation table
8. Sets up registers from footer values
9. Transfers control to original entry point

### Decompression Termination

- The 0xFF special code signals end of compressed data
- Followed by compressed relocation table processing

---

## PSP Signature Protection

Files with encrypted decompressor may include "PSP signature" protection:

- Decompressor writes two-byte signature to offset 0x5C of Program Segment Prefix
- Usually "PK" (or lowercase "pk" in PKWARE programs)
- Payload program can check for signature to detect decompression
- Some decompressors (DISLITE, UNP) can patch files to reproduce signature

---

## Known Issues

- **v1.00-1.03**: May fail if loaded at very low memory address (MS-DOS 5.0+)
  - Fix: Use LOADFIX utility or patch with LOWFIX
- **v1.05**: Additional bug patchable by LOWFIX
- Files with "custom-data-2" segment will lose that data (not preserved)

---

## Version History

| Version | Date | Notes |
|---------|------|-------|
| 1.00β | 1990-05 | Private beta |
| 1.00 | 1990-12-01 | First public release |
| 1.03 | 1990-12-20 | |
| 1.05 | 1991-03-20 | |
| 1.12 | 1991-06-15 | |
| 1.13 | 1991-08-01 | |
| 1.14 | 1992-06-01 | Added encrypted decompressor |
| 1.15 | 1992-07-30 | |
| 1.50 | 1995-04-10 | |
| 2.01 | 1996-03-15 | Added Windows 3.x support |

---

## Decompression Tools

- **PKLITE -x**: Official decompression (limited)
- **UNP**: DOS multi-format decompressor
- **DISLITE**: DOS decompressor with PSP patch support
- **depklite**: Modern C implementation (GitHub)
- **[mz-explode](https://github.com/virginwidow/mz-explode)**: C++ reference implementation with full source
- **Deark**: With -m pklite option

### mz-explode Reference Implementation

The [mz-explode repository](https://github.com/virginwidow/mz-explode) contains a complete C++ implementation for decompressing PKLITE files. Key files:

- `src/explode/unpklite.cc` - Main decompression logic

**Core Algorithm:**
```
while (bx < decompressed_size):
  - Read 1 bit (length_code)
  - if 0: read literal byte, append to output
  - if 1: read 2 more bits, adjust length via Huffman table,
          get base offset, copy previous data to current position
```

**Key Functions:**
- `accept()` - Validates PKLITE signature ("PK" at 0x1E)
- `unpack()` - Main decompression routine
- `adjust_length_code_*()` - Huffman-like decoding tables for length values
- `get_base_offset()` - Decodes distance offsets for back-references
- `build_rellocs()` - Reconstructs relocation tables

---

## Implications for WATOS Emulator

For the WATOS DOS16 emulator to run PKLITE-compressed executables, it would need to either:

1. **Implement full PKLITE decompression algorithm** - Complex, requires LZ77+Huffman decoder
2. **Pre-decompress files before loading** - Use external tool (depklite, UNP)
3. **Emulate the decompressor stub** - Current approach, but requires proper segment/memory setup

The current issue with SETUP.EXE is that the decompressor stub jumps to memory address that should contain continuation code, but our emulator's simplified memory layout doesn't match what PKLITE expects.

### Specific Issue with SETUP.EXE

```
JMP E9 75 FF at 2619:0007
Target: 2619:FF7F = linear 0x3610F
```

The JMP displacement -139 (0xFF75) jumps backwards to an address that:
- Is beyond the copied decompressor code (only 582 bytes at 2619:0000)
- Contains zeroed memory in our emulator
- In real DOS, would contain additional PKLITE stub code due to segment overlap
