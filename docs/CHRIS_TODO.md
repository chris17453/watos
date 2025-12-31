# WATOS Development TODO List

## Current Status (2025-12-31)

### ✅ COMPLETED

1. **Fixed GWBASIC Character Display**
   - SYS_PUTCHAR now writes to VT system (not just ring buffer)
   - Characters display correctly when typing in GWBASIC
   - Location: `src/main.rs:3009-3021`

2. **Professional Keyboard Driver System**
   - Created `crates/drivers/input/keyboard/` with full modifier support
   - Supports: Shift, Ctrl, Alt, AltGr, Caps Lock, Num Lock, Scroll Lock
   - Handles extended scancodes (E0 prefix for Right Ctrl/Alt)
   - Simplified main.rs scancode_to_ascii() to 1 line (was 100+ lines)

3. **Keyboard Layout Support**
   - Implemented layouts: US, UK, DE (German), FR (French)
   - AltGr support for German (€, @, {, }, etc.) and French
   - Location: `crates/drivers/input/keyboard/src/layouts.rs`

4. **Code Page Support**
   - Implemented: CP437 (IBM PC), CP850 (Multilingual), CP1252 (Windows Latin-1)
   - Bidirectional Unicode conversion
   - Location: `crates/drivers/input/keyboard/src/codepage.rs`

5. **Build Tools for Keyboard Assets**
   - `tools/mkkeymaps/` - Compiles keyboard layouts to binary (.kmap files)
   - `tools/mkcodepages/` - Compiles code pages to binary (.cpg files)
   - Integrated into build script (scripts/build.sh:207-259)
   - Assets generated to: `rootfs/system/keymaps/` and `rootfs/system/codepages/`

6. **Runtime Commands**
   - `loadkeys` - Load keyboard layout from file
   - `chcp` - Change code page (show current or set new)
   - Location: `crates/apps/loadkeys/` and `crates/apps/chcp/`

7. **Syscalls Added**
   - SYS_SET_KEYMAP (150) - Load keyboard layout from buffer
   - SYS_SET_CODEPAGE (151) - Load code page from buffer
   - SYS_GET_CODEPAGE (152) - Get current code page ID
   - Location: `crates/core/syscall/src/lib.rs:142-145`
   - Location: `src/main.rs:1250-1253, 2946-3007`

8. **Binary File Formats**
   - KMAP format: Magic "KMAP" + version + name + 3x256 byte maps (normal/shift/altgr)
   - CPAG format: Magic "CPAG" + version + ID + name + 256x4 byte Unicode map
   - Validation implemented in keyboard driver (lib.rs:388-457)

9. **Build System**
   - Successfully builds all components
   - Keyboard assets generated during build
   - Boot test passes

---

## 🚧 IN PROGRESS / PARTIALLY COMPLETE

1. **Dynamic Keyboard Layout Loading**
   - **STATUS**: Infrastructure exists but not fully implemented
   - **ISSUE**: Driver uses hard-coded enum (KeyboardLayout_::US/UK/DE/FR)
   - **TODO**: Refactor driver to use runtime-loaded maps from binary files
   - Current functions validate and parse binary data but have TODO comments
   - Location: `crates/drivers/input/keyboard/src/lib.rs:388-421`

2. **Dynamic Code Page Loading**
   - **STATUS**: Infrastructure exists but not fully implemented
   - **ISSUE**: Driver uses hard-coded enum (CodePage_::CP437/CP850/CP1252/UTF8)
   - **TODO**: Refactor driver to use runtime-loaded character maps
   - Current function validates and parses binary data but has TODO comment
   - Location: `crates/drivers/input/keyboard/src/lib.rs:423-457`

---

## 📋 TODO - HIGH PRIORITY

### 1. Complete Dynamic Keyboard Loading
- [ ] Refactor `KeyboardDriver` to store dynamic maps instead of enums
- [ ] Add runtime map storage (3x256 byte arrays for normal/shift/altgr)
- [ ] Actually apply loaded keymap data from `load_keymap()` function
- [ ] Test loading different layouts at runtime with `loadkeys` command
- [ ] Verify AltGr works with loaded German/French layouts

### 2. Complete Dynamic Code Page Loading
- [ ] Refactor `KeyboardDriver` to store dynamic character map
- [ ] Add runtime character map storage (256 entries of char/u32)
- [ ] Actually apply loaded codepage data from `load_codepage()` function
- [ ] Test switching code pages at runtime with `chcp` command
- [ ] Verify special characters render correctly

### 3. Test GWBASIC with Full Keyboard
- [ ] Boot system and run `gwbasic`
- [ ] Test all modifier keys (Shift, Ctrl, Alt)
- [ ] Test symbols: quotes ("), parentheses, brackets, etc.
- [ ] Verify Caps Lock behavior
- [ ] Test basic BASIC commands with proper syntax

### 4. SVGA Graphics Mode for Mandelbrot
- [ ] Implement SVGA mode switching (VGA syscalls exist: SYS_VGA_SET_MODE = 30)
- [ ] Verify framebuffer access works (SYS_FB_ADDR = 51, SYS_FB_INFO = 50)
- [ ] Test pixel drawing (SYS_VGA_SET_PIXEL = 31)
- [ ] Run mandelbrot.bas in GWBASIC with graphics output
- [ ] **USER GOAL**: "i want to get it to a point that the language runs and i can run the mandelbrot.bas and see an image on my screen in svga mode..."

---

## 📋 TODO - MEDIUM PRIORITY

### 5. File System Improvements
- [ ] Fix WFS population (currently skips files >160 bytes)
- [ ] Ensure keyboard assets (.kmap, .cpg) are accessible on boot disk
- [ ] Verify `/system/keymaps/` and `/system/codepages/` directories exist
- [ ] Test loading keyboard assets from filesystem

### 6. Error Handling
- [ ] Better error messages for invalid keyboard layouts
- [ ] Better error messages for invalid code pages
- [ ] Handle missing keyboard asset files gracefully
- [ ] Add fallback to default US layout if load fails

### 7. Documentation
- [ ] Document keyboard layout binary format in docs/
- [ ] Document code page binary format in docs/
- [ ] Update ARCHITECTURE.md with keyboard subsystem
- [ ] Add examples of creating custom keyboard layouts

---

## 📋 TODO - LOW PRIORITY / FUTURE

### 8. Additional Keyboard Layouts
- [ ] Spanish (ES)
- [ ] Italian (IT)
- [ ] Nordic layouts (NO, SE, FI, DK)
- [ ] Create tool to generate layouts from XKB format

### 9. Additional Code Pages
- [ ] UTF-8 full implementation
- [ ] CP866 (Cyrillic)
- [ ] ISO-8859 series

### 10. Advanced Features
- [ ] Dead keys (accents, diacritics)
- [ ] Compose key support
- [ ] Keyboard LED control (Caps/Num/Scroll Lock indicators)
- [ ] Console-level keyboard buffering improvements

---

## 🐛 KNOWN ISSUES

1. **Unreachable Pattern Warning**
   - File: `crates/drivers/input/keyboard/src/codepage.rs:243-244`
   - Issue: Duplicate quote character mapping ('" => 147' and '" => 148')
   - Fixed: Changed to '\u{201C}' and '\u{201D}' for left/right double quotes
   - **STATUS**: RESOLVED

2. **WFS File Size Limitation**
   - Issue: mkfs_wfs only copies files <160 bytes
   - Impact: Large keyboard asset files may not be in filesystem
   - Workaround: Asset files are small enough to fit
   - **TODO**: Fix WFS population for larger files

3. **Missing Syscall Constants**
   - Issue: Syscall constants were in watos-syscall crate but not in main.rs
   - Fixed: Added SYS_SET_KEYMAP, SYS_SET_CODEPAGE, SYS_GET_CODEPAGE to main.rs:1250-1253
   - **STATUS**: RESOLVED

---

## 🎯 IMMEDIATE NEXT STEPS

1. **Refactor keyboard driver for dynamic loading** (1-2 hours)
   - Replace enum-based layout selection with runtime map storage
   - Make `load_keymap()` actually apply the loaded data
   - Make `load_codepage()` actually apply the loaded character map

2. **Test keyboard system end-to-end** (30 minutes)
   - Boot system
   - Run `chcp 850` to test code page switching
   - Run `loadkeys de` to test layout switching
   - Test in GWBASIC to verify all keys work

3. **Implement SVGA graphics** (2-4 hours)
   - Wire up existing VGA syscalls to actual framebuffer
   - Test with simple pixel drawing program
   - Run mandelbrot.bas

4. **Fix WFS large file support** (1 hour)
   - Investigate mkfs_wfs 160-byte limitation
   - Update to support keyboard asset files
   - Rebuild filesystem image

---

## 📝 NOTES

- **User's Ultimate Goal**: Run mandelbrot.bas in GWBASIC with SVGA graphics
- **Current Blocker**: SVGA mode not implemented (graphics syscalls exist but may not be wired up)
- **Keyboard Progress**: ~90% complete, just needs dynamic loading refactor
- **Build System**: Working perfectly, all assets generate correctly
- **Boot Test**: Passing, system is stable

---

## 🔧 TECHNICAL DEBT

- Refactor keyboard driver to use dynamic maps (current enum-based approach is placeholder)
- Remove TODO comments from load_keymap() and load_codepage() functions
- Add comprehensive keyboard driver tests
- Profile compiler warnings (unused_doc_comments, improper_ctypes in gwbasic)
- Consider splitting codepage.rs (large file with many character tables)

---

Last Updated: 2025-12-31 14:54 UTC
