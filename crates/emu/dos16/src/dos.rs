//! DOS INT 21h API implementation
//!
//! Implements the DOS API functions accessed via INT 21h.

use crate::{DosTask, DosHost, TaskState};
use crate::host::{FileMode, SeekOrigin, DosError};

impl DosTask {
    /// Handle INT 21h (DOS API)
    pub fn int21h<H: DosHost>(&mut self, host: &mut H) {
        let ah = self.cpu.ah();

        match ah {
            // Character I/O
            0x01 => self.dos_getchar_echo(host),
            0x02 => self.dos_putchar(host),
            0x06 => self.dos_direct_console_io(host),
            0x07 => self.dos_getchar_raw(host),
            0x08 => self.dos_getchar_no_echo(host),
            0x09 => self.dos_print_string(host),
            0x0A => self.dos_buffered_input(host),
            0x0B => self.dos_check_input(host),
            0x0C => self.dos_flush_and_input(host),

            // File operations
            0x0D => self.dos_disk_reset(),
            0x0E => self.dos_select_drive(host),
            0x19 => self.dos_get_current_drive(host),
            0x1A => self.dos_set_dta(),
            0x25 => self.dos_set_vector(),
            0x2A => self.dos_get_date(host),
            0x2C => self.dos_get_time(host),
            0x2F => self.dos_get_dta(),
            0x30 => self.dos_get_version(),
            0x35 => self.dos_get_vector(),

            // Extended file operations
            0x3C => self.dos_create_file(host),
            0x3D => self.dos_open_file(host),
            0x3E => self.dos_close_file(host),
            0x3F => self.dos_read_file(host),
            0x40 => self.dos_write_file(host),
            0x41 => self.dos_delete_file(host),
            0x42 => self.dos_seek_file(host),
            0x43 => self.dos_get_set_attr(host),

            // Directory operations
            0x39 => self.dos_mkdir(host),
            0x3A => self.dos_rmdir(host),
            0x3B => self.dos_chdir(host),
            0x47 => self.dos_get_current_dir(host),

            // Memory management
            0x48 => self.dos_alloc_memory(),
            0x49 => self.dos_free_memory(),
            0x4A => self.dos_resize_memory(),

            // Process control
            0x4C => self.dos_exit(host),
            0x4D => self.dos_get_return_code(),

            // Find files
            0x4E => self.dos_find_first(host),
            0x4F => self.dos_find_next(host),

            // Misc
            0x44 => self.dos_ioctl(host),
            0x56 => self.dos_rename(host),
            0x57 => self.dos_get_set_file_datetime(host),

            _ => {
                // Unknown function - set carry flag
                self.cpu.set_cf(true);
            }
        }
    }

    // Character I/O functions

    fn dos_getchar_echo<H: DosHost>(&mut self, host: &mut H) {
        if let Some(ch) = host.console_getchar(self.console) {
            host.console_putchar(self.console, ch);
            self.cpu.set_al(ch);
        }
    }

    fn dos_putchar<H: DosHost>(&mut self, host: &mut H) {
        let ch = self.cpu.dl();
        host.console_putchar(self.console, ch);
    }

    fn dos_direct_console_io<H: DosHost>(&mut self, host: &mut H) {
        let dl = self.cpu.dl();
        if dl == 0xFF {
            // Input
            if let Some(ch) = host.console_getchar(self.console) {
                self.cpu.set_al(ch);
                self.cpu.set_zf(false);
            } else {
                self.cpu.set_al(0);
                self.cpu.set_zf(true);
            }
        } else {
            // Output
            host.console_putchar(self.console, dl);
        }
    }

    fn dos_getchar_raw<H: DosHost>(&mut self, host: &mut H) {
        if let Some(ch) = host.console_getchar(self.console) {
            self.cpu.set_al(ch);
        }
    }

    fn dos_getchar_no_echo<H: DosHost>(&mut self, host: &mut H) {
        if let Some(ch) = host.console_getchar(self.console) {
            self.cpu.set_al(ch);
        }
    }

    fn dos_print_string<H: DosHost>(&mut self, host: &mut H) {
        let mut offset = self.cpu.dx;
        loop {
            let ch = self.memory.read8_segoff(self.cpu.ds, offset);
            if ch == b'$' {
                break;
            }
            host.console_putchar(self.console, ch);
            offset = offset.wrapping_add(1);
        }
    }

    fn dos_buffered_input<H: DosHost>(&mut self, host: &mut H) {
        let buf_seg = self.cpu.ds;
        let buf_off = self.cpu.dx;
        let max_len = self.memory.read8_segoff(buf_seg, buf_off);

        let mut count = 0u8;
        let mut offset = buf_off + 2;

        while count < max_len {
            if let Some(ch) = host.console_getchar(self.console) {
                if ch == 0x0D {
                    // Enter
                    host.console_putchar(self.console, ch);
                    break;
                } else if ch == 0x08 && count > 0 {
                    // Backspace
                    count -= 1;
                    offset -= 1;
                    host.console_write(self.console, b"\x08 \x08");
                } else if ch >= 0x20 {
                    self.memory.write8_segoff(buf_seg, offset, ch);
                    host.console_putchar(self.console, ch);
                    count += 1;
                    offset += 1;
                }
            }
        }

        self.memory.write8_segoff(buf_seg, buf_off + 1, count);
        self.memory.write8_segoff(buf_seg, offset, 0x0D);
    }

    fn dos_check_input<H: DosHost>(&mut self, host: &mut H) {
        if host.console_key_available(self.console) {
            self.cpu.set_al(0xFF);
        } else {
            self.cpu.set_al(0x00);
        }
    }

    fn dos_flush_and_input<H: DosHost>(&mut self, host: &mut H) {
        // Flush then call function in AL
        let func = self.cpu.al();
        match func {
            0x01 => self.dos_getchar_echo(host),
            0x06 => self.dos_direct_console_io(host),
            0x07 => self.dos_getchar_raw(host),
            0x08 => self.dos_getchar_no_echo(host),
            0x0A => self.dos_buffered_input(host),
            _ => {}
        }
    }

    // Disk/Drive functions

    fn dos_disk_reset(&mut self) {
        self.cpu.set_cf(false);
    }

    fn dos_select_drive<H: DosHost>(&mut self, host: &mut H) {
        let drive = self.cpu.dl();
        if host.set_current_drive(drive).is_ok() {
            self.cpu.set_al(26); // Number of logical drives
        }
    }

    fn dos_get_current_drive<H: DosHost>(&mut self, host: &mut H) {
        self.cpu.set_al(host.get_current_drive());
    }

    fn dos_set_dta(&mut self) {
        // DTA is at DS:DX - just remember it (stored in PSP at offset 0x80)
        // For now, we'll handle this in find_first/find_next
        self.cpu.set_cf(false);
    }

    fn dos_set_vector(&mut self) {
        let vector = self.cpu.al();
        let addr = (vector as usize) * 4;
        self.memory.write16(addr, self.cpu.dx);
        self.memory.write16(addr + 2, self.cpu.ds);
    }

    fn dos_get_date<H: DosHost>(&mut self, host: &mut H) {
        let (year, month, day, dow) = host.get_date();
        self.cpu.cx = year;
        self.cpu.set_dh(month);
        self.cpu.set_dl(day);
        self.cpu.set_al(dow);
    }

    fn dos_get_time<H: DosHost>(&mut self, host: &mut H) {
        let (hour, min, sec, hundredths) = host.get_time();
        self.cpu.set_ch(hour);
        self.cpu.set_cl(min);
        self.cpu.set_dh(sec);
        self.cpu.set_dl(hundredths);
    }

    fn dos_get_dta(&mut self) {
        // Return default DTA at PSP:0080
        self.cpu.es = self.cpu.ds;  // PSP segment
        self.cpu.bx = 0x80;
    }

    fn dos_get_version(&mut self) {
        // DOS 5.0
        self.cpu.set_al(5);
        self.cpu.set_ah(0);
        self.cpu.bx = 0;
        self.cpu.cx = 0;
    }

    fn dos_get_vector(&mut self) {
        let vector = self.cpu.al();
        let addr = (vector as usize) * 4;
        self.cpu.bx = self.memory.read16(addr);
        self.cpu.es = self.memory.read16(addr + 2);
    }

    // File operations

    fn dos_create_file<H: DosHost>(&mut self, host: &mut H) {
        let path = self.read_asciiz_string(self.cpu.ds, self.cpu.dx);
        let attr = self.cpu.cx as u8;

        match host.file_create(&path, attr) {
            Ok(handle) => {
                self.cpu.ax = handle.0;
                self.cpu.set_cf(false);
            }
            Err(e) => {
                self.cpu.ax = e as u16;
                self.cpu.set_cf(true);
            }
        }
    }

    fn dos_open_file<H: DosHost>(&mut self, host: &mut H) {
        let path = self.read_asciiz_string(self.cpu.ds, self.cpu.dx);
        let mode = match self.cpu.al() & 3 {
            0 => FileMode::Read,
            1 => FileMode::Write,
            _ => FileMode::ReadWrite,
        };

        match host.file_open(&path, mode) {
            Ok(handle) => {
                self.cpu.ax = handle.0;
                self.cpu.set_cf(false);
            }
            Err(e) => {
                self.cpu.ax = e as u16;
                self.cpu.set_cf(true);
            }
        }
    }

    fn dos_close_file<H: DosHost>(&mut self, host: &mut H) {
        let handle = crate::host::FileHandle(self.cpu.bx);
        match host.file_close(handle) {
            Ok(()) => self.cpu.set_cf(false),
            Err(e) => {
                self.cpu.ax = e as u16;
                self.cpu.set_cf(true);
            }
        }
    }

    fn dos_read_file<H: DosHost>(&mut self, host: &mut H) {
        let handle = crate::host::FileHandle(self.cpu.bx);
        let count = self.cpu.cx as usize;
        let buf_seg = self.cpu.ds;
        let buf_off = self.cpu.dx;

        let buf_addr = ((buf_seg as usize) << 4) + (buf_off as usize);
        let buffer = self.memory.slice_mut(buf_addr, count);

        match host.file_read(handle, buffer) {
            Ok(n) => {
                self.cpu.ax = n as u16;
                self.cpu.set_cf(false);
            }
            Err(e) => {
                self.cpu.ax = e as u16;
                self.cpu.set_cf(true);
            }
        }
    }

    fn dos_write_file<H: DosHost>(&mut self, host: &mut H) {
        let handle = crate::host::FileHandle(self.cpu.bx);
        let count = self.cpu.cx as usize;
        let buf_seg = self.cpu.ds;
        let buf_off = self.cpu.dx;

        let buf_addr = ((buf_seg as usize) << 4) + (buf_off as usize);
        let buffer = self.memory.slice(buf_addr, count);

        match host.file_write(handle, buffer) {
            Ok(n) => {
                self.cpu.ax = n as u16;
                self.cpu.set_cf(false);
            }
            Err(e) => {
                self.cpu.ax = e as u16;
                self.cpu.set_cf(true);
            }
        }
    }

    fn dos_delete_file<H: DosHost>(&mut self, host: &mut H) {
        let path = self.read_asciiz_string(self.cpu.ds, self.cpu.dx);
        match host.file_delete(&path) {
            Ok(()) => self.cpu.set_cf(false),
            Err(e) => {
                self.cpu.ax = e as u16;
                self.cpu.set_cf(true);
            }
        }
    }

    fn dos_seek_file<H: DosHost>(&mut self, host: &mut H) {
        let handle = crate::host::FileHandle(self.cpu.bx);
        let offset = ((self.cpu.cx as u32) << 16) | (self.cpu.dx as u32);
        let origin = match self.cpu.al() {
            0 => SeekOrigin::Start,
            1 => SeekOrigin::Current,
            _ => SeekOrigin::End,
        };

        match host.file_seek(handle, offset as i32, origin) {
            Ok(pos) => {
                self.cpu.dx = (pos >> 16) as u16;
                self.cpu.ax = pos as u16;
                self.cpu.set_cf(false);
            }
            Err(e) => {
                self.cpu.ax = e as u16;
                self.cpu.set_cf(true);
            }
        }
    }

    fn dos_get_set_attr<H: DosHost>(&mut self, host: &mut H) {
        let path = self.read_asciiz_string(self.cpu.ds, self.cpu.dx);

        match self.cpu.al() {
            0 => {
                // Get attributes
                match host.file_get_attr(&path) {
                    Ok(attr) => {
                        self.cpu.cx = attr as u16;
                        self.cpu.set_cf(false);
                    }
                    Err(e) => {
                        self.cpu.ax = e as u16;
                        self.cpu.set_cf(true);
                    }
                }
            }
            1 => {
                // Set attributes
                let attr = self.cpu.cx as u8;
                match host.file_set_attr(&path, attr) {
                    Ok(()) => self.cpu.set_cf(false),
                    Err(e) => {
                        self.cpu.ax = e as u16;
                        self.cpu.set_cf(true);
                    }
                }
            }
            _ => {
                self.cpu.ax = DosError::UnknownError as u16;
                self.cpu.set_cf(true);
            }
        }
    }

    // Directory operations

    fn dos_mkdir<H: DosHost>(&mut self, host: &mut H) {
        let path = self.read_asciiz_string(self.cpu.ds, self.cpu.dx);
        match host.mkdir(&path) {
            Ok(()) => self.cpu.set_cf(false),
            Err(e) => {
                self.cpu.ax = e as u16;
                self.cpu.set_cf(true);
            }
        }
    }

    fn dos_rmdir<H: DosHost>(&mut self, host: &mut H) {
        let path = self.read_asciiz_string(self.cpu.ds, self.cpu.dx);
        match host.rmdir(&path) {
            Ok(()) => self.cpu.set_cf(false),
            Err(e) => {
                self.cpu.ax = e as u16;
                self.cpu.set_cf(true);
            }
        }
    }

    fn dos_chdir<H: DosHost>(&mut self, host: &mut H) {
        let path = self.read_asciiz_string(self.cpu.ds, self.cpu.dx);
        match host.set_current_dir(&path) {
            Ok(()) => self.cpu.set_cf(false),
            Err(e) => {
                self.cpu.ax = e as u16;
                self.cpu.set_cf(true);
            }
        }
    }

    fn dos_get_current_dir<H: DosHost>(&mut self, host: &mut H) {
        let dir = host.get_current_dir();
        let buf_seg = self.cpu.ds;
        let mut buf_off = self.cpu.si;

        for byte in dir.bytes() {
            self.memory.write8_segoff(buf_seg, buf_off, byte);
            buf_off += 1;
        }
        self.memory.write8_segoff(buf_seg, buf_off, 0);
        self.cpu.set_cf(false);
    }

    // Memory management

    fn dos_alloc_memory(&mut self) {
        let paragraphs = self.cpu.bx;
        match self.memory.alloc_paragraphs(paragraphs) {
            Some(seg) => {
                self.cpu.ax = seg;
                self.cpu.set_cf(false);
            }
            None => {
                self.cpu.ax = DosError::InsufficientMemory as u16;
                self.cpu.bx = 0;  // Largest available block
                self.cpu.set_cf(true);
            }
        }
    }

    fn dos_free_memory(&mut self) {
        let segment = self.cpu.es;
        if self.memory.free_paragraphs(segment) {
            self.cpu.set_cf(false);
        } else {
            self.cpu.ax = DosError::InvalidHandle as u16;
            self.cpu.set_cf(true);
        }
    }

    fn dos_resize_memory(&mut self) {
        // Not fully implemented - just succeed if shrinking
        self.cpu.set_cf(false);
    }

    // Process control

    fn dos_exit<H: DosHost>(&mut self, host: &mut H) {
        self.exit_code = self.cpu.al();
        self.state = TaskState::Terminated;
        host.destroy_console(self.console);
        host.exit_program(self.exit_code);
    }

    fn dos_get_return_code(&mut self) {
        self.cpu.set_al(self.exit_code);
        self.cpu.set_ah(0);  // Normal termination
    }

    // Find files

    fn dos_find_first<H: DosHost>(&mut self, host: &mut H) {
        let pattern = self.read_asciiz_string(self.cpu.ds, self.cpu.dx);
        let attr = self.cpu.cx as u8;

        match host.find_first(&pattern, attr) {
            Ok(entry) => {
                self.write_dta_entry(&entry);
                self.cpu.set_cf(false);
            }
            Err(e) => {
                self.cpu.ax = e as u16;
                self.cpu.set_cf(true);
            }
        }
    }

    fn dos_find_next<H: DosHost>(&mut self, host: &mut H) {
        match host.find_next() {
            Ok(entry) => {
                self.write_dta_entry(&entry);
                self.cpu.set_cf(false);
            }
            Err(e) => {
                self.cpu.ax = e as u16;
                self.cpu.set_cf(true);
            }
        }
    }

    fn write_dta_entry(&mut self, entry: &crate::host::DosDirEntry) {
        // DTA is at PSP:0080 (default)
        let dta_seg = self.cpu.ds;
        let dta_off = 0x80u16;

        // Skip first 21 reserved bytes
        let name_off = dta_off + 21;

        // Attribute at offset 21
        self.memory.write8_segoff(dta_seg, name_off, entry.attr);

        // Time at offset 22-23
        self.memory.write16_segoff(dta_seg, name_off + 1, entry.time);

        // Date at offset 24-25
        self.memory.write16_segoff(dta_seg, name_off + 3, entry.date);

        // Size at offset 26-29
        self.memory.write16_segoff(dta_seg, name_off + 5, entry.size as u16);
        self.memory.write16_segoff(dta_seg, name_off + 7, (entry.size >> 16) as u16);

        // Filename at offset 30 (13 bytes, null-terminated)
        for (i, &b) in entry.name.iter().enumerate() {
            self.memory.write8_segoff(dta_seg, name_off + 9 + i as u16, b);
        }
    }

    // IOCTL

    fn dos_ioctl<H: DosHost>(&mut self, _host: &mut H) {
        // Simplified IOCTL
        match self.cpu.al() {
            0x00 => {
                // Get device information
                let handle = self.cpu.bx;
                if handle <= 4 {
                    // Console/standard handles
                    self.cpu.dx = 0x80D3;  // Character device, IOCTL supported
                } else {
                    self.cpu.dx = 0x0000;  // Regular file
                }
                self.cpu.set_cf(false);
            }
            _ => {
                self.cpu.ax = DosError::UnknownError as u16;
                self.cpu.set_cf(true);
            }
        }
    }

    fn dos_rename<H: DosHost>(&mut self, host: &mut H) {
        let old_path = self.read_asciiz_string(self.cpu.ds, self.cpu.dx);
        let new_path = self.read_asciiz_string(self.cpu.es, self.cpu.di);

        match host.file_rename(&old_path, &new_path) {
            Ok(()) => self.cpu.set_cf(false),
            Err(e) => {
                self.cpu.ax = e as u16;
                self.cpu.set_cf(true);
            }
        }
    }

    fn dos_get_set_file_datetime<H: DosHost>(&mut self, _host: &mut H) {
        // Simplified - just succeed
        self.cpu.set_cf(false);
    }

    // Helper functions

    fn read_asciiz_string(&self, seg: u16, off: u16) -> alloc::string::String {
        let mut s = alloc::string::String::new();
        let mut offset = off;
        loop {
            let ch = self.memory.read8_segoff(seg, offset);
            if ch == 0 {
                break;
            }
            s.push(ch as char);
            offset = offset.wrapping_add(1);
        }
        s
    }
}
