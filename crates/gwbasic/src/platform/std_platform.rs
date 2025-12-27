//! Standard library platform implementation (for host systems)

use super::{Console, FileSystem, Graphics, System, FileOpenMode, FileHandle};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::time::{SystemTime, UNIX_EPOCH};

thread_local! {
    static RNG_STATE: RefCell<u64> = RefCell::new(12345);
}

/// Standard console implementation
pub struct StdConsole {
    cursor_row: usize,
    cursor_col: usize,
}

impl StdConsole {
    pub fn new() -> Self {
        StdConsole {
            cursor_row: 0,
            cursor_col: 0,
        }
    }
}

impl Default for StdConsole {
    fn default() -> Self {
        Self::new()
    }
}

impl Console for StdConsole {
    fn print(&mut self, s: &str) {
        print!("{}", s);
        let _ = io::stdout().flush();
    }

    fn print_char(&mut self, ch: char) {
        print!("{}", ch);
        let _ = io::stdout().flush();
    }

    fn read_line(&mut self) -> String {
        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(_) => input.trim().to_string(),
            Err(_) => String::new(),
        }
    }

    fn read_char(&mut self) -> Option<char> {
        let mut buffer = [0u8; 1];
        match io::stdin().read_exact(&mut buffer) {
            Ok(_) => Some(buffer[0] as char),
            Err(_) => None,
        }
    }

    fn clear(&mut self) {
        print!("\x1B[2J\x1B[1;1H");
        let _ = io::stdout().flush();
        self.cursor_row = 0;
        self.cursor_col = 0;
    }

    fn set_cursor(&mut self, row: usize, col: usize) {
        print!("\x1B[{};{}H", row + 1, col + 1);
        let _ = io::stdout().flush();
        self.cursor_row = row;
        self.cursor_col = col;
    }

    fn get_cursor(&self) -> (usize, usize) {
        (self.cursor_row, self.cursor_col)
    }

    fn set_color(&mut self, fg: u8, bg: u8) {
        // ANSI color codes (simplified)
        let fg_code = 30 + (fg & 7);
        let bg_code = 40 + (bg & 7);
        print!("\x1B[{};{}m", fg_code, bg_code);
        let _ = io::stdout().flush();
    }
}

/// Standard file system implementation
struct OpenFile {
    reader: Option<BufReader<File>>,
    writer: Option<BufWriter<File>>,
}

pub struct StdFileSystem {
    files: HashMap<i32, OpenFile>,
    next_handle: i32,
}

impl StdFileSystem {
    pub fn new() -> Self {
        StdFileSystem {
            files: HashMap::new(),
            next_handle: 1,
        }
    }
}

impl Default for StdFileSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl FileSystem for StdFileSystem {
    fn open(&mut self, path: &str, mode: FileOpenMode) -> Result<FileHandle, &'static str> {
        let (reader, writer) = match mode {
            FileOpenMode::Input => {
                let file = File::open(path).map_err(|_| "Cannot open file")?;
                (Some(BufReader::new(file)), None)
            }
            FileOpenMode::Output => {
                let file = File::create(path).map_err(|_| "Cannot create file")?;
                (None, Some(BufWriter::new(file)))
            }
            FileOpenMode::Append => {
                let file = OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(path)
                    .map_err(|_| "Cannot open file for append")?;
                (None, Some(BufWriter::new(file)))
            }
            FileOpenMode::Random => {
                let file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .open(path)
                    .map_err(|_| "Cannot open random file")?;
                (Some(BufReader::new(file.try_clone().map_err(|_| "Cannot clone file")?)),
                 Some(BufWriter::new(file)))
            }
        };

        let handle = self.next_handle;
        self.next_handle += 1;
        self.files.insert(handle, OpenFile { reader, writer });
        Ok(FileHandle(handle))
    }

    fn close(&mut self, handle: FileHandle) -> Result<(), &'static str> {
        if let Some(mut file) = self.files.remove(&handle.0) {
            if let Some(ref mut w) = file.writer {
                let _ = w.flush();
            }
            Ok(())
        } else {
            Err("File not open")
        }
    }

    fn read_line(&mut self, handle: FileHandle) -> Result<String, &'static str> {
        if let Some(file) = self.files.get_mut(&handle.0) {
            if let Some(ref mut reader) = file.reader {
                let mut line = String::new();
                reader.read_line(&mut line).map_err(|_| "Read error")?;
                Ok(line.trim_end().to_string())
            } else {
                Err("File not open for reading")
            }
        } else {
            Err("File not open")
        }
    }

    fn write_line(&mut self, handle: FileHandle, data: &str) -> Result<(), &'static str> {
        if let Some(file) = self.files.get_mut(&handle.0) {
            if let Some(ref mut writer) = file.writer {
                writeln!(writer, "{}", data).map_err(|_| "Write error")?;
                Ok(())
            } else {
                Err("File not open for writing")
            }
        } else {
            Err("File not open")
        }
    }

    fn eof(&self, handle: FileHandle) -> bool {
        // Simplified - would need proper tracking
        self.files.get(&handle.0).is_none()
    }
}

/// Standard system implementation
pub struct StdSystem;

impl StdSystem {
    pub fn new() -> Self {
        StdSystem
    }
}

impl Default for StdSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl System for StdSystem {
    fn timer(&self) -> f32 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap();
        (now.as_secs() % 86400) as f32 + (now.subsec_nanos() as f32 / 1_000_000_000.0)
    }

    fn sleep(&self, ms: u32) {
        std::thread::sleep(std::time::Duration::from_millis(ms as u64));
    }

    fn random(&mut self, seed: Option<i32>) -> f32 {
        RNG_STATE.with(|state| {
            let mut s = state.borrow_mut();

            if let Some(sv) = seed {
                if sv < 0 {
                    *s = (sv.abs() as u64) * 1000;
                } else if sv == 0 {
                    return (*s % 1000) as f32 / 1000.0;
                }
            }

            // Simple LCG
            *s = (*s * 1103515245 + 12345) & 0x7fffffff;
            (*s % 1000000) as f32 / 1000000.0
        })
    }
}

/// Get environment variable (std only)
pub fn get_env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// Get command line arguments (std only)
pub fn get_args() -> Vec<String> {
    std::env::args().collect()
}
