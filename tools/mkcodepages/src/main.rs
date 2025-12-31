//! WATOS Code Page Compiler
//!
//! Generates binary code page files from character encoding definitions

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

/// Binary code page file format:
/// - Magic: "CPAG" (4 bytes)
/// - Version: 1 (1 byte)
/// - Code page ID: u16 (2 bytes, little-endian)
/// - Name length (1 byte)
/// - Name (variable, max 32 bytes)
/// - Byte-to-Unicode map: 256 * 4 bytes (UTF-32 LE) = 1024 bytes
const MAGIC: &[u8; 4] = b"CPAG";
const VERSION: u8 = 1;

struct CodePage {
    id: u16,
    name: String,
    byte_to_unicode: [char; 256],
}

impl CodePage {
    fn new(id: u16, name: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
            byte_to_unicode: ['\0'; 256],
        }
    }

    fn set(&mut self, byte: u8, ch: char) {
        self.byte_to_unicode[byte as usize] = ch;
    }

    fn write_to_file(&self, path: &Path) -> std::io::Result<()> {
        let mut file = File::create(path)?;

        // Write header
        file.write_all(MAGIC)?;
        file.write_all(&[VERSION])?;
        file.write_all(&self.id.to_le_bytes())?;

        let name_bytes = self.name.as_bytes();
        let name_len = name_bytes.len().min(32) as u8;
        file.write_all(&[name_len])?;
        file.write_all(&name_bytes[..name_len as usize])?;

        // Write byte-to-unicode map (256 * 4 bytes UTF-32 LE)
        for ch in &self.byte_to_unicode {
            file.write_all(&(*ch as u32).to_le_bytes())?;
        }

        Ok(())
    }
}

fn build_cp437() -> CodePage {
    let mut cp = CodePage::new(437, "CP437");

    // ASCII (0-127) maps directly
    for i in 0..128 {
        cp.set(i, i as char);
    }

    // Extended ASCII (128-255) - IBM PC original
    let extended: &[(u8, char)] = &[
        (128, 'Ç'), (129, 'ü'), (130, 'é'), (131, 'â'),
        (132, 'ä'), (133, 'à'), (134, 'å'), (135, 'ç'),
        (136, 'ê'), (137, 'ë'), (138, 'è'), (139, 'ï'),
        (140, 'î'), (141, 'ì'), (142, 'Ä'), (143, 'Å'),
        (144, 'É'), (145, 'æ'), (146, 'Æ'), (147, 'ô'),
        (148, 'ö'), (149, 'ò'), (150, 'û'), (151, 'ù'),
        (152, 'ÿ'), (153, 'Ö'), (154, 'Ü'), (155, '¢'),
        (156, '£'), (157, '¥'), (158, '₧'), (159, 'ƒ'),
        (160, 'á'), (161, 'í'), (162, 'ó'), (163, 'ú'),
        (164, 'ñ'), (165, 'Ñ'), (166, 'ª'), (167, 'º'),
        (168, '¿'), (169, '⌐'), (170, '¬'), (171, '½'),
        (172, '¼'), (173, '¡'), (174, '«'), (175, '»'),
        (176, '░'), (177, '▒'), (178, '▓'), (179, '│'),
        (180, '┤'), (181, '╡'), (182, '╢'), (183, '╖'),
        (184, '╕'), (185, '╣'), (186, '║'), (187, '╗'),
        (188, '╝'), (189, '╜'), (190, '╛'), (191, '┐'),
        (192, '└'), (193, '┴'), (194, '┬'), (195, '├'),
        (196, '─'), (197, '┼'), (198, '╞'), (199, '╟'),
        (200, '╚'), (201, '╔'), (202, '╩'), (203, '╦'),
        (204, '╠'), (205, '═'), (206, '╬'), (207, '╧'),
        (208, '╨'), (209, '╤'), (210, '╥'), (211, '╙'),
        (212, '╘'), (213, '╒'), (214, '╓'), (215, '╫'),
        (216, '╪'), (217, '┘'), (218, '┌'), (219, '█'),
        (220, '▄'), (221, '▌'), (222, '▐'), (223, '▀'),
        (224, 'α'), (225, 'ß'), (226, 'Γ'), (227, 'π'),
        (228, 'Σ'), (229, 'σ'), (230, 'µ'), (231, 'τ'),
        (232, 'Φ'), (233, 'Θ'), (234, 'Ω'), (235, 'δ'),
        (236, '∞'), (237, 'φ'), (238, 'ε'), (239, '∩'),
        (240, '≡'), (241, '±'), (242, '≥'), (243, '≤'),
        (244, '⌠'), (245, '⌡'), (246, '÷'), (247, '≈'),
        (248, '°'), (249, '∙'), (250, '·'), (251, '√'),
        (252, 'ⁿ'), (253, '²'), (254, '■'), (255, ' '),
    ];

    for &(byte, ch) in extended {
        cp.set(byte, ch);
    }

    cp
}

fn build_cp850() -> CodePage {
    let mut cp = CodePage::new(850, "CP850");

    // ASCII (0-127) maps directly
    for i in 0..128 {
        cp.set(i, i as char);
    }

    // Extended ASCII (128-255) - Multilingual Latin 1
    let extended: &[(u8, char)] = &[
        (128, 'Ç'), (129, 'ü'), (130, 'é'), (131, 'â'),
        (132, 'ä'), (133, 'à'), (134, 'å'), (135, 'ç'),
        (136, 'ê'), (137, 'ë'), (138, 'è'), (139, 'ï'),
        (140, 'î'), (141, 'ì'), (142, 'Ä'), (143, 'Å'),
        (144, 'É'), (145, 'æ'), (146, 'Æ'), (147, 'ô'),
        (148, 'ö'), (149, 'ò'), (150, 'û'), (151, 'ù'),
        (152, 'ÿ'), (153, 'Ö'), (154, 'Ü'), (155, 'ø'),
        (156, '£'), (157, 'Ø'), (158, '×'), (159, 'ƒ'),
        (160, 'á'), (161, 'í'), (162, 'ó'), (163, 'ú'),
        (164, 'ñ'), (165, 'Ñ'), (166, 'ª'), (167, 'º'),
        (168, '¿'), (169, '®'), (170, '¬'), (171, '½'),
        (172, '¼'), (173, '¡'), (174, '«'), (175, '»'),
        (176, '░'), (177, '▒'), (178, '▓'), (179, '│'),
        (180, '┤'), (181, 'Á'), (182, 'Â'), (183, 'À'),
        (184, '©'), (185, '╣'), (186, '║'), (187, '╗'),
        (188, '╝'), (189, '¢'), (190, '¥'), (191, '┐'),
        (192, '└'), (193, '┴'), (194, '┬'), (195, '├'),
        (196, '─'), (197, '┼'), (198, 'ã'), (199, 'Ã'),
        (200, '╚'), (201, '╔'), (202, '╩'), (203, '╦'),
        (204, '╠'), (205, '═'), (206, '╬'), (207, '¤'),
        (208, 'ð'), (209, 'Ð'), (210, 'Ê'), (211, 'Ë'),
        (212, 'È'), (213, 'ı'), (214, 'Í'), (215, 'Î'),
        (216, 'Ï'), (217, '┘'), (218, '┌'), (219, '█'),
        (220, '▄'), (221, '¦'), (222, 'Ì'), (223, '▀'),
        (224, 'Ó'), (225, 'ß'), (226, 'Ô'), (227, 'Ò'),
        (228, 'õ'), (229, 'Õ'), (230, 'µ'), (231, 'þ'),
        (232, 'Þ'), (233, 'Ú'), (234, 'Û'), (235, 'Ù'),
        (236, 'ý'), (237, 'Ý'), (238, '¯'), (239, '´'),
        (240, '­'), (241, '±'), (242, '‗'), (243, '¾'),
        (244, '¶'), (245, '§'), (246, '÷'), (247, '¸'),
        (248, '°'), (249, '¨'), (250, '·'), (251, '¹'),
        (252, '³'), (253, '²'), (254, '■'), (255, ' '),
    ];

    for &(byte, ch) in extended {
        cp.set(byte, ch);
    }

    cp
}

fn build_cp1252() -> CodePage {
    let mut cp = CodePage::new(1252, "CP1252");

    // ASCII and Latin-1 (0-127, 160-255) map directly
    for i in 0..128 {
        cp.set(i, i as char);
    }
    for i in 160..=255 {
        cp.set(i, i as char);
    }

    // Windows-1252 specific (128-159)
    let windows_chars: &[(u8, char)] = &[
        (128, '€'), (130, '‚'), (131, 'ƒ'), (132, '„'),
        (133, '…'), (134, '†'), (135, '‡'), (136, 'ˆ'),
        (137, '‰'), (138, 'Š'), (139, '‹'), (140, 'Œ'),
        (142, 'Ž'), (145, '\u{2018}'), (146, '\u{2019}'), (147, '"'),
        (148, '"'), (149, '•'), (150, '–'), (151, '—'),
        (152, '˜'), (153, '™'), (154, 'š'), (155, '›'),
        (156, 'œ'), (158, 'ž'), (159, 'Ÿ'),
    ];

    for &(byte, ch) in windows_chars {
        cp.set(byte, ch);
    }

    // Undefined characters in Windows-1252 (129, 141, 143, 144, 157)
    cp.set(129, '?');
    cp.set(141, '?');
    cp.set(143, '?');
    cp.set(144, '?');
    cp.set(157, '?');

    cp
}

fn main() {
    let output_dir = Path::new("rootfs/system/codepages");

    // Create output directory
    fs::create_dir_all(output_dir).expect("Failed to create codepage directory");

    println!("Building code pages...");

    // Build and save code pages
    let codepages = vec![
        build_cp437(),
        build_cp850(),
        build_cp1252(),
    ];

    for cp in codepages {
        let filename = format!("cp{}.cpg", cp.id);
        let path = output_dir.join(&filename);

        cp.write_to_file(&path).expect(&format!("Failed to write {}", filename));
        println!("  Created: {}", path.display());
    }

    println!("Code pages compiled successfully!");
}
