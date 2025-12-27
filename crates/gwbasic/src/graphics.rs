//! Graphics module for GW-BASIC

#[cfg(not(feature = "std"))]
use alloc::boxed::Box;

use crate::error::Result;
use crate::graphics_backend::{GraphicsBackend, AsciiBackend};

/// Screen manager that wraps a graphics backend
pub struct Screen {
    backend: Box<dyn GraphicsBackend>,
}

impl Screen {
    pub fn new(width: usize, height: usize) -> Self {
        Screen {
            backend: Box::new(AsciiBackend::new(width, height)),
        }
    }

    pub fn new_with_backend(backend: Box<dyn GraphicsBackend>) -> Self {
        Screen { backend }
    }

    pub fn cls(&mut self) {
        self.backend.cls();
    }

    pub fn locate(&mut self, row: usize, col: usize) -> Result<()> {
        self.backend.locate(row, col)
    }

    pub fn color(&mut self, fg: Option<u8>, bg: Option<u8>) {
        self.backend.color(fg, bg);
    }

    pub fn pset(&mut self, x: i32, y: i32, color: Option<u8>) -> Result<()> {
        let c = color.unwrap_or(7); // Default to white if not specified
        self.backend.pset(x, y, c)
    }

    pub fn line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: Option<u8>) -> Result<()> {
        let c = color.unwrap_or(7);
        self.backend.line(x1, y1, x2, y2, c)
    }

    pub fn circle(&mut self, x: i32, y: i32, radius: i32, color: Option<u8>) -> Result<()> {
        let c = color.unwrap_or(7);
        self.backend.circle(x, y, radius, c)
    }

    pub fn get_cursor(&self) -> (usize, usize) {
        self.backend.get_cursor()
    }

    pub fn get_size(&self) -> (usize, usize) {
        self.backend.get_size()
    }

    pub fn display(&mut self) {
        self.backend.display();
    }

    pub fn should_close(&self) -> bool {
        self.backend.should_close()
    }

    pub fn update(&mut self) -> Result<()> {
        self.backend.update()
    }
}

impl Default for Screen {
    fn default() -> Self {
        Self::new(80, 25)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screen_creation() {
        let screen = Screen::new(80, 25);
        let (height, width) = screen.get_size();
        assert_eq!(width, 80);
        assert_eq!(height, 25);
    }

    #[test]
    fn test_cls() {
        let mut screen = Screen::new(80, 25);
        screen.cls();
        let (row, col) = screen.get_cursor();
        assert_eq!(col, 0);
        assert_eq!(row, 0);
    }

    #[test]
    fn test_locate() {
        let mut screen = Screen::new(80, 25);
        screen.locate(10, 20).unwrap();
        let (row, col) = screen.get_cursor();
        assert_eq!(row, 10);
        assert_eq!(col, 20);
    }
}