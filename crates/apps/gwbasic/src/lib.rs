//! GW-BASIC Interpreter Library
//!
//! This crate provides a reimplementation of the classic GW-BASIC interpreter
//! in safe, modern Rust with full feature parity and compatibility.
//!
//! Supports both `std` (host systems) and `no_std` (WATOS bare metal) builds.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;

// Platform abstraction layer
pub mod platform;

// Console output macros (must come before modules that use them)
#[macro_use]
pub mod console;

pub mod lexer;
pub mod parser;
pub mod interpreter;
pub mod error;
pub mod value;
pub mod functions;
pub mod graphics;
pub mod graphics_backend;
pub mod fileio;

pub use error::{Error, Result};
pub use interpreter::Interpreter;
pub use lexer::{Lexer, Token, TokenType};
pub use parser::{Parser, AstNode};
pub use value::Value;
pub use graphics::Screen;
pub use fileio::{FileManager, FileMode};

/// Version information for the GW-BASIC interpreter
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const NAME: &str = "GW-BASIC (Rust)";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_info() {
        assert!(!VERSION.is_empty());
        assert_eq!(NAME, "GW-BASIC (Rust)");
    }
}
