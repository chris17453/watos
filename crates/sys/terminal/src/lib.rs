//! WATOS Terminal Emulator
//!
//! A no_std ANSI/VT100 compatible terminal emulator.
//! Based on ttyvid's terminal implementation, adapted for bare-metal use.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │  Console Manager                                             │
//! │  - Virtual consoles (tty0-tty11)                            │
//! │  - Alt+Fn switching                                          │
//! ├─────────────────────────────────────────────────────────────┤
//! │  Terminal                                                    │
//! │  - ANSI parser, Grid, Cursor state                          │
//! ├─────────────────────────────────────────────────────────────┤
//! │  Renderer                                                    │
//! │  - Font rendering, dirty tracking                           │
//! ├─────────────────────────────────────────────────────────────┤
//! │  Framebuffer (trait)                                         │
//! │  - Resolution, double buffering, mode switching             │
//! └─────────────────────────────────────────────────────────────┘
//! ```

#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod color;
pub mod cell;
pub mod framebuffer;
pub mod grid;
pub mod state;
pub mod parser;
pub mod renderer;
pub mod keyboard;
pub mod terminal;
pub mod console;

pub use color::Color;
pub use cell::{Cell, CellFlags};
pub use framebuffer::{Framebuffer, PixelFormat};
pub use grid::Grid;
pub use state::TerminalState;
pub use parser::{Parser, Event};
pub use renderer::Renderer;
pub use keyboard::{KeyEvent, KeyCode, Modifiers};
pub use terminal::Terminal;
pub use console::ConsoleManager;
