//! Platform abstraction layer
//!
//! Provides unified interfaces for both std (host) and no_std (WATOS) builds.

#[cfg(feature = "std")]
mod std_platform;
#[cfg(feature = "std")]
pub use std_platform::*;

// WATOS platform: only when watos feature is enabled (no_std + syscalls)
#[cfg(feature = "watos")]
pub mod watos_platform;
#[cfg(feature = "watos")]
pub use watos_platform::*;

// Fallback stub platform for library-only builds (no std, no watos)
#[cfg(all(not(feature = "std"), not(feature = "watos")))]
pub mod stub_platform;
#[cfg(all(not(feature = "std"), not(feature = "watos")))]
pub use stub_platform::*;

// Re-export alloc types for no_std
#[cfg(not(feature = "std"))]
pub use alloc::{
    string::{String, ToString},
    vec::Vec,
    vec,
    format,
    boxed::Box,
    collections::BTreeMap as HashMap,
};

#[cfg(feature = "std")]
pub use std::{
    string::{String, ToString},
    vec::Vec,
    vec,
    format,
    boxed::Box,
    collections::HashMap,
};

/// Console trait for text I/O
pub trait Console {
    fn print(&mut self, s: &str);
    fn print_char(&mut self, ch: char);
    fn read_line(&mut self) -> String;
    fn read_char(&mut self) -> Option<char>;
    fn clear(&mut self);
    fn set_cursor(&mut self, row: usize, col: usize);
    fn get_cursor(&self) -> (usize, usize);
    fn set_color(&mut self, fg: u8, bg: u8);
}

/// File system trait
pub trait FileSystem {
    fn open(&mut self, path: &str, mode: FileOpenMode) -> Result<FileHandle, &'static str>;
    fn close(&mut self, handle: FileHandle) -> Result<(), &'static str>;
    fn read_line(&mut self, handle: FileHandle) -> Result<String, &'static str>;
    fn write_line(&mut self, handle: FileHandle, data: &str) -> Result<(), &'static str>;
    fn eof(&self, handle: FileHandle) -> bool;
}

#[derive(Clone, Copy, PartialEq)]
pub enum FileOpenMode {
    Input,
    Output,
    Append,
    Random,
}

#[derive(Clone, Copy, PartialEq)]
pub struct FileHandle(pub i32);

/// Graphics trait for screen operations
pub trait Graphics {
    fn pset(&mut self, x: i32, y: i32, color: u8);
    fn line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: u8);
    fn circle(&mut self, x: i32, y: i32, radius: i32, color: u8);
    fn cls(&mut self);
    fn set_mode(&mut self, mode: u8);
    fn get_size(&self) -> (usize, usize);
    fn display(&mut self);
}

/// Timer/system functions
pub trait System {
    fn timer(&self) -> f32;
    fn sleep(&self, ms: u32);
    fn random(&mut self, seed: Option<i32>) -> f32;
}
