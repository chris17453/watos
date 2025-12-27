//! Value types for the GW-BASIC interpreter

#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::format;

use crate::error::{Error, Result};
use core::fmt;

/// Represents a value in GW-BASIC
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Integer value
    Integer(i32),

    /// Single-precision floating point
    Single(f32),

    /// Double-precision floating point
    Double(f64),

    /// String value
    String(String),

    /// Nil/Empty value
    Nil,
}

impl Value {
    /// Convert value to integer
    pub fn as_integer(&self) -> Result<i32> {
        match self {
            Value::Integer(i) => Ok(*i),
            Value::Single(f) => Ok(*f as i32),
            Value::Double(d) => Ok(*d as i32),
            Value::String(s) => parse_i32(s)
                .ok_or_else(|| Error::TypeError(format!("Cannot convert '{}' to integer", s))),
            Value::Nil => Ok(0),
        }
    }

    /// Convert value to double
    pub fn as_double(&self) -> Result<f64> {
        match self {
            Value::Integer(i) => Ok(*i as f64),
            Value::Single(f) => Ok(*f as f64),
            Value::Double(d) => Ok(*d),
            Value::String(s) => parse_f64(s)
                .ok_or_else(|| Error::TypeError(format!("Cannot convert '{}' to double", s))),
            Value::Nil => Ok(0.0),
        }
    }

    /// Convert value to string
    pub fn as_string(&self) -> String {
        match self {
            Value::Integer(i) => format_i32(*i),
            Value::Single(f) => format_f32(*f),
            Value::Double(d) => format_f64(*d),
            Value::String(s) => s.clone(),
            Value::Nil => String::new(),
        }
    }

    /// Check if value is numeric
    pub fn is_numeric(&self) -> bool {
        matches!(self, Value::Integer(_) | Value::Single(_) | Value::Double(_))
    }

    /// Check if value is string
    pub fn is_string(&self) -> bool {
        matches!(self, Value::String(_))
    }

    /// Convert value to string with Result
    pub fn as_string_result(&self) -> Result<String> {
        match self {
            Value::String(s) => Ok(s.clone()),
            _ => Err(Error::TypeError("Expected string value".into())),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Integer(i) => write!(f, "{}", i),
            Value::Single(s) => write!(f, "{}", s),
            Value::Double(d) => write!(f, "{}", d),
            Value::String(s) => write!(f, "{}", s),
            Value::Nil => write!(f, ""),
        }
    }
}

// Helper functions for no_std number parsing/formatting
fn parse_i32(s: &str) -> Option<i32> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    let (sign, digits) = if s.starts_with('-') {
        (-1i32, &s[1..])
    } else if s.starts_with('+') {
        (1, &s[1..])
    } else {
        (1, s)
    };

    let mut result: i32 = 0;
    for c in digits.chars() {
        if !c.is_ascii_digit() {
            return None;
        }
        result = result.checked_mul(10)?.checked_add((c as u8 - b'0') as i32)?;
    }

    Some(result * sign)
}

fn parse_f64(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    // Handle sign
    let (sign, rest) = if s.starts_with('-') {
        (-1.0, &s[1..])
    } else if s.starts_with('+') {
        (1.0, &s[1..])
    } else {
        (1.0, s)
    };

    // Split at decimal point
    let parts: (&str, &str) = if let Some(pos) = rest.find('.') {
        (&rest[..pos], &rest[pos + 1..])
    } else {
        (rest, "")
    };

    // Parse integer part
    let mut result = 0.0f64;
    for c in parts.0.chars() {
        if !c.is_ascii_digit() {
            return None;
        }
        result = result * 10.0 + (c as u8 - b'0') as f64;
    }

    // Parse fractional part
    let mut fraction = 0.0f64;
    let mut divisor = 1.0f64;
    for c in parts.1.chars() {
        if !c.is_ascii_digit() {
            break;
        }
        divisor *= 10.0;
        fraction += (c as u8 - b'0') as f64 / divisor;
    }

    Some(sign * (result + fraction))
}

fn format_i32(n: i32) -> String {
    if n == 0 {
        return "0".into();
    }

    let mut result = String::new();
    let (sign, mut value) = if n < 0 {
        ("-", -(n as i64) as u32)
    } else {
        ("", n as u32)
    };

    while value > 0 {
        let digit = (value % 10) as u8 + b'0';
        result.insert(0, digit as char);
        value /= 10;
    }

    let mut final_result = String::from(sign);
    final_result.push_str(&result);
    final_result
}

fn format_f32(n: f32) -> String {
    format_f64(n as f64)
}

fn format_f64(n: f64) -> String {
    // Simple floating point formatting
    if n.is_nan() {
        return "NaN".into();
    }
    if n.is_infinite() {
        return if n > 0.0 { "Inf" } else { "-Inf" }.into();
    }
    if n == 0.0 {
        return "0".into();
    }

    let sign = if n < 0.0 { "-" } else { "" };
    let abs_n = n.abs();

    // Get integer part
    let int_part = libm::trunc(abs_n) as i64;
    let frac_part = abs_n - libm::trunc(abs_n);

    // Format integer part
    let int_str = if int_part == 0 {
        "0".into()
    } else {
        let mut s = String::new();
        let mut v = int_part;
        while v > 0 {
            let digit = (v % 10) as u8 + b'0';
            s.insert(0, digit as char);
            v /= 10;
        }
        s
    };

    // Format fractional part (6 decimal places)
    if frac_part > 0.0 {
        let mut frac_str = String::from(".");
        let mut f = frac_part;
        for _ in 0..6 {
            f *= 10.0;
            let digit = libm::trunc(f) as u8;
            frac_str.push((digit + b'0') as char);
            f = f - libm::trunc(f);
            if f < 0.000001 {
                break;
            }
        }
        // Remove trailing zeros
        while frac_str.ends_with('0') {
            frac_str.pop();
        }
        if frac_str == "." {
            format!("{}{}", sign, int_str)
        } else {
            format!("{}{}{}", sign, int_str, frac_str)
        }
    } else {
        format!("{}{}", sign, int_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integer_value() {
        let val = Value::Integer(42);
        assert_eq!(val.as_integer().unwrap(), 42);
        assert_eq!(val.as_double().unwrap(), 42.0);
        assert_eq!(val.as_string(), "42");
        assert!(val.is_numeric());
        assert!(!val.is_string());
    }

    #[test]
    fn test_string_value() {
        let val = Value::String("Hello".into());
        assert_eq!(val.as_string(), "Hello");
        assert!(!val.is_numeric());
        assert!(val.is_string());
    }

    #[test]
    fn test_value_display() {
        let val = Value::Integer(123);
        assert_eq!(val.to_string(), "123");
    }

    #[test]
    fn test_nil_value() {
        let val = Value::Nil;
        assert_eq!(val.as_integer().unwrap(), 0);
        assert_eq!(val.as_double().unwrap(), 0.0);
        assert_eq!(val.as_string(), "");
    }
}
