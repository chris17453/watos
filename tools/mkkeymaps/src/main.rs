//! WATOS Keyboard Map Compiler
//!
//! Generates binary keymap files from keyboard layout definitions

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

/// Binary keymap file format:
/// - Magic: "KMAP" (4 bytes)
/// - Version: 1 (1 byte)
/// - Layout name length (1 byte)
/// - Layout name (variable, max 32 bytes)
/// - Normal map: 256 bytes (scancode -> char/0)
/// - Shift map: 256 bytes (scancode+shift -> char/0)
/// - AltGr map: 256 bytes (scancode+altgr -> char/0)
const MAGIC: &[u8; 4] = b"KMAP";
const VERSION: u8 = 1;

struct KeyMap {
    name: String,
    normal: [u8; 256],
    shift: [u8; 256],
    altgr: [u8; 256],
}

impl KeyMap {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            normal: [0; 256],
            shift: [0; 256],
            altgr: [0; 256],
        }
    }

    fn set(&mut self, scancode: u8, normal: char, shift: char) {
        self.normal[scancode as usize] = normal as u8;
        self.shift[scancode as usize] = shift as u8;
    }

    fn set_altgr(&mut self, scancode: u8, altgr: char) {
        self.altgr[scancode as usize] = altgr as u8;
    }

    fn write_to_file(&self, path: &Path) -> std::io::Result<()> {
        let mut file = File::create(path)?;

        // Write header
        file.write_all(MAGIC)?;
        file.write_all(&[VERSION])?;

        let name_bytes = self.name.as_bytes();
        let name_len = name_bytes.len().min(32) as u8;
        file.write_all(&[name_len])?;
        file.write_all(&name_bytes[..name_len as usize])?;

        // Write maps
        file.write_all(&self.normal)?;
        file.write_all(&self.shift)?;
        file.write_all(&self.altgr)?;

        Ok(())
    }
}

fn build_us_layout() -> KeyMap {
    let mut map = KeyMap::new("US");

    // Letters
    map.set(0x1E, 'a', 'A'); map.set(0x30, 'b', 'B'); map.set(0x2E, 'c', 'C');
    map.set(0x20, 'd', 'D'); map.set(0x12, 'e', 'E'); map.set(0x21, 'f', 'F');
    map.set(0x22, 'g', 'G'); map.set(0x23, 'h', 'H'); map.set(0x17, 'i', 'I');
    map.set(0x24, 'j', 'J'); map.set(0x25, 'k', 'K'); map.set(0x26, 'l', 'L');
    map.set(0x32, 'm', 'M'); map.set(0x31, 'n', 'N'); map.set(0x18, 'o', 'O');
    map.set(0x19, 'p', 'P'); map.set(0x10, 'q', 'Q'); map.set(0x13, 'r', 'R');
    map.set(0x1F, 's', 'S'); map.set(0x14, 't', 'T'); map.set(0x16, 'u', 'U');
    map.set(0x2F, 'v', 'V'); map.set(0x11, 'w', 'W'); map.set(0x2D, 'x', 'X');
    map.set(0x15, 'y', 'Y'); map.set(0x2C, 'z', 'Z');

    // Numbers and symbols
    map.set(0x02, '1', '!'); map.set(0x03, '2', '@'); map.set(0x04, '3', '#');
    map.set(0x05, '4', '$'); map.set(0x06, '5', '%'); map.set(0x07, '6', '^');
    map.set(0x08, '7', '&'); map.set(0x09, '8', '*'); map.set(0x0A, '9', '(');
    map.set(0x0B, '0', ')');

    // Punctuation
    map.set(0x0C, '-', '_'); map.set(0x0D, '=', '+');
    map.set(0x1A, '[', '{'); map.set(0x1B, ']', '}');
    map.set(0x27, ';', ':'); map.set(0x28, '\'', '"');
    map.set(0x29, '`', '~'); map.set(0x2B, '\\', '|');
    map.set(0x33, ',', '<'); map.set(0x34, '.', '>');
    map.set(0x35, '/', '?');

    // Special keys
    map.set(0x39, ' ', ' ');   // Space
    map.set(0x1C, '\n', '\n'); // Enter
    map.set(0x0E, '\x08', '\x08'); // Backspace
    map.set(0x0F, '\t', '\t'); // Tab

    map
}

fn build_uk_layout() -> KeyMap {
    let mut map = build_us_layout();  // Start with US layout
    map.name = "UK".to_string();

    // UK-specific differences
    map.set(0x28, '\'', '@');  // Quote/At
    map.set(0x03, '2', '"');   // 2/"
    map.set(0x04, '3', '£');   // 3/£ (pound sign)
    map.set(0x29, '`', '¬');   // Backtick/Not sign
    map.set(0x56, '\\', '|');  // Extra key next to left shift

    map
}

fn build_de_layout() -> KeyMap {
    let mut map = KeyMap::new("DE");

    // QWERTZ layout (Y and Z swapped)
    map.set(0x1E, 'a', 'A'); map.set(0x30, 'b', 'B'); map.set(0x2E, 'c', 'C');
    map.set(0x20, 'd', 'D'); map.set(0x12, 'e', 'E'); map.set(0x21, 'f', 'F');
    map.set(0x22, 'g', 'G'); map.set(0x23, 'h', 'H'); map.set(0x17, 'i', 'I');
    map.set(0x24, 'j', 'J'); map.set(0x25, 'k', 'K'); map.set(0x26, 'l', 'L');
    map.set(0x32, 'm', 'M'); map.set(0x31, 'n', 'N'); map.set(0x18, 'o', 'O');
    map.set(0x19, 'p', 'P'); map.set(0x10, 'q', 'Q'); map.set(0x13, 'r', 'R');
    map.set(0x1F, 's', 'S'); map.set(0x14, 't', 'T'); map.set(0x16, 'u', 'U');
    map.set(0x2F, 'v', 'V'); map.set(0x11, 'w', 'W'); map.set(0x2D, 'x', 'X');
    map.set(0x15, 'z', 'Z'); // Y -> Z (QWERTZ)
    map.set(0x2C, 'y', 'Y'); // Z -> Y (QWERTZ)

    // German numbers and symbols
    map.set(0x02, '1', '!'); map.set(0x03, '2', '"'); map.set(0x04, '3', '§');
    map.set(0x05, '4', '$'); map.set(0x06, '5', '%'); map.set(0x07, '6', '&');
    map.set(0x08, '7', '/'); map.set(0x09, '8', '('); map.set(0x0A, '9', ')');
    map.set(0x0B, '0', '=');

    // German special characters
    map.set(0x0C, 'ß', '?');   // Sharp S
    map.set(0x1A, 'ü', 'Ü');   // U-umlaut
    map.set(0x1B, '+', '*');
    map.set(0x27, 'ö', 'Ö');   // O-umlaut
    map.set(0x28, 'ä', 'Ä');   // A-umlaut
    map.set(0x29, '^', '°');
    map.set(0x2B, '#', '\'');
    map.set(0x33, ',', ';');
    map.set(0x34, '.', ':');
    map.set(0x35, '-', '_');
    map.set(0x56, '<', '>');   // Extra key

    // AltGr combinations
    map.set_altgr(0x03, '²');  // AltGr+2
    map.set_altgr(0x04, '³');  // AltGr+3
    map.set_altgr(0x08, '{');  // AltGr+7
    map.set_altgr(0x09, '[');  // AltGr+8
    map.set_altgr(0x0A, ']');  // AltGr+9
    map.set_altgr(0x0B, '}');  // AltGr+0
    map.set_altgr(0x0C, '\\'); // AltGr+ß
    map.set_altgr(0x10, '@');  // AltGr+Q
    map.set_altgr(0x12, '€');  // AltGr+E (Euro)
    map.set_altgr(0x2B, '~');  // AltGr+#
    map.set_altgr(0x56, '|');  // AltGr+<

    // Special keys
    map.set(0x39, ' ', ' ');
    map.set(0x1C, '\n', '\n');
    map.set(0x0E, '\x08', '\x08');
    map.set(0x0F, '\t', '\t');

    map
}

fn build_fr_layout() -> KeyMap {
    let mut map = KeyMap::new("FR");

    // AZERTY layout
    map.set(0x10, 'a', 'A'); map.set(0x30, 'b', 'B'); map.set(0x2E, 'c', 'C');
    map.set(0x20, 'd', 'D'); map.set(0x12, 'e', 'E'); map.set(0x21, 'f', 'F');
    map.set(0x22, 'g', 'G'); map.set(0x23, 'h', 'H'); map.set(0x17, 'i', 'I');
    map.set(0x24, 'j', 'J'); map.set(0x25, 'k', 'K'); map.set(0x26, 'l', 'L');
    map.set(0x27, 'm', 'M'); map.set(0x31, 'n', 'N'); map.set(0x18, 'o', 'O');
    map.set(0x19, 'p', 'P'); map.set(0x1E, 'q', 'Q'); map.set(0x13, 'r', 'R');
    map.set(0x1F, 's', 'S'); map.set(0x14, 't', 'T'); map.set(0x16, 'u', 'U');
    map.set(0x2F, 'v', 'V'); map.set(0x2C, 'w', 'W'); map.set(0x2D, 'x', 'X');
    map.set(0x15, 'y', 'Y'); map.set(0x11, 'z', 'Z');

    // French number row (unshifted = symbols, shifted = numbers)
    map.set(0x02, '&', '1'); map.set(0x03, 'é', '2'); map.set(0x04, '"', '3');
    map.set(0x05, '\'', '4'); map.set(0x06, '(', '5'); map.set(0x07, '-', '6');
    map.set(0x08, 'è', '7'); map.set(0x09, '_', '8'); map.set(0x0A, 'ç', '9');
    map.set(0x0B, 'à', '0');

    // French punctuation
    map.set(0x0C, ')', '°');
    map.set(0x0D, '=', '+');
    map.set(0x1A, '^', '¨');
    map.set(0x1B, '$', '£');
    map.set(0x28, 'ù', '%');
    map.set(0x2B, '*', 'µ');
    map.set(0x29, '²', '³');
    map.set(0x32, ',', '?');
    map.set(0x33, ';', '.');
    map.set(0x34, ':', '/');
    map.set(0x35, '!', '§');
    map.set(0x56, '<', '>');

    // AltGr combinations for French
    map.set_altgr(0x03, '~');  // AltGr+é
    map.set_altgr(0x04, '#');  // AltGr+"
    map.set_altgr(0x05, '{');  // AltGr+'
    map.set_altgr(0x06, '[');  // AltGr+(
    map.set_altgr(0x07, '|');  // AltGr+-
    map.set_altgr(0x08, '`');  // AltGr+è
    map.set_altgr(0x09, '\\'); // AltGr+_
    map.set_altgr(0x0A, '^');  // AltGr+ç
    map.set_altgr(0x0B, '@');  // AltGr+à
    map.set_altgr(0x0C, ']');  // AltGr+)
    map.set_altgr(0x0D, '}');  // AltGr+=
    map.set_altgr(0x12, '€');  // AltGr+E

    // Special keys
    map.set(0x39, ' ', ' ');
    map.set(0x1C, '\n', '\n');
    map.set(0x0E, '\x08', '\x08');
    map.set(0x0F, '\t', '\t');

    map
}

fn main() {
    let output_dir = Path::new("rootfs/system/keymaps");

    // Create output directory
    fs::create_dir_all(output_dir).expect("Failed to create keymap directory");

    println!("Building keyboard layouts...");

    // Build and save layouts
    let layouts = vec![
        build_us_layout(),
        build_uk_layout(),
        build_de_layout(),
        build_fr_layout(),
    ];

    for layout in layouts {
        let filename = format!("{}.kmap", layout.name.to_lowercase());
        let path = output_dir.join(&filename);

        layout.write_to_file(&path).expect(&format!("Failed to write {}", filename));
        println!("  Created: {}", path.display());
    }

    println!("Keyboard layouts compiled successfully!");
}
