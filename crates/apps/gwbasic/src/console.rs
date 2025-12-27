//! Console output module for GW-BASIC
//!
//! Provides platform-agnostic print macros that work in both std and no_std.

/// External syscall for WATOS console write
#[cfg(not(feature = "std"))]
extern "C" {
    pub fn watos_console_write(buf: *const u8, len: usize);
}

/// Print without newline - platform agnostic
#[cfg(feature = "std")]
#[macro_export]
macro_rules! console_print {
    ($($arg:tt)*) => {
        {
            use std::io::Write;
            print!($($arg)*);
            let _ = std::io::stdout().flush();
        }
    };
}

#[cfg(not(feature = "std"))]
#[macro_export]
macro_rules! console_print {
    ($($arg:tt)*) => {
        {
            use alloc::format;
            let s = format!($($arg)*);
            unsafe {
                $crate::console::watos_console_write(s.as_ptr(), s.len());
            }
        }
    };
}

/// Print with newline - platform agnostic
#[cfg(feature = "std")]
#[macro_export]
macro_rules! console_println {
    () => {
        println!()
    };
    ($($arg:tt)*) => {
        println!($($arg)*)
    };
}

#[cfg(not(feature = "std"))]
#[macro_export]
macro_rules! console_println {
    () => {
        $crate::console_print!("\n")
    };
    ($($arg:tt)*) => {
        {
            $crate::console_print!($($arg)*);
            $crate::console_print!("\n");
        }
    };
}

/// Error print (goes to stderr on std, console on no_std)
#[cfg(feature = "std")]
#[macro_export]
macro_rules! console_eprintln {
    ($($arg:tt)*) => {
        eprintln!($($arg)*)
    };
}

#[cfg(not(feature = "std"))]
#[macro_export]
macro_rules! console_eprintln {
    ($($arg:tt)*) => {
        $crate::console_println!($($arg)*)
    };
}
