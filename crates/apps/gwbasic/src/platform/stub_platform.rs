//! Stub platform for library-only builds
//!
//! This provides minimal placeholder implementations when the library
//! is compiled without std or watos features. Used for type-checking
//! and library builds only - not functional at runtime.

extern crate alloc;

use super::{Console, FileSystem, Graphics, System, FileOpenMode, FileHandle};
use alloc::string::String;

/// Stub console that panics on any operation
pub struct StubConsole;

impl StubConsole {
    pub fn new() -> Self {
        StubConsole
    }
}

impl Default for StubConsole {
    fn default() -> Self {
        Self::new()
    }
}

impl Console for StubConsole {
    fn print(&mut self, _s: &str) {
        // Stub: no-op
    }

    fn print_char(&mut self, _ch: char) {
        // Stub: no-op
    }

    fn read_line(&mut self) -> String {
        String::new()
    }

    fn read_char(&mut self) -> Option<char> {
        None
    }

    fn clear(&mut self) {
        // Stub: no-op
    }

    fn set_cursor(&mut self, _row: usize, _col: usize) {
        // Stub: no-op
    }

    fn get_cursor(&self) -> (usize, usize) {
        (0, 0)
    }

    fn set_color(&mut self, _fg: u8, _bg: u8) {
        // Stub: no-op
    }
}

/// Stub filesystem that returns errors
pub struct StubFileSystem;

impl StubFileSystem {
    pub fn new() -> Self {
        StubFileSystem
    }
}

impl Default for StubFileSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl FileSystem for StubFileSystem {
    fn open(&mut self, _path: &str, _mode: FileOpenMode) -> Result<FileHandle, &'static str> {
        Err("No filesystem available")
    }

    fn close(&mut self, _handle: FileHandle) -> Result<(), &'static str> {
        Err("No filesystem available")
    }

    fn read_line(&mut self, _handle: FileHandle) -> Result<String, &'static str> {
        Err("No filesystem available")
    }

    fn write_line(&mut self, _handle: FileHandle, _data: &str) -> Result<(), &'static str> {
        Err("No filesystem available")
    }

    fn eof(&self, _handle: FileHandle) -> bool {
        true
    }
}

/// Stub graphics that does nothing
pub struct StubGraphics;

impl StubGraphics {
    pub fn new() -> Self {
        StubGraphics
    }
}

impl Default for StubGraphics {
    fn default() -> Self {
        Self::new()
    }
}

impl Graphics for StubGraphics {
    fn pset(&mut self, _x: i32, _y: i32, _color: u8) {
        // Stub: no-op
    }

    fn line(&mut self, _x1: i32, _y1: i32, _x2: i32, _y2: i32, _color: u8) {
        // Stub: no-op
    }

    fn circle(&mut self, _x: i32, _y: i32, _radius: i32, _color: u8) {
        // Stub: no-op
    }

    fn cls(&mut self) {
        // Stub: no-op
    }

    fn set_mode(&mut self, _mode: u8) {
        // Stub: no-op
    }

    fn get_size(&self) -> (usize, usize) {
        (320, 200)
    }

    fn display(&mut self) {
        // Stub: no-op
    }
}

/// Stub system functions
pub struct StubSystem {
    random_state: u32,
}

impl StubSystem {
    pub fn new() -> Self {
        StubSystem {
            random_state: 12345,
        }
    }
}

impl Default for StubSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl System for StubSystem {
    fn timer(&self) -> f32 {
        0.0
    }

    fn sleep(&self, _ms: u32) {
        // Stub: no-op
    }

    fn random(&mut self, seed: Option<i32>) -> f32 {
        if let Some(s) = seed {
            self.random_state = s as u32;
        }
        // Simple LCG
        self.random_state = self.random_state.wrapping_mul(1103515245).wrapping_add(12345);
        (self.random_state as f32 / u32::MAX as f32)
    }
}
