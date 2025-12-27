//! Global state shared across the kernel
//!
//! This module contains global variables and functions that need to be accessed
//! from multiple parts of the kernel.

// Global cursor position
pub static mut CURSOR_X: u32 = 0;
pub static mut CURSOR_Y: u32 = 0;

// Framebuffer globals
pub static mut FB_ADDR: u64 = 0;
pub static mut FB_WIDTH: u32 = 0;
pub static mut FB_HEIGHT: u32 = 0;
pub static mut FB_PITCH: u32 = 0;
pub static mut FB_BGR: bool = false;

/// Set cursor position
pub fn set_cursor(x: u32, y: u32) {
    unsafe {
        CURSOR_X = x;
        CURSOR_Y = y;
    }
}

/// Clear screen with framebuffer clear implementation
pub fn clear_screen_and_cursor() {
    unsafe {
        // Call framebuffer clear
        fb_clear_impl(0, 0, 170); // Clear to blue
        // Reset cursor
        CURSOR_X = 0;
        CURSOR_Y = 0;
    }
}

/// Framebuffer pixel implementation
pub unsafe fn fb_put_pixel_impl(x: u32, y: u32, r: u8, g: u8, b: u8) {
    if x >= FB_WIDTH || y >= FB_HEIGHT { return; }
    let offset = (y * FB_PITCH + x * 4) as usize;
    let ptr = (FB_ADDR as usize + offset) as *mut u8;
    
    if FB_BGR {
        *ptr.add(0) = b;
        *ptr.add(1) = g;
        *ptr.add(2) = r;
        *ptr.add(3) = 255;
    } else {
        *ptr.add(0) = r;
        *ptr.add(1) = g;
        *ptr.add(2) = b;
        *ptr.add(3) = 255;
    }
}

/// Clear framebuffer to color
pub unsafe fn fb_clear_impl(r: u8, g: u8, b: u8) {
    for y in 0..FB_HEIGHT {
        for x in 0..FB_WIDTH {
            fb_put_pixel_impl(x, y, r, g, b);
        }
    }
}