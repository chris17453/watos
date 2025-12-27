//! Graphics backend module
//!
//! Provides abstraction for different graphics rendering backends:
//! - ASCII: Terminal-based rendering with characters (always available)
//! - Window: GUI window with pixel-based rendering (std/host only)
//! - WatosVga: VGA/SVGA graphics for WATOS (watos/no_std only)

pub mod ascii;

#[cfg(feature = "host")]
pub mod window;

#[cfg(not(feature = "std"))]
pub mod watos_vga;

pub use ascii::AsciiBackend;

#[cfg(feature = "host")]
pub use window::WindowBackend;

#[cfg(not(feature = "std"))]
pub use watos_vga::{WatosVgaBackend, VideoMode};

use crate::error::Result;

/// Graphics backend trait - abstracts the rendering implementation
pub trait GraphicsBackend {
    /// Set a pixel at (x, y) with the given color
    fn pset(&mut self, x: i32, y: i32, color: u8) -> Result<()>;

    /// Draw a line from (x1, y1) to (x2, y2) with the given color
    fn line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: u8) -> Result<()>;

    /// Draw a circle at (x, y) with radius and color
    fn circle(&mut self, x: i32, y: i32, radius: i32, color: u8) -> Result<()>;

    /// Clear the screen
    fn cls(&mut self);

    /// Set cursor position (for text mode)
    fn locate(&mut self, row: usize, col: usize) -> Result<()>;

    /// Set foreground/background colors
    fn color(&mut self, fg: Option<u8>, bg: Option<u8>);

    /// Display/update the screen
    fn display(&mut self);

    /// Get screen dimensions (height, width)
    fn get_size(&self) -> (usize, usize);

    /// Get cursor position (row, col)
    fn get_cursor(&self) -> (usize, usize);

    /// Check if the window should close (for GUI backends)
    fn should_close(&self) -> bool {
        false
    }

    /// Update the window (for GUI backends that need event polling)
    fn update(&mut self) -> Result<()> {
        Ok(())
    }
}
