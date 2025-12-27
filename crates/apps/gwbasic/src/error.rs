//! Error types for the GW-BASIC interpreter

#[cfg(not(feature = "std"))]
use alloc::string::String;

use core::fmt;

/// Result type alias for GW-BASIC operations
pub type Result<T> = core::result::Result<T, Error>;

/// Error types that can occur during lexing, parsing, or interpretation
#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// Syntax error during lexing or parsing
    SyntaxError(String),

    /// Runtime error during interpretation
    RuntimeError(String),

    /// Type mismatch error
    TypeError(String),

    /// Undefined variable or label
    UndefinedError(String),

    /// Division by zero
    DivisionByZero,

    /// Out of memory
    OutOfMemory,

    /// I/O error
    IoError(String),

    /// Line number error
    LineNumberError(String),

    /// Program termination (END statement)
    ProgramEnd,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::SyntaxError(msg) => write!(f, "Syntax error: {}", msg),
            Error::RuntimeError(msg) => write!(f, "Runtime error: {}", msg),
            Error::TypeError(msg) => write!(f, "Type error: {}", msg),
            Error::UndefinedError(msg) => write!(f, "Undefined: {}", msg),
            Error::DivisionByZero => write!(f, "Division by zero"),
            Error::OutOfMemory => write!(f, "Out of memory"),
            Error::IoError(msg) => write!(f, "I/O error: {}", msg),
            Error::LineNumberError(msg) => write!(f, "Line number error: {}", msg),
            Error::ProgramEnd => write!(f, "Program ended"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(feature = "std"))]
    use alloc::string::ToString;

    #[test]
    fn test_error_display() {
        let err = Error::SyntaxError("unexpected token".into());
        assert_eq!(err.to_string(), "Syntax error: unexpected token");
    }

    #[test]
    fn test_division_by_zero() {
        let err = Error::DivisionByZero;
        assert_eq!(err.to_string(), "Division by zero");
    }
}
