//! Built-in functions for GW-BASIC
//!
//! Provides all standard GW-BASIC functions with platform-specific implementations
//! for both std (host) and no_std (WATOS) environments.

#[cfg(not(feature = "std"))]
use alloc::{string::String, format, string::ToString};

#[cfg(feature = "std")]
use std::cell::RefCell;

use crate::error::{Error, Result};
use crate::value::Value;

// Thread-local RNG state for std, static for no_std
#[cfg(feature = "std")]
thread_local! {
    static RNG_STATE: RefCell<u64> = RefCell::new(12345);
}

#[cfg(not(feature = "std"))]
static mut RNG_STATE: u64 = 12345;

/// FFI-safe date structure
#[repr(C)]
#[derive(Copy, Clone)]
pub struct WatosDate {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

/// FFI-safe time structure
#[repr(C)]
#[derive(Copy, Clone)]
pub struct WatosTime {
    pub hours: u8,
    pub minutes: u8,
    pub seconds: u8,
}

/// Math functions
pub fn abs_fn(val: Value) -> Result<Value> {
    Ok(Value::Double(val.as_double()?.abs()))
}

pub fn int_fn(val: Value) -> Result<Value> {
    Ok(Value::Integer(libm::floor(val.as_double()?) as i32))
}

pub fn sqr_fn(val: Value) -> Result<Value> {
    let v = val.as_double()?;
    if v < 0.0 {
        return Err(Error::RuntimeError("Square root of negative number".into()));
    }
    Ok(Value::Double(libm::sqrt(v)))
}

pub fn sin_fn(val: Value) -> Result<Value> {
    Ok(Value::Double(libm::sin(val.as_double()?)))
}

pub fn cos_fn(val: Value) -> Result<Value> {
    Ok(Value::Double(libm::cos(val.as_double()?)))
}

pub fn tan_fn(val: Value) -> Result<Value> {
    Ok(Value::Double(libm::tan(val.as_double()?)))
}

pub fn atn_fn(val: Value) -> Result<Value> {
    Ok(Value::Double(libm::atan(val.as_double()?)))
}

pub fn exp_fn(val: Value) -> Result<Value> {
    Ok(Value::Double(libm::exp(val.as_double()?)))
}

pub fn log_fn(val: Value) -> Result<Value> {
    let v = val.as_double()?;
    if v <= 0.0 {
        return Err(Error::RuntimeError("Logarithm of non-positive number".into()));
    }
    Ok(Value::Double(libm::log(v)))
}

pub fn sgn_fn(val: Value) -> Result<Value> {
    let v = val.as_double()?;
    let sign = if v > 0.0 { 1 } else if v < 0.0 { -1 } else { 0 };
    Ok(Value::Integer(sign))
}

pub fn fix_fn(val: Value) -> Result<Value> {
    Ok(Value::Integer(libm::trunc(val.as_double()?) as i32))
}

pub fn cint_fn(val: Value) -> Result<Value> {
    Ok(Value::Integer(libm::round(val.as_double()?) as i32))
}

pub fn csng_fn(val: Value) -> Result<Value> {
    Ok(Value::Single(val.as_double()? as f32))
}

pub fn cdbl_fn(val: Value) -> Result<Value> {
    Ok(Value::Double(val.as_double()?))
}

/// String functions
pub fn len_fn(val: Value) -> Result<Value> {
    Ok(Value::Integer(val.as_string().len() as i32))
}

pub fn asc_fn(val: Value) -> Result<Value> {
    let s = val.as_string();
    if s.is_empty() {
        return Err(Error::RuntimeError("ASC on empty string".into()));
    }
    Ok(Value::Integer(s.chars().next().unwrap() as i32))
}

pub fn chr_fn(val: Value) -> Result<Value> {
    let code = val.as_integer()?;
    if code < 0 || code > 255 {
        return Err(Error::RuntimeError(format!("CHR$ code out of range: {}", code)));
    }
    Ok(Value::String((code as u8 as char).to_string()))
}

pub fn str_fn(val: Value) -> Result<Value> {
    Ok(Value::String(val.to_string()))
}

pub fn val_fn(val: Value) -> Result<Value> {
    let string = val.as_string();
    let s = string.trim();
    // Try integer first, then float
    if let Some(i) = parse_i32(s) {
        Ok(Value::Integer(i))
    } else if let Some(f) = parse_f64(s) {
        Ok(Value::Double(f))
    } else {
        Ok(Value::Integer(0))
    }
}

pub fn left_fn(s: Value, n: Value) -> Result<Value> {
    let string = s.as_string();
    let count = n.as_integer()? as usize;
    Ok(Value::String(string.chars().take(count).collect()))
}

pub fn right_fn(s: Value, n: Value) -> Result<Value> {
    let string = s.as_string();
    let count = n.as_integer()? as usize;
    let chars: alloc::vec::Vec<char> = string.chars().collect();
    let start = if count > chars.len() { 0 } else { chars.len() - count };
    Ok(Value::String(chars[start..].iter().collect()))
}

pub fn mid_fn(s: Value, start: Value, len: Option<Value>) -> Result<Value> {
    let string = s.as_string();
    let start_pos = (start.as_integer()? - 1).max(0) as usize;
    let chars: alloc::vec::Vec<char> = string.chars().collect();

    if start_pos >= chars.len() {
        return Ok(Value::String(String::new()));
    }

    let result = if let Some(length) = len {
        let count = length.as_integer()? as usize;
        chars[start_pos..].iter().take(count).collect()
    } else {
        chars[start_pos..].iter().collect()
    };

    Ok(Value::String(result))
}

pub fn space_fn(n: Value) -> Result<Value> {
    let count = n.as_integer()?;
    if count < 0 {
        return Err(Error::RuntimeError("SPACE$ count cannot be negative".into()));
    }
    let mut s = String::new();
    for _ in 0..count {
        s.push(' ');
    }
    Ok(Value::String(s))
}

pub fn string_fn(n: Value, ch: Value) -> Result<Value> {
    let count = n.as_integer()?;
    if count < 0 {
        return Err(Error::RuntimeError("STRING$ count cannot be negative".into()));
    }

    let char_code = if ch.is_string() {
        let s = ch.as_string();
        if s.is_empty() {
            return Err(Error::RuntimeError("STRING$ character cannot be empty".into()));
        }
        s.chars().next().unwrap()
    } else {
        let code = ch.as_integer()?;
        if code < 0 || code > 255 {
            return Err(Error::RuntimeError("STRING$ code out of range".into()));
        }
        code as u8 as char
    };

    let mut s = String::new();
    for _ in 0..count {
        s.push(char_code);
    }
    Ok(Value::String(s))
}

pub fn instr_fn(start: Option<Value>, haystack: Value, needle: Value) -> Result<Value> {
    let start_pos = if let Some(s) = start {
        (s.as_integer()? - 1).max(0) as usize
    } else {
        0
    };

    let hay = haystack.as_string();
    let need = needle.as_string();

    if start_pos >= hay.len() {
        return Ok(Value::Integer(0));
    }

    if let Some(pos) = hay[start_pos..].find(&need) {
        Ok(Value::Integer((start_pos + pos + 1) as i32))
    } else {
        Ok(Value::Integer(0))
    }
}

pub fn hex_fn(val: Value) -> Result<Value> {
    Ok(Value::String(format!("{:X}", val.as_integer()?)))
}

pub fn oct_fn(val: Value) -> Result<Value> {
    Ok(Value::String(format!("{:o}", val.as_integer()?)))
}

/// Conversion functions
pub fn peek_fn(_addr: Value) -> Result<Value> {
    // WATOS: could implement actual memory peek via syscall
    Ok(Value::Integer(0))
}

pub fn inp_fn(_port: Value) -> Result<Value> {
    // WATOS: could implement port I/O via syscall
    Ok(Value::Integer(0))
}

/// System functions - RND
pub fn rnd_fn(seed: Option<Value>) -> Result<Value> {
    #[cfg(feature = "std")]
    {
        RNG_STATE.with(|state| {
            let mut s = state.borrow_mut();

            if let Some(seed_val) = seed {
                let sv = seed_val.as_double()?;
                if sv < 0.0 {
                    *s = (sv.abs() * 1000000.0) as u64;
                } else if sv == 0.0 {
                    return Ok(Value::Single((*s % 1000) as f32 / 1000.0));
                }
            }

            // Simple LCG
            *s = (*s * 1103515245 + 12345) & 0x7fffffff;
            Ok(Value::Single((*s % 1000000) as f32 / 1000000.0))
        })
    }

    #[cfg(not(feature = "std"))]
    {
        unsafe {
            if let Some(seed_val) = seed {
                let sv = seed_val.as_double()?;
                if sv < 0.0 {
                    RNG_STATE = (sv.abs() * 1000000.0) as u64;
                } else if sv == 0.0 {
                    return Ok(Value::Single((RNG_STATE % 1000) as f32 / 1000.0));
                }
            }

            // Simple LCG
            RNG_STATE = (RNG_STATE * 1103515245 + 12345) & 0x7fffffff;
            Ok(Value::Single((RNG_STATE % 1000000) as f32 / 1000000.0))
        }
    }
}

/// System functions - TIMER
pub fn timer_fn() -> Result<Value> {
    #[cfg(feature = "std")]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap();
        let seconds_since_midnight = (now.as_secs() % 86400) as f32;
        Ok(Value::Single(seconds_since_midnight))
    }

    #[cfg(not(feature = "std"))]
    {
        // WATOS syscall for timer
        let ticks = unsafe { watos_timer_syscall() };
        // Convert to seconds since midnight (DOS-like 18.2 Hz timer)
        Ok(Value::Single((ticks as f32) / 18.2))
    }
}

/// Additional string functions
pub fn lcase_fn(val: Value) -> Result<Value> {
    // Manual lowercase for no_std
    let s = val.as_string();
    let lower: String = s.chars().map(|c| {
        if c >= 'A' && c <= 'Z' {
            ((c as u8) + 32) as char
        } else {
            c
        }
    }).collect();
    Ok(Value::String(lower))
}

pub fn ucase_fn(val: Value) -> Result<Value> {
    // Manual uppercase for no_std
    let s = val.as_string();
    let upper: String = s.chars().map(|c| {
        if c >= 'a' && c <= 'z' {
            ((c as u8) - 32) as char
        } else {
            c
        }
    }).collect();
    Ok(Value::String(upper))
}

pub fn input_fn(n: Value, _file_num: Option<Value>) -> Result<Value> {
    let count = n.as_integer()? as usize;

    #[cfg(feature = "std")]
    {
        use std::io::{self, Read};
        let mut buffer = vec![0u8; count];
        match io::stdin().read_exact(&mut buffer) {
            Ok(_) => Ok(Value::String(String::from_utf8_lossy(&buffer).to_string())),
            Err(_) => Ok(Value::String(" ".repeat(count))),
        }
    }

    #[cfg(not(feature = "std"))]
    {
        // WATOS: read from console via syscall
        let mut buffer = alloc::vec![0u8; count];
        let read_count = unsafe {
            watos_console_read(buffer.as_mut_ptr(), count)
        };
        if read_count > 0 {
            Ok(Value::String(String::from_utf8_lossy(&buffer[..read_count]).to_string()))
        } else {
            Ok(Value::String(String::new()))
        }
    }
}

/// Conversion functions
pub fn cvi_fn(val: Value) -> Result<Value> {
    let s = val.as_string();
    if s.len() < 2 {
        return Err(Error::RuntimeError("CVI requires 2-byte string".into()));
    }
    let bytes = s.as_bytes();
    let n = i16::from_le_bytes([bytes[0], bytes[1]]) as i32;
    Ok(Value::Integer(n))
}

pub fn cvs_fn(val: Value) -> Result<Value> {
    let s = val.as_string();
    if s.len() < 4 {
        return Err(Error::RuntimeError("CVS requires 4-byte string".into()));
    }
    let bytes = s.as_bytes();
    let n = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    Ok(Value::Single(n))
}

pub fn cvd_fn(val: Value) -> Result<Value> {
    let s = val.as_string();
    if s.len() < 8 {
        return Err(Error::RuntimeError("CVD requires 8-byte string".into()));
    }
    let bytes = s.as_bytes();
    let n = f64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5], bytes[6], bytes[7],
    ]);
    Ok(Value::Double(n))
}

pub fn mki_fn(val: Value) -> Result<Value> {
    let n = val.as_integer()? as i16;
    let bytes = n.to_le_bytes();
    Ok(Value::String(String::from_utf8_lossy(&bytes).to_string()))
}

pub fn mks_fn(val: Value) -> Result<Value> {
    let n = val.as_double()? as f32;
    let bytes = n.to_le_bytes();
    Ok(Value::String(String::from_utf8_lossy(&bytes).to_string()))
}

pub fn mkd_fn(val: Value) -> Result<Value> {
    let n = val.as_double()?;
    let bytes = n.to_le_bytes();
    Ok(Value::String(String::from_utf8_lossy(&bytes).to_string()))
}

/// System functions
pub fn fre_fn(_val: Value) -> Result<Value> {
    // Return available memory (simulated or via syscall)
    #[cfg(not(feature = "std"))]
    {
        let free = unsafe { watos_get_free_memory() };
        return Ok(Value::Integer(free as i32));
    }

    #[cfg(feature = "std")]
    Ok(Value::Integer(65000))
}

pub fn varptr_fn(_var_name: Value) -> Result<Value> {
    Ok(Value::Integer(0))
}

pub fn inkey_fn() -> Result<Value> {
    #[cfg(not(feature = "std"))]
    {
        let key = unsafe { watos_get_key_no_wait() };
        if key == 0 {
            return Ok(Value::String(String::new()));
        }
        Ok(Value::String((key as char).to_string()))
    }

    #[cfg(feature = "std")]
    Ok(Value::String(String::new()))
}

pub fn date_fn() -> Result<Value> {
    #[cfg(feature = "std")]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap();
        let days_since_epoch = now.as_secs() / 86400;
        Ok(Value::String(format!("{:02}-{:02}-{:04}",
            (days_since_epoch % 365) / 30 + 1,
            (days_since_epoch % 365) % 30 + 1,
            1970 + days_since_epoch / 365)))
    }

    #[cfg(not(feature = "std"))]
    {
        let date = unsafe { watos_get_date() };
        Ok(Value::String(format!("{:02}-{:02}-{:04}", date.month, date.day, date.year)))
    }
}

pub fn time_fn() -> Result<Value> {
    #[cfg(feature = "std")]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap();
        let seconds = now.as_secs() % 86400;
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        let secs = seconds % 60;
        Ok(Value::String(format!("{:02}:{:02}:{:02}", hours, minutes, secs)))
    }

    #[cfg(not(feature = "std"))]
    {
        let time = unsafe { watos_get_time() };
        Ok(Value::String(format!("{:02}:{:02}:{:02}", time.hours, time.minutes, time.seconds)))
    }
}

pub fn pos_fn(_dummy: Value) -> Result<Value> {
    #[cfg(not(feature = "std"))]
    {
        let col = unsafe { watos_get_cursor_col() };
        return Ok(Value::Integer(col as i32 + 1));
    }

    #[cfg(feature = "std")]
    Ok(Value::Integer(1))
}

pub fn csrlin_fn() -> Result<Value> {
    #[cfg(not(feature = "std"))]
    {
        let row = unsafe { watos_get_cursor_row() };
        return Ok(Value::Integer(row as i32 + 1));
    }

    #[cfg(feature = "std")]
    Ok(Value::Integer(1))
}

/// File functions (placeholders)
pub fn eof_fn(_file_num: Value) -> Result<Value> {
    Ok(Value::Integer(0))
}

pub fn loc_fn(_file_num: Value) -> Result<Value> {
    Ok(Value::Integer(0))
}

pub fn lof_fn(_file_num: Value) -> Result<Value> {
    Ok(Value::Integer(0))
}

/// Screen functions
pub fn point_fn(_x: Value, _y: Value) -> Result<Value> {
    #[cfg(not(feature = "std"))]
    {
        let x = _x.as_integer()?;
        let y = _y.as_integer()?;
        let color = unsafe { watos_get_pixel(x, y) };
        return Ok(Value::Integer(color as i32));
    }

    #[cfg(feature = "std")]
    Ok(Value::Integer(0))
}

pub fn screen_fn(_row: Value, _col: Value, _color_num: Option<Value>) -> Result<Value> {
    Ok(Value::Integer(32)) // Space
}

/// Error handling functions
pub fn erl_fn() -> Result<Value> {
    Ok(Value::Integer(0))
}

pub fn err_fn() -> Result<Value> {
    Ok(Value::Integer(0))
}

pub fn erdev_fn() -> Result<Value> {
    Ok(Value::Integer(0))
}

pub fn erdev_string_fn() -> Result<Value> {
    Ok(Value::String(String::new()))
}

/// Environment and system functions
pub fn environ_fn(val: Value) -> Result<Value> {
    #[cfg(feature = "std")]
    {
        let var_name = if let Ok(name) = val.as_string_result() {
            name
        } else {
            return Ok(Value::String(String::new()));
        };

        match std::env::var(var_name) {
            Ok(value) => Ok(Value::String(value)),
            Err(_) => Ok(Value::String(String::new())),
        }
    }

    #[cfg(not(feature = "std"))]
    {
        // WATOS doesn't have environment variables
        let _ = val;
        Ok(Value::String(String::new()))
    }
}

/// I/O control functions
pub fn ioctl_fn(_file_num: Value) -> Result<Value> {
    Ok(Value::String(String::new()))
}

/// Joystick functions
pub fn stick_fn(_val: Value) -> Result<Value> {
    Ok(Value::Integer(0))
}

pub fn strig_fn(_val: Value) -> Result<Value> {
    Ok(Value::Integer(0))
}

/// File I/O functions
pub fn fileattr_fn(_filenum: Value, _attribute: Value) -> Result<Value> {
    Ok(Value::Integer(0))
}

pub fn ioctl_string_fn(_filenum: Value) -> Result<Value> {
    Ok(Value::String(String::new()))
}

/// Machine language function call
pub fn usr_fn(_index: Option<Value>, arg: Value) -> Result<Value> {
    let _ = arg.as_double()?;
    Ok(Value::Integer(0))
}

// Helper functions for no_std number parsing
fn parse_i32(s: &str) -> Option<i32> {
    let s = s.trim();
    if s.is_empty() { return None; }

    let (sign, digits) = if s.starts_with('-') {
        (-1i32, &s[1..])
    } else if s.starts_with('+') {
        (1, &s[1..])
    } else {
        (1, s)
    };

    let mut result: i32 = 0;
    for c in digits.chars() {
        if !c.is_ascii_digit() { return None; }
        result = result.checked_mul(10)?.checked_add((c as u8 - b'0') as i32)?;
    }
    Some(result * sign)
}

fn parse_f64(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() { return None; }

    let (sign, rest) = if s.starts_with('-') {
        (-1.0, &s[1..])
    } else if s.starts_with('+') {
        (1.0, &s[1..])
    } else {
        (1.0, s)
    };

    let parts: (&str, &str) = if let Some(pos) = rest.find('.') {
        (&rest[..pos], &rest[pos + 1..])
    } else {
        (rest, "")
    };

    let mut result = 0.0f64;
    for c in parts.0.chars() {
        if !c.is_ascii_digit() { return None; }
        result = result * 10.0 + (c as u8 - b'0') as f64;
    }

    let mut fraction = 0.0f64;
    let mut divisor = 1.0f64;
    for c in parts.1.chars() {
        if !c.is_ascii_digit() { break; }
        divisor *= 10.0;
        fraction += (c as u8 - b'0') as f64 / divisor;
    }

    Some(sign * (result + fraction))
}

// WATOS syscall stubs (implemented by platform module)
#[cfg(not(feature = "std"))]
extern "C" {
    fn watos_timer_syscall() -> u64;
    fn watos_console_read(buf: *mut u8, len: usize) -> usize;
    fn watos_get_free_memory() -> usize;
    fn watos_get_key_no_wait() -> u8;
    fn watos_get_date() -> WatosDate;
    fn watos_get_time() -> WatosTime;
    fn watos_get_cursor_row() -> u8;
    fn watos_get_cursor_col() -> u8;
    fn watos_get_pixel(x: i32, y: i32) -> u8;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_math_functions() {
        assert_eq!(abs_fn(Value::Integer(-5)).unwrap().as_integer().unwrap(), 5);
        assert_eq!(int_fn(Value::Double(3.7)).unwrap().as_integer().unwrap(), 3);
    }

    #[test]
    fn test_string_functions() {
        assert_eq!(len_fn(Value::String("Hello".into())).unwrap().as_integer().unwrap(), 5);
        assert_eq!(asc_fn(Value::String("A".into())).unwrap().as_integer().unwrap(), 65);
        assert_eq!(chr_fn(Value::Integer(65)).unwrap().as_string(), "A");
    }

    #[test]
    fn test_left_right_mid() {
        let s = Value::String("HELLO".into());
        assert_eq!(left_fn(s.clone(), Value::Integer(2)).unwrap().as_string(), "HE");
        assert_eq!(right_fn(s.clone(), Value::Integer(2)).unwrap().as_string(), "LO");
        assert_eq!(mid_fn(s, Value::Integer(2), Some(Value::Integer(3))).unwrap().as_string(), "ELL");
    }

    #[test]
    fn test_case_functions() {
        assert_eq!(lcase_fn(Value::String("HELLO".into())).unwrap().as_string(), "hello");
        assert_eq!(ucase_fn(Value::String("hello".into())).unwrap().as_string(), "HELLO");
    }

    #[test]
    fn test_usr_function() {
        assert_eq!(usr_fn(None, Value::Integer(100)).unwrap().as_integer().unwrap(), 0);
        assert_eq!(usr_fn(Some(Value::Integer(5)), Value::Double(3.14)).unwrap().as_integer().unwrap(), 0);
    }
}
