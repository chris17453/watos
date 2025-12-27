//! BIOS Interrupt Handlers
//!
//! Implements BIOS services (INT 10h, 16h, etc.)

use crate::{DosTask, DosHost};

impl DosTask {
    /// Handle INT 10h (Video BIOS)
    pub fn int10h<H: DosHost>(&mut self, host: &mut H) {
        let ah = self.cpu.ah();

        match ah {
            0x00 => {
                // Set video mode - just clear screen
                host.console_clear(self.console);
            }
            0x01 => {
                // Set cursor shape - ignore
            }
            0x02 => {
                // Set cursor position
                let row = self.cpu.dh();
                let col = self.cpu.dl();
                host.console_set_cursor(self.console, row, col);
            }
            0x03 => {
                // Get cursor position
                let (row, col) = host.console_get_cursor(self.console);
                self.cpu.set_dh(row);
                self.cpu.set_dl(col);
                self.cpu.cx = 0x0607;  // Default cursor shape
            }
            0x05 => {
                // Select active page - ignore
            }
            0x06 => {
                // Scroll up - simplified: just clear if scrolling whole screen
                let lines = self.cpu.al();
                if lines == 0 {
                    host.console_clear(self.console);
                }
                // TODO: Proper scroll implementation
            }
            0x07 => {
                // Scroll down - similar to scroll up
                let lines = self.cpu.al();
                if lines == 0 {
                    host.console_clear(self.console);
                }
            }
            0x08 => {
                // Read character and attribute at cursor
                // Return space with default attribute
                self.cpu.set_al(b' ');
                self.cpu.set_ah(0x07);  // Light gray on black
            }
            0x09 => {
                // Write character and attribute at cursor
                let ch = self.cpu.al();
                let _attr = self.cpu.bl();
                let count = self.cpu.cx;
                for _ in 0..count {
                    host.console_putchar(self.console, ch);
                }
            }
            0x0A => {
                // Write character only at cursor
                let ch = self.cpu.al();
                let count = self.cpu.cx;
                for _ in 0..count {
                    host.console_putchar(self.console, ch);
                }
            }
            0x0E => {
                // Teletype output
                let ch = self.cpu.al();
                host.console_putchar(self.console, ch);
            }
            0x0F => {
                // Get video mode
                self.cpu.set_al(0x03);  // 80x25 text mode
                self.cpu.set_ah(80);    // Columns
                self.cpu.set_bh(0);     // Page
            }
            0x10 => {
                // Set palette registers - ignore
            }
            0x11 => {
                // Character generator - ignore
            }
            0x12 => {
                // Video subsystem configuration
                self.cpu.set_al(0x12);
            }
            0x13 => {
                // Write string
                let row = self.cpu.dh();
                let col = self.cpu.dl();
                host.console_set_cursor(self.console, row, col);

                let seg = self.cpu.es;
                let mut off = self.cpu.bp;
                let count = self.cpu.cx;
                let mode = self.cpu.al();

                for _ in 0..count {
                    let ch = self.memory.read8_segoff(seg, off);
                    off += 1;
                    if mode & 2 != 0 {
                        // Attribute follows character
                        let _attr = self.memory.read8_segoff(seg, off);
                        off += 1;
                    }
                    host.console_putchar(self.console, ch);
                }
            }
            0x1A => {
                // Get/set display combination
                self.cpu.set_al(0x1A);  // Function supported
                self.cpu.set_bl(0x08);  // VGA with color analog display
            }
            _ => {}
        }
    }

    /// Handle INT 16h (Keyboard BIOS)
    pub fn int16h<H: DosHost>(&mut self, host: &mut H) {
        let ah = self.cpu.ah();

        match ah {
            0x00 | 0x10 => {
                // Wait for keystroke
                if let Some(ch) = host.console_getchar(self.console) {
                    self.cpu.set_al(ch);
                    self.cpu.set_ah(0);  // Scan code (simplified)
                }
            }
            0x01 | 0x11 => {
                // Check for keystroke
                if host.console_key_available(self.console) {
                    self.cpu.set_zf(false);
                    // Would need to peek at key
                    self.cpu.ax = 0;
                } else {
                    self.cpu.set_zf(true);
                }
            }
            0x02 | 0x12 => {
                // Get shift flags
                self.cpu.set_al(0);  // No shift keys pressed
            }
            0x03 => {
                // Set typematic rate - ignore
            }
            0x05 => {
                // Store keystroke in buffer - ignore
            }
            _ => {}
        }
    }

    /// Handle INT 1Ah (Time BIOS)
    pub fn int1ah<H: DosHost>(&mut self, host: &mut H) {
        let ah = self.cpu.ah();

        match ah {
            0x00 => {
                // Get system time (ticks since midnight)
                let (hour, min, sec, _) = host.get_time();
                let ticks = (hour as u32) * 65543 + (min as u32) * 1092 + (sec as u32) * 18;
                self.cpu.cx = (ticks >> 16) as u16;
                self.cpu.dx = ticks as u16;
                self.cpu.set_al(0);  // Midnight flag
            }
            0x01 => {
                // Set system time - ignore
            }
            0x02 => {
                // Get RTC time
                let (hour, min, sec, _) = host.get_time();
                self.cpu.set_ch(to_bcd(hour));
                self.cpu.set_cl(to_bcd(min));
                self.cpu.set_dh(to_bcd(sec));
                self.cpu.set_dl(0);  // DST flag
                self.cpu.set_cf(false);
            }
            0x04 => {
                // Get RTC date
                let (year, month, day, _) = host.get_date();
                self.cpu.set_ch(to_bcd((year / 100) as u8));
                self.cpu.set_cl(to_bcd((year % 100) as u8));
                self.cpu.set_dh(to_bcd(month));
                self.cpu.set_dl(to_bcd(day));
                self.cpu.set_cf(false);
            }
            _ => {}
        }
    }

    /// Handle INT 20h (Program Terminate)
    pub fn int20h<H: DosHost>(&mut self, host: &mut H) {
        self.exit_code = 0;
        self.state = crate::TaskState::Terminated;
        host.destroy_console(self.console);
        host.exit_program(0);
    }

    /// Handle INT 27h (Terminate and Stay Resident)
    pub fn int27h<H: DosHost>(&mut self, host: &mut H) {
        // Simplified - just terminate
        self.exit_code = 0;
        self.state = crate::TaskState::Terminated;
        host.destroy_console(self.console);
        host.exit_program(0);
    }
}

/// Convert binary to BCD
fn to_bcd(val: u8) -> u8 {
    ((val / 10) << 4) | (val % 10)
}
