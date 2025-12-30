//! Terminal output utilities for readline
//!
//! Provides ANSI escape sequence handling for cursor movement,
//! line clearing, and other terminal operations.

use watos_syscall::numbers as syscall;

/// Terminal output handler
pub struct Terminal;

impl Terminal {
    /// Write a string to stdout
    pub fn write(s: &str) {
        Self::write_bytes(s.as_bytes());
    }

    /// Write bytes to stdout
    pub fn write_bytes(bytes: &[u8]) {
        unsafe {
            core::arch::asm!(
                "int 0x80",
                in("eax") syscall::SYS_WRITE,
                in("rdi") 1u64,  // stdout
                in("rsi") bytes.as_ptr() as u64,
                in("rdx") bytes.len() as u64,
                lateout("rax") _,
                options(nostack)
            );
        }
    }

    /// Write a single character
    pub fn write_char(ch: char) {
        let mut buf = [0u8; 4];
        let s = ch.encode_utf8(&mut buf);
        Self::write(s);
    }

    /// Move cursor left by n columns
    pub fn move_cursor_left(n: usize) {
        if n > 0 {
            Self::write_csi_param(n, 'D');
        }
    }

    /// Move cursor right by n columns
    pub fn move_cursor_right(n: usize) {
        if n > 0 {
            Self::write_csi_param(n, 'C');
        }
    }

    /// Move cursor up by n rows
    pub fn move_cursor_up(n: usize) {
        if n > 0 {
            Self::write_csi_param(n, 'A');
        }
    }

    /// Move cursor down by n rows
    pub fn move_cursor_down(n: usize) {
        if n > 0 {
            Self::write_csi_param(n, 'B');
        }
    }

    /// Move cursor to specific column (1-based)
    pub fn move_to_column(col: usize) {
        Self::write_csi_param(col, 'G');
    }

    /// Clear from cursor to end of line
    pub fn clear_to_end() {
        Self::write("\x1b[K");
    }

    /// Clear from cursor to start of line
    pub fn clear_to_start() {
        Self::write("\x1b[1K");
    }

    /// Clear entire line
    pub fn clear_line() {
        Self::write("\x1b[2K");
    }

    /// Clear screen
    pub fn clear_screen() {
        Self::write("\x1b[2J\x1b[H");
    }

    /// Save cursor position
    pub fn save_cursor() {
        Self::write("\x1b[s");
    }

    /// Restore cursor position
    pub fn restore_cursor() {
        Self::write("\x1b[u");
    }

    /// Produce a beep/bell
    pub fn beep() {
        Self::write("\x07");
    }

    /// Carriage return (move to start of line)
    pub fn carriage_return() {
        Self::write("\r");
    }

    /// Newline
    pub fn newline() {
        Self::write("\r\n");
    }

    /// Hide cursor
    pub fn hide_cursor() {
        Self::write("\x1b[?25l");
    }

    /// Show cursor
    pub fn show_cursor() {
        Self::write("\x1b[?25h");
    }

    /// Write CSI sequence with numeric parameter
    /// e.g., ESC [ n C for move cursor right
    fn write_csi_param(n: usize, cmd: char) {
        let mut buf = [0u8; 16];
        let mut pos = 0;

        // ESC [
        buf[pos] = 0x1B;
        pos += 1;
        buf[pos] = b'[';
        pos += 1;

        // Write number
        if n == 0 {
            buf[pos] = b'0';
            pos += 1;
        } else {
            let mut temp = [0u8; 10];
            let mut temp_pos = 0;
            let mut val = n;
            while val > 0 {
                temp[temp_pos] = b'0' + (val % 10) as u8;
                temp_pos += 1;
                val /= 10;
            }
            // Reverse the digits
            while temp_pos > 0 {
                temp_pos -= 1;
                buf[pos] = temp[temp_pos];
                pos += 1;
            }
        }

        // Command character
        buf[pos] = cmd as u8;
        pos += 1;

        Self::write_bytes(&buf[..pos]);
    }

    /// Set text attribute (color, bold, etc.)
    pub fn set_attribute(attr: u8) {
        Self::write_csi_param(attr as usize, 'm');
    }

    /// Reset text attributes
    pub fn reset_attributes() {
        Self::write("\x1b[0m");
    }

    /// Set foreground color (0-7 for basic colors)
    pub fn set_fg_color(color: u8) {
        Self::set_attribute(30 + color);
    }

    /// Set background color (0-7 for basic colors)
    pub fn set_bg_color(color: u8) {
        Self::set_attribute(40 + color);
    }

    /// Set bold text
    pub fn set_bold() {
        Self::set_attribute(1);
    }

    /// Set underlined text
    pub fn set_underline() {
        Self::set_attribute(4);
    }

    /// Set reverse video
    pub fn set_reverse() {
        Self::set_attribute(7);
    }
}

/// Color constants
pub mod colors {
    pub const BLACK: u8 = 0;
    pub const RED: u8 = 1;
    pub const GREEN: u8 = 2;
    pub const YELLOW: u8 = 3;
    pub const BLUE: u8 = 4;
    pub const MAGENTA: u8 = 5;
    pub const CYAN: u8 = 6;
    pub const WHITE: u8 = 7;
}
