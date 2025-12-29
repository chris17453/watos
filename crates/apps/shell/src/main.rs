//! WATOS Shell - Simple command interpreter

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use watos_syscall::numbers as syscall;

#[inline(always)]
unsafe fn syscall0(num: u32) -> u64 {
    let ret: u64;
    core::arch::asm!(
        "int 0x80",
        in("eax") num,
        lateout("rax") ret,
        options(nostack)
    );
    ret
}

#[inline(always)]
unsafe fn syscall1(num: u32, arg1: u64) -> u64 {
    let ret: u64;
    core::arch::asm!(
        "int 0x80",
        in("eax") num,
        in("rdi") arg1,
        lateout("rax") ret,
        options(nostack)
    );
    ret
}

#[inline(always)]
unsafe fn syscall2(num: u32, arg1: u64, arg2: u64) -> u64 {
    let ret: u64;
    core::arch::asm!(
        "int 0x80",
        in("eax") num,
        in("rdi") arg1,
        in("rsi") arg2,
        lateout("rax") ret,
        options(nostack)
    );
    ret
}

#[inline(always)]
unsafe fn syscall3(num: u32, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    let ret: u64;
    core::arch::asm!(
        "int 0x80",
        in("eax") num,
        in("rdi") arg1,
        in("rsi") arg2,
        in("rdx") arg3,
        lateout("rax") ret,
        options(nostack)
    );
    ret
}

#[inline(always)]
unsafe fn syscall4(num: u32, arg1: u64, arg2: u64, arg3: u64, arg4: u64) -> u64 {
    let ret: u64;
    core::arch::asm!(
        "int 0x80",
        in("eax") num,
        in("rdi") arg1,
        in("rsi") arg2,
        in("rdx") arg3,
        in("r10") arg4,
        lateout("rax") ret,
        options(nostack)
    );
    ret
}

fn write_str(s: &str) {
    unsafe {
        syscall3(syscall::SYS_WRITE, 1, s.as_ptr() as u64, s.len() as u64);
    }
}

fn read_line(buf: &mut [u8]) -> usize {
    let mut pos = 0;
    loop {
        let key = loop {
            let k = unsafe { syscall0(syscall::SYS_GETKEY) as u8 };
            if k != 0 {
                break k;
            }
            for _ in 0..10000 { core::hint::spin_loop(); }
        };

        if key == b'\n' || key == b'\r' {
            write_str("\r\n");
            break;
        } else if key == 0x08 || key == 0x7F {
            if pos > 0 {
                pos -= 1;
                write_str("\x08 \x08");
            }
        } else if key >= 0x20 && key < 0x7F && pos < buf.len() {
            buf[pos] = key;
            pos += 1;
            let echo = [key];
            unsafe {
                syscall3(syscall::SYS_WRITE, 1, echo.as_ptr() as u64, 1);
            }
        }
    }
    pos
}

fn exit(code: i32) -> ! {
    unsafe {
        syscall1(syscall::SYS_EXIT, code as u64);
    }
    loop {}
}

/// Expand variables in command line ($VAR, ${VAR}, ~)
/// Returns the length of the expanded command
fn expand_variables(cmd: &[u8], output: &mut [u8]) -> usize {
    let mut out_pos = 0;
    let mut i = 0;
    let cmd_len = cmd.len();

    while i < cmd_len && out_pos < output.len() {
        // Tilde expansion (~ at start or after space)
        if cmd[i] == b'~' && (i == 0 || cmd[i - 1] == b' ') {
            // Expand ~ to HOME
            static mut HOME_BUF: [u8; 128] = [0u8; 128];
            let home_len = unsafe {
                syscall4(
                    syscall::SYS_GETENV,
                    b"HOME".as_ptr() as u64,
                    4,
                    HOME_BUF.as_mut_ptr() as u64,
                    HOME_BUF.len() as u64
                ) as usize
            };

            if home_len > 0 && home_len < 128 {
                let copy_len = core::cmp::min(home_len, output.len() - out_pos);
                unsafe {
                    output[out_pos..out_pos + copy_len].copy_from_slice(&HOME_BUF[..copy_len]);
                }
                out_pos += copy_len;
            } else {
                // No HOME, just copy ~
                output[out_pos] = b'~';
                out_pos += 1;
            }
            i += 1;
        }
        // Variable expansion
        else if cmd[i] == b'$' {
            i += 1; // Skip $

            // Check for ${VAR} syntax
            let is_braced = i < cmd_len && cmd[i] == b'{';
            if is_braced {
                i += 1; // Skip {
            }

            // Extract variable name
            let var_start = i;
            while i < cmd_len {
                let c = cmd[i];
                if is_braced {
                    if c == b'}' {
                        break;
                    }
                } else {
                    // Variable name: alphanumeric and underscore
                    if !((c >= b'A' && c <= b'Z') || (c >= b'a' && c <= b'z') ||
                         (c >= b'0' && c <= b'9') || c == b'_') {
                        break;
                    }
                }
                i += 1;
            }

            let var_name = &cmd[var_start..i];

            if is_braced && i < cmd_len && cmd[i] == b'}' {
                i += 1; // Skip closing }
            }

            // Get variable value
            if var_name.len() > 0 {
                static mut VAR_BUF: [u8; 256] = [0u8; 256];
                let val_len = unsafe {
                    syscall4(
                        syscall::SYS_GETENV,
                        var_name.as_ptr() as u64,
                        var_name.len() as u64,
                        VAR_BUF.as_mut_ptr() as u64,
                        VAR_BUF.len() as u64
                    ) as usize
                };

                if val_len > 0 && val_len < 256 {
                    let copy_len = core::cmp::min(val_len, output.len() - out_pos);
                    unsafe {
                        output[out_pos..out_pos + copy_len].copy_from_slice(&VAR_BUF[..copy_len]);
                    }
                    out_pos += copy_len;
                }
                // If variable not found, expand to empty string (bash behavior)
            }
        }
        // Regular character
        else {
            output[out_pos] = cmd[i];
            out_pos += 1;
            i += 1;
        }
    }

    out_pos
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    write_str("\r\n");
    write_str("WATOS Shell v0.1\r\n");
    write_str("Type 'help' for available commands\r\n");
    write_str("\r\n");

    let mut cmd_buf = [0u8; 128];

    loop {
        write_str("$ ");
        let len = read_line(&mut cmd_buf);

        if len == 0 {
            continue;
        }

        // Trim leading/trailing whitespace
        let mut start = 0;
        let mut end = len;
        while start < end && cmd_buf[start] == b' ' {
            start += 1;
        }
        while end > start && cmd_buf[end - 1] == b' ' {
            end -= 1;
        }

        if start >= end {
            continue; // Empty or whitespace-only
        }

        // Expand variables ($VAR, ${VAR}, ~)
        let mut expanded_buf = [0u8; 256];
        let expanded_len = expand_variables(&cmd_buf[start..end], &mut expanded_buf);
        let cmd = &expanded_buf[..expanded_len];

        // Built-in commands
        if cmd == b"help" {
            write_str("Available commands:\r\n");
            write_str("  help         - Show this help\r\n");
            write_str("  clear        - Clear screen\r\n");
            write_str("  exit         - Exit shell\r\n");
            write_str("  echo         - Echo text\r\n");
            write_str("  ls           - List files\r\n");
            write_str("  pwd          - Print working directory\r\n");
            write_str("  cd           - Change directory\r\n");
            write_str("  uname        - System information\r\n");
            write_str("  ps           - Process list\r\n");
            write_str("  date         - Show date/time\r\n");
            write_str("  export VAR=VALUE - Set environment variable\r\n");
            write_str("  unset VAR    - Unset environment variable\r\n");
            write_str("  env          - List environment variables\r\n");
            write_str("  set          - List environment variables\r\n");
            write_str("\r\n");
        } else if cmd == b"exit" {
            write_str("Goodbye!\r\n");
            exit(0);
        } else if cmd == b"clear" {
            // ANSI clear screen
            write_str("\x1b[2J\x1b[H");
        } else if cmd.starts_with(b"echo ") || cmd.starts_with(b"echo\t") {
            // echo - print arguments (already expanded)
            let text = &cmd[5..]; // Skip "echo "
            unsafe {
                syscall3(syscall::SYS_WRITE, 1, text.as_ptr() as u64, text.len() as u64);
            }
            write_str("\r\n");
        } else if cmd.starts_with(b"export ") || cmd.starts_with(b"export\t") {
            // export VAR=VALUE
            let var_part = &cmd[7..]; // Skip "export "
            if let Some(eq_pos) = var_part.iter().position(|&c| c == b'=') {
                let key = &var_part[..eq_pos];
                let value = &var_part[eq_pos + 1..];

                let result = unsafe {
                    syscall4(
                        syscall::SYS_SETENV,
                        key.as_ptr() as u64,
                        key.len() as u64,
                        value.as_ptr() as u64,
                        value.len() as u64
                    )
                };

                if result != 0 {
                    write_str("export: failed to set variable\r\n");
                }
            } else {
                write_str("export: usage: export VAR=VALUE\r\n");
            }
        } else if cmd.starts_with(b"unset ") || cmd.starts_with(b"unset\t") {
            // unset VAR
            let var_name = &cmd[6..]; // Skip "unset "
            let result = unsafe {
                syscall2(
                    syscall::SYS_UNSETENV,
                    var_name.as_ptr() as u64,
                    var_name.len() as u64
                )
            };

            if result != 0 {
                write_str("unset: failed to unset variable\r\n");
            }
        } else if cmd == b"env" || cmd == b"set" {
            // List all environment variables
            static mut ENV_BUF: [u8; 4096] = [0u8; 4096];

            let num_vars = unsafe {
                syscall2(
                    syscall::SYS_LISTENV,
                    ENV_BUF.as_mut_ptr() as u64,
                    ENV_BUF.len() as u64
                )
            };

            if num_vars > 0 {
                // Parse null-separated strings
                let mut offset = 0;
                unsafe {
                    for _ in 0..num_vars {
                        if offset >= ENV_BUF.len() {
                            break;
                        }

                        // Find null terminator
                        let mut len = 0;
                        while offset + len < ENV_BUF.len() && ENV_BUF[offset + len] != 0 {
                            len += 1;
                        }

                        if len > 0 {
                            syscall3(syscall::SYS_WRITE, 1, ENV_BUF[offset..].as_ptr() as u64, len as u64);
                            write_str("\r\n");
                        }

                        offset += len + 1; // Skip string + null terminator
                    }
                }
            }
        } else {
            // Try to execute as external command
            // Pass the full command line (command + arguments) to exec
            let full_cmdline = cmd;
            let cmdline_len = full_cmdline.len();

            let result = unsafe {
                syscall2(syscall::SYS_EXEC, full_cmdline.as_ptr() as u64, cmdline_len as u64)
            };

            if result != 0 {
                // Command failed to execute
                // Extract just the command name for error message
                let cmd_name_end = full_cmdline.iter()
                    .position(|&c| c == b' ')
                    .unwrap_or(cmdline_len);
                let cmd_name = &full_cmdline[..cmd_name_end];

                write_str("Command not found: ");
                unsafe {
                    syscall3(syscall::SYS_WRITE, 1, cmd_name.as_ptr() as u64, cmd_name.len() as u64);
                }
                write_str("\r\n");
            }
        }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
