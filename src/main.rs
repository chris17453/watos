#![no_std]
#![no_main]

extern crate alloc;
use core::panic::PanicInfo;
use linked_list_allocator::LockedHeap;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

mod interrupts;
mod net;
mod disk;
mod runtime;

// Boot info structure from bootloader at 0x80000
#[repr(C)]
struct BootInfo {
    magic: u32,
    framebuffer_addr: u64,
    framebuffer_width: u32,
    framebuffer_height: u32,
    framebuffer_pitch: u32,
    framebuffer_bpp: u32,
    pixel_format: u32, // 0=RGB, 1=BGR
}

const BOOT_INFO_ADDR: usize = 0x80000;
const BOOT_MAGIC: u32 = 0xD0564;

// Framebuffer state
static mut FB_ADDR: u64 = 0;
static mut FB_WIDTH: u32 = 0;
static mut FB_HEIGHT: u32 = 0;
static mut FB_PITCH: u32 = 0;
static mut FB_BGR: bool = false;

// Text cursor
const CHAR_WIDTH: u32 = 8;
const CHAR_HEIGHT: u32 = 16;
static mut CURSOR_X: u32 = 0;
static mut CURSOR_Y: u32 = 0;
static mut TEXT_COLS: u32 = 80;
static mut TEXT_ROWS: u32 = 25;

// 8x16 VGA font - standard PC BIOS font for ASCII 32-126
// Each character is 16 bytes (16 rows of 8 pixels)
static FONT_8X16: [u8; 1520] = [
    // Space (32)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // ! (33)
    0x00, 0x00, 0x18, 0x3C, 0x3C, 0x3C, 0x18, 0x18, 0x18, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00,
    // " (34)
    0x00, 0x66, 0x66, 0x66, 0x24, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // # (35)
    0x00, 0x00, 0x00, 0x6C, 0x6C, 0xFE, 0x6C, 0x6C, 0x6C, 0xFE, 0x6C, 0x6C, 0x00, 0x00, 0x00, 0x00,
    // $ (36)
    0x18, 0x18, 0x7C, 0xC6, 0xC2, 0xC0, 0x7C, 0x06, 0x06, 0x86, 0xC6, 0x7C, 0x18, 0x18, 0x00, 0x00,
    // % (37)
    0x00, 0x00, 0x00, 0x00, 0xC2, 0xC6, 0x0C, 0x18, 0x30, 0x60, 0xC6, 0x86, 0x00, 0x00, 0x00, 0x00,
    // & (38)
    0x00, 0x00, 0x38, 0x6C, 0x6C, 0x38, 0x76, 0xDC, 0xCC, 0xCC, 0xCC, 0x76, 0x00, 0x00, 0x00, 0x00,
    // ' (39)
    0x00, 0x30, 0x30, 0x30, 0x60, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // ( (40)
    0x00, 0x00, 0x0C, 0x18, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x18, 0x0C, 0x00, 0x00, 0x00, 0x00,
    // ) (41)
    0x00, 0x00, 0x30, 0x18, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x18, 0x30, 0x00, 0x00, 0x00, 0x00,
    // * (42)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x66, 0x3C, 0xFF, 0x3C, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // + (43)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x7E, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // , (44)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x18, 0x30, 0x00, 0x00, 0x00,
    // - (45)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFE, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // . (46)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00,
    // / (47)
    0x00, 0x00, 0x00, 0x00, 0x02, 0x06, 0x0C, 0x18, 0x30, 0x60, 0xC0, 0x80, 0x00, 0x00, 0x00, 0x00,
    // 0 (48)
    0x00, 0x00, 0x38, 0x6C, 0xC6, 0xC6, 0xD6, 0xD6, 0xC6, 0xC6, 0x6C, 0x38, 0x00, 0x00, 0x00, 0x00,
    // 1 (49)
    0x00, 0x00, 0x18, 0x38, 0x78, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x7E, 0x00, 0x00, 0x00, 0x00,
    // 2 (50)
    0x00, 0x00, 0x7C, 0xC6, 0x06, 0x0C, 0x18, 0x30, 0x60, 0xC0, 0xC6, 0xFE, 0x00, 0x00, 0x00, 0x00,
    // 3 (51)
    0x00, 0x00, 0x7C, 0xC6, 0x06, 0x06, 0x3C, 0x06, 0x06, 0x06, 0xC6, 0x7C, 0x00, 0x00, 0x00, 0x00,
    // 4 (52)
    0x00, 0x00, 0x0C, 0x1C, 0x3C, 0x6C, 0xCC, 0xFE, 0x0C, 0x0C, 0x0C, 0x1E, 0x00, 0x00, 0x00, 0x00,
    // 5 (53)
    0x00, 0x00, 0xFE, 0xC0, 0xC0, 0xC0, 0xFC, 0x06, 0x06, 0x06, 0xC6, 0x7C, 0x00, 0x00, 0x00, 0x00,
    // 6 (54)
    0x00, 0x00, 0x38, 0x60, 0xC0, 0xC0, 0xFC, 0xC6, 0xC6, 0xC6, 0xC6, 0x7C, 0x00, 0x00, 0x00, 0x00,
    // 7 (55)
    0x00, 0x00, 0xFE, 0xC6, 0x06, 0x06, 0x0C, 0x18, 0x30, 0x30, 0x30, 0x30, 0x00, 0x00, 0x00, 0x00,
    // 8 (56)
    0x00, 0x00, 0x7C, 0xC6, 0xC6, 0xC6, 0x7C, 0xC6, 0xC6, 0xC6, 0xC6, 0x7C, 0x00, 0x00, 0x00, 0x00,
    // 9 (57)
    0x00, 0x00, 0x7C, 0xC6, 0xC6, 0xC6, 0x7E, 0x06, 0x06, 0x06, 0x0C, 0x78, 0x00, 0x00, 0x00, 0x00,
    // : (58)
    0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00,
    // ; (59)
    0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x18, 0x18, 0x30, 0x00, 0x00, 0x00, 0x00,
    // < (60)
    0x00, 0x00, 0x00, 0x06, 0x0C, 0x18, 0x30, 0x60, 0x30, 0x18, 0x0C, 0x06, 0x00, 0x00, 0x00, 0x00,
    // = (61)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x7E, 0x00, 0x00, 0x7E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // > (62)
    0x00, 0x00, 0x00, 0x60, 0x30, 0x18, 0x0C, 0x06, 0x0C, 0x18, 0x30, 0x60, 0x00, 0x00, 0x00, 0x00,
    // ? (63)
    0x00, 0x00, 0x7C, 0xC6, 0xC6, 0x0C, 0x18, 0x18, 0x18, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00,
    // @ (64)
    0x00, 0x00, 0x00, 0x7C, 0xC6, 0xC6, 0xDE, 0xDE, 0xDE, 0xDC, 0xC0, 0x7C, 0x00, 0x00, 0x00, 0x00,
    // A (65)
    0x00, 0x00, 0x10, 0x38, 0x6C, 0xC6, 0xC6, 0xFE, 0xC6, 0xC6, 0xC6, 0xC6, 0x00, 0x00, 0x00, 0x00,
    // B (66)
    0x00, 0x00, 0xFC, 0x66, 0x66, 0x66, 0x7C, 0x66, 0x66, 0x66, 0x66, 0xFC, 0x00, 0x00, 0x00, 0x00,
    // C (67)
    0x00, 0x00, 0x3C, 0x66, 0xC2, 0xC0, 0xC0, 0xC0, 0xC0, 0xC2, 0x66, 0x3C, 0x00, 0x00, 0x00, 0x00,
    // D (68)
    0x00, 0x00, 0xF8, 0x6C, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x6C, 0xF8, 0x00, 0x00, 0x00, 0x00,
    // E (69)
    0x00, 0x00, 0xFE, 0x66, 0x62, 0x68, 0x78, 0x68, 0x60, 0x62, 0x66, 0xFE, 0x00, 0x00, 0x00, 0x00,
    // F (70)
    0x00, 0x00, 0xFE, 0x66, 0x62, 0x68, 0x78, 0x68, 0x60, 0x60, 0x60, 0xF0, 0x00, 0x00, 0x00, 0x00,
    // G (71)
    0x00, 0x00, 0x3C, 0x66, 0xC2, 0xC0, 0xC0, 0xDE, 0xC6, 0xC6, 0x66, 0x3A, 0x00, 0x00, 0x00, 0x00,
    // H (72)
    0x00, 0x00, 0xC6, 0xC6, 0xC6, 0xC6, 0xFE, 0xC6, 0xC6, 0xC6, 0xC6, 0xC6, 0x00, 0x00, 0x00, 0x00,
    // I (73)
    0x00, 0x00, 0x3C, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3C, 0x00, 0x00, 0x00, 0x00,
    // J (74)
    0x00, 0x00, 0x1E, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0xCC, 0xCC, 0xCC, 0x78, 0x00, 0x00, 0x00, 0x00,
    // K (75)
    0x00, 0x00, 0xE6, 0x66, 0x66, 0x6C, 0x78, 0x78, 0x6C, 0x66, 0x66, 0xE6, 0x00, 0x00, 0x00, 0x00,
    // L (76)
    0x00, 0x00, 0xF0, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x62, 0x66, 0xFE, 0x00, 0x00, 0x00, 0x00,
    // M (77)
    0x00, 0x00, 0xC6, 0xEE, 0xFE, 0xFE, 0xD6, 0xC6, 0xC6, 0xC6, 0xC6, 0xC6, 0x00, 0x00, 0x00, 0x00,
    // N (78)
    0x00, 0x00, 0xC6, 0xE6, 0xF6, 0xFE, 0xDE, 0xCE, 0xC6, 0xC6, 0xC6, 0xC6, 0x00, 0x00, 0x00, 0x00,
    // O (79)
    0x00, 0x00, 0x7C, 0xC6, 0xC6, 0xC6, 0xC6, 0xC6, 0xC6, 0xC6, 0xC6, 0x7C, 0x00, 0x00, 0x00, 0x00,
    // P (80)
    0x00, 0x00, 0xFC, 0x66, 0x66, 0x66, 0x7C, 0x60, 0x60, 0x60, 0x60, 0xF0, 0x00, 0x00, 0x00, 0x00,
    // Q (81)
    0x00, 0x00, 0x7C, 0xC6, 0xC6, 0xC6, 0xC6, 0xC6, 0xC6, 0xD6, 0xDE, 0x7C, 0x0C, 0x0E, 0x00, 0x00,
    // R (82)
    0x00, 0x00, 0xFC, 0x66, 0x66, 0x66, 0x7C, 0x6C, 0x66, 0x66, 0x66, 0xE6, 0x00, 0x00, 0x00, 0x00,
    // S (83)
    0x00, 0x00, 0x7C, 0xC6, 0xC6, 0x60, 0x38, 0x0C, 0x06, 0xC6, 0xC6, 0x7C, 0x00, 0x00, 0x00, 0x00,
    // T (84)
    0x00, 0x00, 0x7E, 0x7E, 0x5A, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3C, 0x00, 0x00, 0x00, 0x00,
    // U (85)
    0x00, 0x00, 0xC6, 0xC6, 0xC6, 0xC6, 0xC6, 0xC6, 0xC6, 0xC6, 0xC6, 0x7C, 0x00, 0x00, 0x00, 0x00,
    // V (86)
    0x00, 0x00, 0xC6, 0xC6, 0xC6, 0xC6, 0xC6, 0xC6, 0xC6, 0x6C, 0x38, 0x10, 0x00, 0x00, 0x00, 0x00,
    // W (87)
    0x00, 0x00, 0xC6, 0xC6, 0xC6, 0xC6, 0xD6, 0xD6, 0xD6, 0xFE, 0xEE, 0x6C, 0x00, 0x00, 0x00, 0x00,
    // X (88)
    0x00, 0x00, 0xC6, 0xC6, 0x6C, 0x7C, 0x38, 0x38, 0x7C, 0x6C, 0xC6, 0xC6, 0x00, 0x00, 0x00, 0x00,
    // Y (89)
    0x00, 0x00, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x18, 0x18, 0x18, 0x18, 0x3C, 0x00, 0x00, 0x00, 0x00,
    // Z (90)
    0x00, 0x00, 0xFE, 0xC6, 0x86, 0x0C, 0x18, 0x30, 0x60, 0xC2, 0xC6, 0xFE, 0x00, 0x00, 0x00, 0x00,
    // [ (91)
    0x00, 0x00, 0x3C, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x3C, 0x00, 0x00, 0x00, 0x00,
    // \ (92)
    0x00, 0x00, 0x00, 0x80, 0xC0, 0xE0, 0x70, 0x38, 0x1C, 0x0E, 0x06, 0x02, 0x00, 0x00, 0x00, 0x00,
    // ] (93)
    0x00, 0x00, 0x3C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x3C, 0x00, 0x00, 0x00, 0x00,
    // ^ (94)
    0x10, 0x38, 0x6C, 0xC6, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // _ (95)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0x00, 0x00,
    // ` (96)
    0x30, 0x30, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // a (97)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x78, 0x0C, 0x7C, 0xCC, 0xCC, 0xCC, 0x76, 0x00, 0x00, 0x00, 0x00,
    // b (98)
    0x00, 0x00, 0xE0, 0x60, 0x60, 0x78, 0x6C, 0x66, 0x66, 0x66, 0x66, 0x7C, 0x00, 0x00, 0x00, 0x00,
    // c (99)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x7C, 0xC6, 0xC0, 0xC0, 0xC0, 0xC6, 0x7C, 0x00, 0x00, 0x00, 0x00,
    // d (100)
    0x00, 0x00, 0x1C, 0x0C, 0x0C, 0x3C, 0x6C, 0xCC, 0xCC, 0xCC, 0xCC, 0x76, 0x00, 0x00, 0x00, 0x00,
    // e (101)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x7C, 0xC6, 0xFE, 0xC0, 0xC0, 0xC6, 0x7C, 0x00, 0x00, 0x00, 0x00,
    // f (102)
    0x00, 0x00, 0x38, 0x6C, 0x64, 0x60, 0xF0, 0x60, 0x60, 0x60, 0x60, 0xF0, 0x00, 0x00, 0x00, 0x00,
    // g (103)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x76, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0x7C, 0x0C, 0xCC, 0x78, 0x00,
    // h (104)
    0x00, 0x00, 0xE0, 0x60, 0x60, 0x6C, 0x76, 0x66, 0x66, 0x66, 0x66, 0xE6, 0x00, 0x00, 0x00, 0x00,
    // i (105)
    0x00, 0x00, 0x18, 0x18, 0x00, 0x38, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3C, 0x00, 0x00, 0x00, 0x00,
    // j (106)
    0x00, 0x00, 0x06, 0x06, 0x00, 0x0E, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x66, 0x66, 0x3C, 0x00,
    // k (107)
    0x00, 0x00, 0xE0, 0x60, 0x60, 0x66, 0x6C, 0x78, 0x78, 0x6C, 0x66, 0xE6, 0x00, 0x00, 0x00, 0x00,
    // l (108)
    0x00, 0x00, 0x38, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3C, 0x00, 0x00, 0x00, 0x00,
    // m (109)
    0x00, 0x00, 0x00, 0x00, 0x00, 0xEC, 0xFE, 0xD6, 0xD6, 0xD6, 0xD6, 0xC6, 0x00, 0x00, 0x00, 0x00,
    // n (110)
    0x00, 0x00, 0x00, 0x00, 0x00, 0xDC, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x00, 0x00, 0x00, 0x00,
    // o (111)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x7C, 0xC6, 0xC6, 0xC6, 0xC6, 0xC6, 0x7C, 0x00, 0x00, 0x00, 0x00,
    // p (112)
    0x00, 0x00, 0x00, 0x00, 0x00, 0xDC, 0x66, 0x66, 0x66, 0x66, 0x66, 0x7C, 0x60, 0x60, 0xF0, 0x00,
    // q (113)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x76, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0x7C, 0x0C, 0x0C, 0x1E, 0x00,
    // r (114)
    0x00, 0x00, 0x00, 0x00, 0x00, 0xDC, 0x76, 0x66, 0x60, 0x60, 0x60, 0xF0, 0x00, 0x00, 0x00, 0x00,
    // s (115)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x7C, 0xC6, 0x60, 0x38, 0x0C, 0xC6, 0x7C, 0x00, 0x00, 0x00, 0x00,
    // t (116)
    0x00, 0x00, 0x10, 0x30, 0x30, 0xFC, 0x30, 0x30, 0x30, 0x30, 0x36, 0x1C, 0x00, 0x00, 0x00, 0x00,
    // u (117)
    0x00, 0x00, 0x00, 0x00, 0x00, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0x76, 0x00, 0x00, 0x00, 0x00,
    // v (118)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x18, 0x00, 0x00, 0x00, 0x00,
    // w (119)
    0x00, 0x00, 0x00, 0x00, 0x00, 0xC6, 0xC6, 0xD6, 0xD6, 0xD6, 0xFE, 0x6C, 0x00, 0x00, 0x00, 0x00,
    // x (120)
    0x00, 0x00, 0x00, 0x00, 0x00, 0xC6, 0x6C, 0x38, 0x38, 0x38, 0x6C, 0xC6, 0x00, 0x00, 0x00, 0x00,
    // y (121)
    0x00, 0x00, 0x00, 0x00, 0x00, 0xC6, 0xC6, 0xC6, 0xC6, 0xC6, 0xC6, 0x7E, 0x06, 0x0C, 0xF8, 0x00,
    // z (122)
    0x00, 0x00, 0x00, 0x00, 0x00, 0xFE, 0xCC, 0x18, 0x30, 0x60, 0xC6, 0xFE, 0x00, 0x00, 0x00, 0x00,
    // { (123)
    0x00, 0x00, 0x0E, 0x18, 0x18, 0x18, 0x70, 0x18, 0x18, 0x18, 0x18, 0x0E, 0x00, 0x00, 0x00, 0x00,
    // | (124)
    0x00, 0x00, 0x18, 0x18, 0x18, 0x18, 0x00, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00,
    // } (125)
    0x00, 0x00, 0x70, 0x18, 0x18, 0x18, 0x0E, 0x18, 0x18, 0x18, 0x18, 0x70, 0x00, 0x00, 0x00, 0x00,
    // ~ (126)
    0x00, 0x00, 0x76, 0xDC, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

// 64-bit entry point
#[unsafe(naked)]
#[no_mangle]
pub extern "C" fn _start() -> ! {
    core::arch::naked_asm!(
        "mov dx, 0x3F8",
        "mov al, 'K'",
        "out dx, al",
        "mov rsp, 0x200000",
        "call {main}",
        "2: hlt",
        "jmp 2b",
        main = sym kernel_main,
    )
}

// Serial port
const SERIAL_PORT: u16 = 0x3F8;
// PS/2 keyboard port
const KB_DATA_PORT: u16 = 0x60;
const KB_STATUS_PORT: u16 = 0x64;

// ACPI shutdown (QEMU q35)
const ACPI_PM1A_CNT: u16 = 0x604;

unsafe fn outb(port: u16, value: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") value, options(nostack, preserves_flags));
}

unsafe fn outw(port: u16, value: u16) {
    core::arch::asm!("out dx, ax", in("dx") port, in("ax") value, options(nostack, preserves_flags));
}

unsafe fn acpi_shutdown() -> ! {
    // ACPI S5 (soft off) for QEMU q35 machine
    outw(ACPI_PM1A_CNT, 0x2000);
    // If that didn't work, halt
    loop { core::arch::asm!("cli; hlt"); }
}

unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    core::arch::asm!("in al, dx", out("al") value, in("dx") port, options(nostack, preserves_flags));
    value
}

unsafe fn serial_init() {
    outb(SERIAL_PORT + 1, 0x00);
    outb(SERIAL_PORT + 3, 0x80);
    outb(SERIAL_PORT + 0, 0x03);
    outb(SERIAL_PORT + 1, 0x00);
    outb(SERIAL_PORT + 3, 0x03);
    outb(SERIAL_PORT + 2, 0xC7);
    outb(SERIAL_PORT + 4, 0x0B);
}

unsafe fn serial_write(s: &[u8]) {
    for &byte in s {
        for _ in 0..1000 { core::arch::asm!("nop", options(nostack)); }
        outb(SERIAL_PORT, byte);
    }
}

unsafe fn serial_read_char() -> Option<u8> {
    let status = inb(SERIAL_PORT + 5);
    if status & 1 != 0 { Some(inb(SERIAL_PORT)) } else { None }
}

// PS/2 keyboard scancode to ASCII (simple set 1)
fn scancode_to_ascii(scancode: u8, shift: bool) -> Option<u8> {
    // Only handle key press (not release - high bit set)
    if scancode & 0x80 != 0 { return None; }

    let c = match scancode {
        0x02 => if shift { b'!' } else { b'1' },
        0x03 => if shift { b'@' } else { b'2' },
        0x04 => if shift { b'#' } else { b'3' },
        0x05 => if shift { b'$' } else { b'4' },
        0x06 => if shift { b'%' } else { b'5' },
        0x07 => if shift { b'^' } else { b'6' },
        0x08 => if shift { b'&' } else { b'7' },
        0x09 => if shift { b'*' } else { b'8' },
        0x0A => if shift { b'(' } else { b'9' },
        0x0B => if shift { b')' } else { b'0' },
        0x0C => if shift { b'_' } else { b'-' },
        0x0D => if shift { b'+' } else { b'=' },
        0x0E => 0x08, // Backspace
        0x10 => if shift { b'Q' } else { b'q' },
        0x11 => if shift { b'W' } else { b'w' },
        0x12 => if shift { b'E' } else { b'e' },
        0x13 => if shift { b'R' } else { b'r' },
        0x14 => if shift { b'T' } else { b't' },
        0x15 => if shift { b'Y' } else { b'y' },
        0x16 => if shift { b'U' } else { b'u' },
        0x17 => if shift { b'I' } else { b'i' },
        0x18 => if shift { b'O' } else { b'o' },
        0x19 => if shift { b'P' } else { b'p' },
        0x1A => if shift { b'{' } else { b'[' },
        0x1B => if shift { b'}' } else { b']' },
        0x1C => b'\r', // Enter
        0x1E => if shift { b'A' } else { b'a' },
        0x1F => if shift { b'S' } else { b's' },
        0x20 => if shift { b'D' } else { b'd' },
        0x21 => if shift { b'F' } else { b'f' },
        0x22 => if shift { b'G' } else { b'g' },
        0x23 => if shift { b'H' } else { b'h' },
        0x24 => if shift { b'J' } else { b'j' },
        0x25 => if shift { b'K' } else { b'k' },
        0x26 => if shift { b'L' } else { b'l' },
        0x27 => if shift { b':' } else { b';' },
        0x28 => if shift { b'"' } else { b'\'' },
        0x29 => if shift { b'~' } else { b'`' },
        0x2B => if shift { b'|' } else { b'\\' },
        0x2C => if shift { b'Z' } else { b'z' },
        0x2D => if shift { b'X' } else { b'x' },
        0x2E => if shift { b'C' } else { b'c' },
        0x2F => if shift { b'V' } else { b'v' },
        0x30 => if shift { b'B' } else { b'b' },
        0x31 => if shift { b'N' } else { b'n' },
        0x32 => if shift { b'M' } else { b'm' },
        0x33 => if shift { b'<' } else { b',' },
        0x34 => if shift { b'>' } else { b'.' },
        0x35 => if shift { b'?' } else { b'/' },
        0x39 => b' ', // Space
        _ => return None,
    };
    Some(c)
}

unsafe fn kb_read_char(shift: &mut bool) -> Option<u8> {
    // Check if keyboard has data
    let status = inb(KB_STATUS_PORT);
    if status & 1 == 0 { return None; }

    let scancode = inb(KB_DATA_PORT);

    // Handle shift keys
    if scancode == 0x2A || scancode == 0x36 { *shift = true; return None; }
    if scancode == 0xAA || scancode == 0xB6 { *shift = false; return None; }

    scancode_to_ascii(scancode, *shift)
}

// Framebuffer functions
unsafe fn fb_put_pixel(x: u32, y: u32, r: u8, g: u8, b: u8) {
    if x >= FB_WIDTH || y >= FB_HEIGHT { return; }
    let offset = (y * FB_PITCH + x * 4) as usize;
    let ptr = (FB_ADDR as usize + offset) as *mut u8;
    if FB_BGR {
        *ptr = b;
        *ptr.add(1) = g;
        *ptr.add(2) = r;
    } else {
        *ptr = r;
        *ptr.add(1) = g;
        *ptr.add(2) = b;
    }
}

unsafe fn fb_clear(r: u8, g: u8, b: u8) {
    for y in 0..FB_HEIGHT {
        for x in 0..FB_WIDTH {
            fb_put_pixel(x, y, r, g, b);
        }
    }
}

unsafe fn fb_draw_char(x: u32, y: u32, c: u8, fg_r: u8, fg_g: u8, fg_b: u8, bg_r: u8, bg_g: u8, bg_b: u8) {
    let idx = if c >= 32 && c < 127 { (c - 32) as usize } else { 0 };

    for row in 0..16u32 {
        let bits = FONT_8X16[idx * 16 + row as usize];

        for col in 0..8u32 {
            let px = x + col;
            let py = y + row;
            if (bits >> (7 - col)) & 1 != 0 {
                fb_put_pixel(px, py, fg_r, fg_g, fg_b);
            } else {
                fb_put_pixel(px, py, bg_r, bg_g, bg_b);
            }
        }
    }
}

unsafe fn fb_scroll() {
    let row_bytes = (FB_PITCH * CHAR_HEIGHT) as usize;
    let total_rows = TEXT_ROWS - 1;
    let fb_ptr = FB_ADDR as *mut u8;

    for row in 0..total_rows {
        let dst_offset = (row * CHAR_HEIGHT * FB_PITCH) as usize;
        let src_offset = ((row + 1) * CHAR_HEIGHT * FB_PITCH) as usize;
        core::ptr::copy(fb_ptr.add(src_offset), fb_ptr.add(dst_offset), row_bytes);
    }

    // Clear last row
    let last_row_y = (TEXT_ROWS - 1) * CHAR_HEIGHT;
    for y in last_row_y..last_row_y + CHAR_HEIGHT {
        for x in 0..FB_WIDTH {
            fb_put_pixel(x, y, 0, 0, 170); // Blue background
        }
    }

    CURSOR_Y = TEXT_ROWS - 1;
}

unsafe fn fb_putchar(c: u8) {
    match c {
        b'\n' => {
            CURSOR_X = 0;
            CURSOR_Y += 1;
            if CURSOR_Y >= TEXT_ROWS { fb_scroll(); }
        }
        b'\r' => { CURSOR_X = 0; }
        0x08 => {
            if CURSOR_X > 0 {
                CURSOR_X -= 1;
                let px = CURSOR_X * CHAR_WIDTH;
                let py = CURSOR_Y * CHAR_HEIGHT;
                fb_draw_char(px, py, b' ', 255, 255, 255, 0, 0, 170);
            }
        }
        _ => {
            let px = CURSOR_X * CHAR_WIDTH;
            let py = CURSOR_Y * CHAR_HEIGHT;
            fb_draw_char(px, py, c, 255, 255, 255, 0, 0, 170);
            CURSOR_X += 1;
            if CURSOR_X >= TEXT_COLS {
                CURSOR_X = 0;
                CURSOR_Y += 1;
                if CURSOR_Y >= TEXT_ROWS { fb_scroll(); }
            }
        }
    }
}

unsafe fn fb_print(s: &[u8]) {
    for &c in s { fb_putchar(c); }
}

pub unsafe fn console_print(s: &[u8]) {
    serial_write(s);
    fb_print(s);
}

// Helper to print a u8 as decimal
unsafe fn print_u8(val: u8) {
    let mut buf = [0u8; 3];
    let mut pos = 0;
    if val >= 100 {
        buf[pos] = b'0' + val / 100;
        pos += 1;
    }
    if val >= 10 {
        buf[pos] = b'0' + (val / 10) % 10;
        pos += 1;
    }
    buf[pos] = b'0' + val % 10;
    pos += 1;
    console_print(&buf[..pos]);
}

// Helper to print a u16 as decimal
unsafe fn print_u16(val: u16) {
    let mut buf = [0u8; 5];
    let mut pos = 0;
    let mut started = false;

    if val >= 10000 {
        buf[pos] = b'0' + (val / 10000) as u8;
        pos += 1;
        started = true;
    }
    if val >= 1000 || started {
        buf[pos] = b'0' + ((val / 1000) % 10) as u8;
        pos += 1;
        started = true;
    }
    if val >= 100 || started {
        buf[pos] = b'0' + ((val / 100) % 10) as u8;
        pos += 1;
        started = true;
    }
    if val >= 10 || started {
        buf[pos] = b'0' + ((val / 10) % 10) as u8;
        pos += 1;
    }
    buf[pos] = b'0' + (val % 10) as u8;
    pos += 1;
    console_print(&buf[..pos]);
}

// Helper to print a u32 as hex
unsafe fn print_hex32(val: u32) {
    let hex = b"0123456789ABCDEF";
    let mut buf = [0u8; 8];
    for i in 0..8 {
        let nibble = ((val >> (28 - i * 4)) & 0xF) as usize;
        buf[i] = hex[nibble];
    }
    console_print(&buf);
}

// Helper to print u64 size right-aligned in 10 chars
unsafe fn print_size_padded(val: u64) {
    let mut buf = [b' '; 10];  // Pre-fill with spaces for right alignment
    let mut n = val;
    let mut pos = 9;

    loop {
        buf[pos] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 || pos == 0 {
            break;
        }
        pos -= 1;
    }
    console_print(&buf);
}

#[no_mangle]
pub extern "C" fn kernel_main() -> ! {
    unsafe {
        serial_init();
        serial_write(b"Kernel starting...\r\n");
    }

    let boot_info = unsafe { &*(BOOT_INFO_ADDR as *const BootInfo) };

    unsafe {
        if boot_info.magic == BOOT_MAGIC {
            serial_write(b"Boot info found!\r\n");
            FB_ADDR = boot_info.framebuffer_addr;
            FB_WIDTH = boot_info.framebuffer_width;
            FB_HEIGHT = boot_info.framebuffer_height;
            FB_PITCH = boot_info.framebuffer_pitch;
            FB_BGR = boot_info.pixel_format == 1;
            TEXT_COLS = FB_WIDTH / CHAR_WIDTH;
            TEXT_ROWS = FB_HEIGHT / CHAR_HEIGHT;
        } else {
            serial_write(b"No boot info!\r\n");
            loop { core::arch::asm!("hlt"); }
        }
    }

    // Clear screen to blue
    unsafe {
        fb_clear(0, 0, 170);
        serial_write(b"Screen cleared\r\n");
    }

    // Initialize heap
    unsafe {
        const HEAP_START: usize = 0x300000;
        const HEAP_SIZE: usize = 64 * 1024;
        ALLOCATOR.lock().init(HEAP_START, HEAP_SIZE);
    }

    // Initialize interrupts (timer + keyboard)
    unsafe {
        serial_write(b"Initializing interrupts...\r\n");
    }
    interrupts::init();
    unsafe {
        serial_write(b"Interrupts enabled\r\n");
    }

    // Display boot banner
    unsafe {
        console_print(b"=====================================\n");
        console_print(b"         DOS64 Kernel v0.1\n");
        console_print(b"   64-bit DOS-compatible OS in Rust\n");
        console_print(b"=====================================\n");
        console_print(b"\n");

        // Auto-detect network
        console_print(b"Detecting network... ");
        if let Some(nic) = net::E1000::new() {
            console_print(b"e1000 found! MAC=");
            let mac = nic.mac_address();
            let hex = b"0123456789ABCDEF";
            for (i, &b) in mac.iter().enumerate() {
                console_print(&[hex[(b >> 4) as usize], hex[(b & 0xF) as usize]]);
                if i < 5 { console_print(b":"); }
            }
            if nic.link_status() {
                console_print(b" Link=UP\n");
            } else {
                console_print(b" Link=DOWN\n");
            }
        } else {
            console_print(b"none\n");
        }

        // Auto-detect and mount disks
        console_print(b"Scanning disks... ");
        // Debug: Check what AHCI controller we find
        if let Some(dev) = net::pci::find_ahci() {
            dev.enable();
            console_print(b"AHCI@");
            print_u8(dev.bus);
            console_print(b":");
            print_u8(dev.device);
            console_print(b"\n");
            // Show port status (AHCI uses BAR5 for ABAR)
            let mmio = dev.ahci_base();
            if mmio != 0 {
                let pi = core::ptr::read_volatile((mmio + 0x0C) as *const u32);
                console_print(b"  PI=0x");
                print_hex32(pi);
                for port in 0..6u8 {
                    if pi & (1 << port) != 0 {
                        let port_base = mmio + 0x100 + (port as u64 * 0x80);
                        let ssts = core::ptr::read_volatile((port_base + 0x28) as *const u32);
                        let det = ssts & 0xF;
                        let ipm = (ssts >> 8) & 0xF;
                        console_print(b" P");
                        print_u8(port);
                        console_print(b"=");
                        print_u8(det as u8);
                        console_print(b"/");
                        print_u8(ipm as u8);
                    }
                }
                console_print(b"\n");
            }
            console_print(b"  ");
        }
        let num_drives = disk::init_drives();
        print_u8(num_drives);
        console_print(b" drive(s) mounted\n");

        console_print(b"\nType 'help' for commands.\n");
        console_print(b"\n");
        runtime::register_default_runtimes();
        let drive = disk::drive_manager().current_drive();
        console_print(&[drive, b':', b'\\', b'>', b' ']);
    }

    // Main command loop
    let mut command_buffer = [0u8; 64];
    let mut buffer_pos: usize = 0;
    let mut shift_pressed = false;

    loop {
        // Wait for interrupt first (power saving)
        interrupts::halt();
        runtime::poll_tasks();

        // After waking, check for input
        let ch = unsafe {
            if let Some(scancode) = interrupts::get_scancode() {
                // Convert interrupt scancode to char
                if scancode == 0x2A || scancode == 0x36 { shift_pressed = true; None }
                else if scancode == 0xAA || scancode == 0xB6 { shift_pressed = false; None }
                else { scancode_to_ascii(scancode, shift_pressed) }
            } else if let Some(c) = serial_read_char() {
                Some(c)
            } else {
                None
            }
        };

        if let Some(ch) = ch {
            match ch {
                b'\r' | b'\n' => {
                    unsafe { console_print(b"\n"); }

                    let cmd = core::str::from_utf8(&command_buffer[..buffer_pos]).unwrap_or("");

                    match cmd.trim() {
                        "help" => unsafe {
                            console_print(b"Commands:\n");
                            console_print(b"  help       - Show this help\n");
                            console_print(b"  ver        - Show version\n");
                            console_print(b"  vol        - Show mounted volumes\n");
                            console_print(b"  C: D: etc  - Change drive\n");
                            console_print(b"  dir        - List files\n");
                            console_print(b"  type <f>   - Display file contents\n");
                            console_print(b"  chkdsk     - Check filesystem\n");
                            console_print(b"  format     - Format current drive\n");
                            console_print(b"  mem        - Memory info\n");
                            console_print(b"  net        - Network info\n");
                            console_print(b"  ifconfig   - Show IP config\n");
                            console_print(b"  ping <ip>  - Ping a host\n");
                            console_print(b"  cls        - Clear screen\n");
                            console_print(b"  exit       - Shutdown\n");
                        },
                        "ver" => unsafe {
                            console_print(b"DOS64 Version 0.1\n");
                            console_print(b"WFS Filesystem v1.0\n");
                        },
                        "vol" => unsafe {
                            console_print(b"Mounted Volumes:\n");
                            console_print(b"Drive  Type    Size     Status\n");
                            console_print(b"-----  ------  -------  ------\n");
                            for drive in disk::drive_manager().list_drives() {
                                console_print(&[drive.letter, b':', b' ', b' ', b' ', b' ', b' ']);
                                let fs_str = drive.fs_type_str();
                                console_print(fs_str.as_bytes());
                                for _ in fs_str.len()..8 {
                                    console_print(b" ");
                                }
                                print_u16(drive.size_mb() as u16);
                                console_print(b" MB   Ready\n");
                            }
                        },
                        "chkdsk" => unsafe {
                            console_print(b"Checking filesystem...\n");
                            if let Some(ahci) = disk::AhciController::new() {
                                if let Some(mut wfs) = disk::Wfs::mount(ahci) {
                                    let result = wfs.check_filesystem(false);
                                    console_print(b"Superblocks checked: ");
                                    print_u8(result.superblocks_checked as u8);
                                    console_print(b"\n");
                                    console_print(b"Files checked:       ");
                                    print_u16(result.files_checked as u16);
                                    console_print(b"\n");
                                    console_print(b"Blocks checked:      ");
                                    print_u16(result.blocks_checked as u16);
                                    console_print(b"\n");
                                    if result.errors_found > 0 {
                                        console_print(b"ERRORS FOUND:        ");
                                        print_u16(result.errors_found as u16);
                                        console_print(b"\n");
                                        console_print(b"Bad blocks:          ");
                                        print_u16(result.bad_blocks as u16);
                                        console_print(b"\n");
                                    } else {
                                        console_print(b"No errors found.\n");
                                    }
                                } else {
                                    console_print(b"No WFS filesystem on current drive\n");
                                }
                            } else {
                                console_print(b"No disk controller found\n");
                            }
                        },
                        "format" => unsafe {
                            // Format current drive
                            let current = disk::drive_manager().current_drive();
                            console_print(b"Format drive ");
                            console_print(&[current, b':']);
                            console_print(b" with WFS? (y/n): ");
                            console_print(b"\nWARNING: All data will be lost!\n");
                            console_print(b"Use 'format X:' to format drive X\n");
                        },
                        "dir" => unsafe {
                            let current = disk::drive_manager().current_drive();
                            if let Some(drive) = disk::drive_manager().get_drive(current) {
                                console_print(b" Volume in drive ");
                                console_print(&[current]);
                                console_print(b" is ");
                                console_print(drive.fs_type_str().as_bytes());
                                console_print(b" (port ");
                                print_u8(drive.disk_port);
                                console_print(b")\n Directory of ");
                                console_print(&[current, b':', b'\\', b'\n', b'\n']);

                                // Try to read from WFS disk
                                // Need a fresh controller since the previous one was consumed
                                if let Some(ahci) = disk::AhciController::new_port(drive.disk_port) {
                                    match disk::Wfs::try_mount(ahci) {
                                        disk::MountResult::Ok(mut wfs) => {
                                            let mut count = 0u32;
                                            let mut total_size = 0u64;
                                            for entry in wfs.list_files() {
                                                // Show permissions/flags: -rwxshr
                                                // r=readonly, x=exec, s=system, h=hidden, d=directory
                                                let flags = entry.flags;
                                                let perm = [
                                                    if flags & 0x0010 != 0 { b'd' } else { b'-' },  // DIR
                                                    if flags & 0x0002 != 0 { b'r' } else { b'-' },  // READONLY
                                                    if flags & 0x0001 != 0 { b'x' } else { b'-' },  // EXEC
                                                    if flags & 0x0004 != 0 { b's' } else { b'-' },  // SYSTEM
                                                    if flags & 0x0008 != 0 { b'h' } else { b'-' },  // HIDDEN
                                                ];
                                                console_print(&perm);
                                                console_print(b" ");

                                                // Print file size right-aligned (10 chars)
                                                let size = entry.size;
                                                print_size_padded(size);
                                                console_print(b"  ");

                                                // Date (placeholder)
                                                console_print(b"2025-12-25  ");

                                                // Filename (up to 40 chars to fit nicely)
                                                let name = entry.name_str();
                                                for c in name.bytes().take(40) {
                                                    console_print(&[c]);
                                                }
                                                console_print(b"\n");

                                                count += 1;
                                                total_size += size;
                                            }
                                            console_print(b"\n");
                                            print_u16(count as u16);
                                            console_print(b" file(s)  ");
                                            print_size_padded(total_size);
                                            console_print(b" bytes total\n");
                                        }
                                        disk::MountResult::ReadFailed => {
                                            console_print(b"Failed to read disk\n");
                                        }
                                        disk::MountResult::BadMagic(magic) => {
                                            console_print(b"Not a WFS disk (magic: 0x");
                                            print_hex32(magic);
                                            console_print(b")\n");
                                        }
                                        disk::MountResult::CrcMismatch => {
                                            console_print(b"WFS CRC error: sz=");
                                            print_u16(disk::DEBUG_SB_SIZE as u16);
                                            console_print(b" calc=0x");
                                            print_hex32(disk::DEBUG_CALC_CRC);
                                            console_print(b" stored=0x");
                                            print_hex32(disk::DEBUG_STORED_CRC);
                                            console_print(b"\n");
                                        }
                                        disk::MountResult::BadVersion(ver) => {
                                            console_print(b"Unsupported WFS version: ");
                                            print_u16(ver);
                                            console_print(b"\n");
                                        }
                                    }
                                } else {
                                    console_print(b"Disk not accessible\n");
                                }
                            } else {
                                console_print(b"No drives available\n");
                                console_print(b"Use 'vol' to see mounted volumes\n");
                            }
                        },
                        "mem" => unsafe {
                            console_print(b"Memory: 256 MB total\n");
                            console_print(b"Heap:   64 KB allocated\n");
                        },
                        "disk" => unsafe {
                            console_print(b"Disk diagnostic:\n");
                            // Check if AHCI PCI device is found
                            if let Some(dev) = net::pci::find_ahci() {
                                dev.enable();
                                console_print(b"AHCI at PCI ");
                                print_u8(dev.bus);
                                console_print(b":");
                                print_u8(dev.device);
                                console_print(b".");
                                print_u8(dev.function);
                                console_print(b" MMIO=0x");
                                print_hex32(dev.mmio_base() as u32);
                                console_print(b"\n");

                                let mmio = dev.mmio_base();
                                if mmio != 0 {
                                    let pi = core::ptr::read_volatile((mmio + 0x0C) as *const u32);
                                    console_print(b"Ports implemented: 0x");
                                    print_hex32(pi);
                                    console_print(b"\n");
                                    for port in 0..6u8 {
                                        if pi & (1 << port) != 0 {
                                            let port_base = mmio + 0x100 + (port as u64 * 0x80);
                                            let ssts = core::ptr::read_volatile((port_base + 0x28) as *const u32);
                                            console_print(b"  Port ");
                                            print_u8(port);
                                            console_print(b": SSTS=0x");
                                            print_hex32(ssts);
                                            let det = ssts & 0xF;
                                            let ipm = (ssts >> 8) & 0xF;
                                            if det == 3 && ipm == 1 {
                                                console_print(b" [DEVICE PRESENT]\n");
                                            } else {
                                                console_print(b" [no device]\n");
                                            }
                                        }
                                    }
                                } else {
                                    console_print(b"BAR0 not configured!\n");
                                }
                            } else {
                                console_print(b"No AHCI controller found via PCI\n");
                            }
                        },
                        "net" => unsafe {
                            console_print(b"Detecting network card...\n");
                            if let Some(nic) = net::E1000::new() {
                                console_print(b"e1000 NIC found!\n");
                                console_print(b"MAC: ");
                                let mac = nic.mac_address();
                                for (i, &b) in mac.iter().enumerate() {
                                    let hex = b"0123456789ABCDEF";
                                    console_print(&[hex[(b >> 4) as usize], hex[(b & 0xF) as usize]]);
                                    if i < 5 { console_print(b":"); }
                                }
                                console_print(b"\n");
                                if nic.link_status() {
                                    console_print(b"Link: UP\n");
                                } else {
                                    console_print(b"Link: DOWN\n");
                                }
                            } else {
                                console_print(b"No e1000 NIC found\n");
                            }
                        },
                        "ifconfig" => unsafe {
                            if let Some(nic) = net::E1000::new() {
                                let stack = net::NetworkStack::new(nic);
                                console_print(b"eth0:\n");
                                console_print(b"  IP:      10.0.2.15\n");
                                console_print(b"  Netmask: 255.255.255.0\n");
                                console_print(b"  Gateway: 10.0.2.2\n");
                                console_print(b"  MAC:     ");
                                let mac = stack.nic.mac_address();
                                let hex = b"0123456789ABCDEF";
                                for (i, &b) in mac.iter().enumerate() {
                                    console_print(&[hex[(b >> 4) as usize], hex[(b & 0xF) as usize]]);
                                    if i < 5 { console_print(b":"); }
                                }
                                console_print(b"\n");
                                if stack.nic.link_status() {
                                    console_print(b"  Status:  UP\n");
                                } else {
                                    console_print(b"  Status:  DOWN\n");
                                }
                            } else {
                                console_print(b"No network interface found\n");
                            }
                        },
                        "netdiag" => unsafe {
                            console_print(b"Network Diagnostics\n");
                            console_print(b"-------------------\n");
                            if let Some(mut nic) = net::E1000::new() {
                                console_print(b"NIC: e1000 OK\n");
                                console_print(b"Link: ");
                                if nic.link_status() {
                                    console_print(b"UP\n");
                                } else {
                                    console_print(b"DOWN\n");
                                }

                                // Show MMIO base
                                console_print(b"MMIO: ");
                                print_hex32(nic.get_mmio_base() as u32);
                                console_print(b"\n");

                                // Test TX by sending a simple ARP
                                console_print(b"TX test: sending ARP... ");
                                let test_pkt = [
                                    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, // dst MAC (broadcast)
                                    0x52, 0x54, 0x00, 0x12, 0x34, 0x56, // src MAC
                                    0x08, 0x06, // EtherType: ARP
                                    0x00, 0x01, // Hardware: Ethernet
                                    0x08, 0x00, // Protocol: IPv4
                                    0x06, 0x04, // HW size, Proto size
                                    0x00, 0x01, // Opcode: Request
                                    0x52, 0x54, 0x00, 0x12, 0x34, 0x56, // Sender MAC
                                    10, 0, 2, 15, // Sender IP
                                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Target MAC
                                    10, 0, 2, 2, // Target IP (gateway)
                                ];

                                // Send multiple times
                                for i in 0..3 {
                                    if nic.send(&test_pkt) {
                                        console_print(b".");
                                    } else {
                                        console_print(b"X");
                                    }
                                    // Small delay between sends
                                    for _ in 0..100000 { core::arch::asm!("nop"); }
                                }
                                console_print(b" done\n");

                                // Wait and check for any received packets
                                console_print(b"RX test: waiting 3 sec...\n");
                                interrupts::enable_timer();
                                let mut rx_buf = [0u8; 2048];
                                let mut received = 0u16;
                                let start_ticks = interrupts::get_ticks();
                                // Wait ~3 seconds (about 55 ticks at 18.2Hz)
                                while interrupts::get_ticks().wrapping_sub(start_ticks) < 55 {
                                    let len = nic.recv(&mut rx_buf);
                                    if len > 0 {
                                        received += 1;
                                        console_print(b"  Pkt ");
                                        print_u16(received);
                                        console_print(b": ");
                                        print_u16(len as u16);
                                        console_print(b" bytes, EthType=0x");
                                        let hex = b"0123456789ABCDEF";
                                        console_print(&[hex[(rx_buf[12] >> 4) as usize]]);
                                        console_print(&[hex[(rx_buf[12] & 0xF) as usize]]);
                                        console_print(&[hex[(rx_buf[13] >> 4) as usize]]);
                                        console_print(&[hex[(rx_buf[13] & 0xF) as usize]]);
                                        console_print(b"\n");
                                    }
                                    interrupts::halt(); // Wait for next timer tick
                                }
                                interrupts::disable_timer();
                                console_print(b"Total: ");
                                print_u16(received);
                                console_print(b" packets\n");
                            } else {
                                console_print(b"No NIC found\n");
                            }
                        },
                        "cls" => unsafe {
                            fb_clear(0, 0, 170);
                            CURSOR_X = 0;
                            CURSOR_Y = 0;
                        },
                        "exit" => unsafe {
                            console_print(b"System shutting down...\n");
                            acpi_shutdown();
                        },
                        "" => {},
                        _ if cmd.trim().starts_with("type ") => unsafe {
                            let filename = cmd.trim().strip_prefix("type ").unwrap_or("").trim();
                            if filename.is_empty() {
                                console_print(b"Usage: type <filename>\n");
                            } else {
                                // Get current drive from drive manager (same as DIR command)
                                let current = disk::drive_manager().current_drive();
                                if let Some(drive) = disk::drive_manager().get_drive(current) {
                                    // Use the correct port for this drive
                                    if let Some(ahci) = disk::AhciController::new_port(drive.disk_port) {
                                        if let Some(mut wfs) = disk::Wfs::mount(ahci) {
                                            if let Some(entry) = wfs.find_file(filename) {
                                                // Allocate buffer for file
                                                let mut buf = alloc::vec![0u8; entry.size as usize];
                                                if let Some(size) = wfs.read_file(&entry, &mut buf) {
                                                    // Print file contents
                                                    for &b in &buf[..size] {
                                                        if b == b'\n' {
                                                            console_print(b"\n");
                                                        } else if b >= 0x20 && b < 0x7F {
                                                            console_print(&[b]);
                                                        }
                                                    }
                                                    console_print(b"\n");
                                                } else {
                                                    console_print(b"Error reading file\n");
                                                }
                                            } else {
                                                console_print(b"File not found: ");
                                                console_print(filename.as_bytes());
                                                console_print(b"\n");
                                            }
                                        } else {
                                            console_print(b"No WFS filesystem found\n");
                                        }
                                    } else {
                                        console_print(b"No disk controller found\n");
                                    }
                                } else {
                                    console_print(b"Drive not found\n");
                                }
                            }
                        },
                        _ if cmd.trim().starts_with("format ") => unsafe {
                            let arg = cmd.trim().strip_prefix("format ").unwrap_or("").trim();
                            // Parse drive letter (e.g., "D:" or "D")
                            let letter = if arg.len() >= 1 {
                                let first = arg.as_bytes()[0].to_ascii_uppercase();
                                if first >= b'A' && first <= b'Z' {
                                    Some(first)
                                } else {
                                    None
                                }
                            } else {
                                None
                            };

                            if let Some(letter) = letter {
                                if let Some(_drive) = disk::drive_manager().get_drive(letter) {
                                    console_print(b"Formatting drive ");
                                    console_print(&[letter, b':']);
                                    console_print(b" with WFS...\n");

                                    if disk::drive_manager().format_wfs(letter) {
                                        console_print(b"Format complete.\n");
                                    } else {
                                        console_print(b"Format failed!\n");
                                    }
                                } else {
                                    console_print(b"Drive not found: ");
                                    console_print(&[letter, b':']);
                                    console_print(b"\n");
                                }
                            } else {
                                console_print(b"Usage: format X:\n");
                            }
                        },
                        // Drive letter switching (C:, D:, etc.)
                        _ if cmd.trim().len() == 2 && cmd.trim().ends_with(':') => unsafe {
                            let letter = cmd.trim().as_bytes()[0].to_ascii_uppercase();
                            if disk::drive_manager().set_current_drive(letter) {
                                // Drive changed successfully
                            } else {
                                console_print(b"Invalid drive specification\n");
                            }
                        },
                        _ if cmd.trim().starts_with("ping ") => unsafe {
                            let ip_str = cmd.trim().strip_prefix("ping ").unwrap_or("").trim();
                            if ip_str.is_empty() {
                                console_print(b"Usage: ping <ip>\n");
                            } else if let Some(target) = net::parse_ipv4(ip_str) {
                                console_print(b"Pinging ");
                                console_print(ip_str.as_bytes());
                                console_print(b"...\n");

                                if let Some(nic) = net::E1000::new() {
                                    let mut stack = net::NetworkStack::new(nic);
                                    match stack.ping(target) {
                                        net::PingResult::Success { seq, ttl } => {
                                            console_print(b"Reply from ");
                                            console_print(ip_str.as_bytes());
                                            console_print(b": seq=");
                                            print_u16(seq);
                                            console_print(b" ttl=");
                                            print_u8(ttl);
                                            console_print(b"\n");
                                        }
                                        net::PingResult::Timeout => {
                                            console_print(b"Request timed out.\n");
                                        }
                                        net::PingResult::Unreachable => {
                                            console_print(b"Destination unreachable.\n");
                                        }
                                    }
                                } else {
                                    console_print(b"No network interface.\n");
                                }
                            } else {
                                console_print(b"Invalid IP address\n");
                            }
                        },
                        _ if cmd.trim().starts_with("run ") => unsafe {
                            let filename = cmd.trim().strip_prefix("run ").unwrap_or("").trim();
                            if filename.is_empty() {
                                console_print(b"Usage: run <filename>\n");
                            } else {
                                // Get current drive from drive manager (same as DIR command)
                                let current = disk::drive_manager().current_drive();
                                if let Some(drive) = disk::drive_manager().get_drive(current) {
                                    // Use the correct port for this drive
                                    if let Some(ahci) = disk::AhciController::new_port(drive.disk_port) {
                                        if let Some(mut wfs) = disk::Wfs::mount(ahci) {
                                            if let Some(entry) = wfs.find_file(filename) {
                                                let mut buf = alloc::vec![0u8; entry.size as usize];
                                                if let Some(size) = wfs.read_file(&entry, &mut buf) {
                                                    // Detect format and dispatch to runtime
                                                    match runtime::detect_and_run(filename, &buf[..size]) {
                                                        runtime::RunResult::Scheduled(id) => {
                                                            console_print(b"Task scheduled: ");
                                                            print_u16(id as u16);
                                                            console_print(b"\n");
                                                        }
                                                        runtime::RunResult::Failed => {
                                                            console_print(b"Failed to run file\n");
                                                        }
                                                    }
                                                } else {
                                                    console_print(b"Error reading file\n");
                                                }
                                            } else {
                                                console_print(b"File not found: ");
                                                console_print(filename.as_bytes());
                                                console_print(b"\n");
                                            }
                                        } else {
                                            console_print(b"No WFS filesystem found\n");
                                        }
                                    } else {
                                        console_print(b"No disk controller found\n");
                                    }
                                } else {
                                    console_print(b"Drive not found\n");
                                }
                            }
                        },
                        _ => unsafe {
                            console_print(b"Bad command or file name\n");
                        },
                    }

                    buffer_pos = 0;
                    unsafe {
                        let drive = disk::drive_manager().current_drive();
                        console_print(&[drive, b':', b'\\', b'>', b' ']);
                    }
                }
                0x7F | 0x08 => {
                    if buffer_pos > 0 {
                        buffer_pos -= 1;
                        unsafe {
                            serial_write(b"\x08 \x08");
                            fb_putchar(0x08);
                        }
                    }
                }
                c if c >= 0x20 && c < 0x7F && buffer_pos < 63 => {
                    command_buffer[buffer_pos] = c;
                    buffer_pos += 1;
                    unsafe { console_print(&[c]); }
                }
                _ => {}
            }
        }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    unsafe { serial_write(b"KERNEL PANIC!\r\n"); }
    loop { unsafe { core::arch::asm!("hlt"); } }
}
