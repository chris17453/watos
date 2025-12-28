//! WATOS Console Application
//!
//! User-space terminal emulator that provides:
//! - VT100/ANSI terminal emulation
//! - Keyboard input with modifiers (Shift, Ctrl, Alt)
//! - Framebuffer rendering via syscalls
//!
//! This runs as a user-space app, NOT in the kernel.

#![no_std]
#![no_main]

extern crate alloc;

use core::panic::PanicInfo;
use watos_syscall::numbers as syscall;
use watos_terminal::console::ConsoleManager;
use watos_terminal::framebuffer::{FramebufferInfo, PixelFormat, SimpleFramebuffer};
use watos_terminal::keyboard::KeyCode;

// ============================================================================
// Raw Syscall Wrappers
// ============================================================================

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

// ============================================================================
// System Call Helpers
// ============================================================================

fn fb_addr() -> u64 {
    unsafe { syscall0(syscall::SYS_FB_ADDR) }
}

fn fb_dimensions() -> (u32, u32, u32) {
    unsafe {
        let packed = syscall0(syscall::SYS_FB_DIMENSIONS);
        let width = (packed >> 32) as u32;
        let height = ((packed >> 16) & 0xFFFF) as u32;
        let pitch = (packed & 0xFFFF) as u32 * 4;
        (width, height, pitch)
    }
}

fn read_scancode() -> u8 {
    unsafe { syscall0(syscall::SYS_READ_SCANCODE) as u8 }
}

fn get_ticks() -> u64 {
    unsafe { syscall0(syscall::SYS_GETTICKS) }
}

fn serial_write(s: &str) {
    unsafe {
        syscall3(syscall::SYS_WRITE, 0, s.as_ptr() as u64, s.len() as u64);
    }
}

fn getcwd(buf: &mut [u8]) -> usize {
    unsafe { syscall2(syscall::SYS_GETCWD, buf.as_mut_ptr() as u64, buf.len() as u64) as usize }
}

/// Open a file for reading
fn open_file_read(path: &str) -> i64 {
    // O_RDONLY = 0
    unsafe {
        syscall3(syscall::SYS_OPEN, path.as_ptr() as u64, path.len() as u64, 0) as i64
    }
}

/// Read from a file descriptor
fn read_fd(fd: i64, buf: &mut [u8]) -> i64 {
    unsafe {
        syscall3(syscall::SYS_READ, fd as u64, buf.as_mut_ptr() as u64, buf.len() as u64) as i64
    }
}

/// Write DOS-style prompt showing current directory (e.g., "C:\>")
fn write_prompt(console: &mut ConsoleManager) {
    static mut CWD_BUF: [u8; 128] = [0u8; 128];

    let len = unsafe { getcwd(&mut CWD_BUF) };
    if len > 0 {
        let cwd = unsafe { &CWD_BUF[..len] };
        if let Ok(s) = core::str::from_utf8(cwd) {
            console.write_str(s);
        }
    } else {
        console.write_str("?");
    }
    console.write_str(">");
}

fn exit(code: i32) -> ! {
    unsafe {
        syscall1(syscall::SYS_EXIT, code as u64);
    }
    loop {}
}

/// Execute a program with the full command line
/// Returns: 0 on success, 1 on exec error, 2 on not found
fn exec_program(cmdline: &str) -> u64 {
    unsafe {
        syscall2(syscall::SYS_EXEC, cmdline.as_ptr() as u64, cmdline.len() as u64)
    }
}

/// Read pending output from kernel console buffer (fd=0)
/// Returns the number of bytes read (0 if empty)
fn read_console_output(buf: &mut [u8]) -> usize {
    unsafe {
        // SYS_READ with fd=0 reads from the console output buffer
        syscall3(syscall::SYS_READ, 0, buf.as_mut_ptr() as u64, buf.len() as u64) as usize
    }
}

// ============================================================================
// Global Allocator (via syscalls)
// ============================================================================

use core::alloc::{GlobalAlloc, Layout};

struct SyscallAllocator;

unsafe impl GlobalAlloc for SyscallAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // Request memory from kernel
        syscall1(syscall::SYS_MALLOC, layout.size() as u64) as *mut u8
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // Return memory to kernel
        let _ = syscall3(syscall::SYS_FREE as u32, ptr as u64, layout.size() as u64, 0);
    }
}

#[global_allocator]
static ALLOCATOR: SyscallAllocator = SyscallAllocator;

// ============================================================================
// Entry Point
// ============================================================================

/// Run startup script from AUTOEXEC.CMD if present
fn run_autoexec(console: &mut ConsoleManager, framebuffer: &mut SimpleFramebuffer) {
    serial_write("[CONSOLE] Checking for AUTOEXEC.CMD...\r\n");

    // Try to open C:/AUTOEXEC.CMD (boot disk is mounted as C:)
    let fd = open_file_read("C:/AUTOEXEC.CMD");
    if fd < 0 {
        serial_write("[CONSOLE] No AUTOEXEC.CMD found\r\n");
        return;
    }

    serial_write("[CONSOLE] Running AUTOEXEC.CMD\r\n");
    console.write_str("Running AUTOEXEC.CMD...\r\n");
    console.render(framebuffer);

    // Read the file content
    static mut AUTOEXEC_BUF: [u8; 1024] = [0u8; 1024];
    let bytes_read = unsafe { read_fd(fd, &mut AUTOEXEC_BUF) };
    close_fd(fd);

    if bytes_read <= 0 {
        serial_write("[CONSOLE] AUTOEXEC.CMD is empty\r\n");
        return;
    }

    let content = unsafe { &AUTOEXEC_BUF[..bytes_read as usize] };

    // Parse and execute each line
    let mut line_start = 0;
    for i in 0..bytes_read as usize {
        if content[i] == b'\n' || content[i] == b'\r' {
            if i > line_start {
                // We have a non-empty line
                let line = &content[line_start..i];
                // Skip carriage return if present
                let line = if line.last() == Some(&b'\r') {
                    &line[..line.len() - 1]
                } else {
                    line
                };

                if !line.is_empty() {
                    if let Ok(cmd) = core::str::from_utf8(line) {
                        let cmd = cmd.trim();
                        if !cmd.is_empty() && !cmd.starts_with('#') {
                            serial_write("[AUTOEXEC] ");
                            serial_write(cmd);
                            serial_write("\r\n");

                            // Echo the command
                            console.write_str(cmd);
                            console.write_str("\r\n");
                            console.render(framebuffer);

                            // Execute the command
                            process_command(console, cmd);

                            // Drain output after command
                            let mut output_buf = [0u8; 256];
                            loop {
                                let bytes = read_console_output(&mut output_buf);
                                if bytes == 0 {
                                    break;
                                }
                                if let Ok(s) = core::str::from_utf8(&output_buf[..bytes]) {
                                    console.write_str(s);
                                } else {
                                    console.write(&output_buf[..bytes]);
                                }
                            }
                            console.render(framebuffer);
                        }
                    }
                }
            }
            line_start = i + 1;
        }
    }

    // Handle last line without newline
    if line_start < bytes_read as usize {
        let line = &content[line_start..bytes_read as usize];
        if let Ok(cmd) = core::str::from_utf8(line) {
            let cmd = cmd.trim();
            if !cmd.is_empty() && !cmd.starts_with('#') {
                serial_write("[AUTOEXEC] ");
                serial_write(cmd);
                serial_write("\r\n");

                console.write_str(cmd);
                console.write_str("\r\n");
                console.render(framebuffer);

                process_command(console, cmd);

                // Drain output
                let mut output_buf = [0u8; 256];
                loop {
                    let bytes = read_console_output(&mut output_buf);
                    if bytes == 0 {
                        break;
                    }
                    if let Ok(s) = core::str::from_utf8(&output_buf[..bytes]) {
                        console.write_str(s);
                    } else {
                        console.write(&output_buf[..bytes]);
                    }
                }
                console.render(framebuffer);
            }
        }
    }

    serial_write("[CONSOLE] AUTOEXEC.CMD complete\r\n");

    // Dump screen content for automated testing
    serial_write("<<<SCREEN_DUMP_START>>>\r\n");
    console.for_each_row(|_row, cells| {
        // Find last non-space character
        let mut last_char = 0;
        for (i, cell) in cells.iter().enumerate() {
            if cell.ch != ' ' {
                last_char = i + 1;
            }
        }
        // Output the row content up to last non-space
        for cell in &cells[..last_char] {
            let mut buf = [0u8; 4];
            let s = cell.ch.encode_utf8(&mut buf);
            serial_write(s);
        }
        serial_write("\r\n");
    });
    serial_write("<<<SCREEN_DUMP_END>>>\r\n");
}

#[no_mangle]
extern "C" fn _start() -> ! {
    serial_write("[CONSOLE] Starting console app\r\n");

    // Get framebuffer info from kernel
    serial_write("[CONSOLE] Getting FB addr...\r\n");
    let fb_address = fb_addr();
    serial_write("[CONSOLE] FB addr done\r\n");
    if fb_address == 0 {
        serial_write("[CONSOLE] ERROR: No framebuffer\r\n");
        exit(1);
    }

    serial_write("[CONSOLE] Getting dimensions...\r\n");
    let (width, height, pitch) = fb_dimensions();
    serial_write("[CONSOLE] Dimensions done\r\n");

    serial_write("[CONSOLE] Creating FB struct...\r\n");
    // Create framebuffer
    let fb_info = FramebufferInfo {
        width,
        height,
        pitch,
        bpp: 32,
        format: PixelFormat::Bgr, // UEFI GOP typically uses BGR
    };

    let mut framebuffer = unsafe {
        SimpleFramebuffer::new(fb_address as *mut u32, fb_info)
    };
    serial_write("[CONSOLE] FB struct done\r\n");

    // Calculate terminal size (8x16 font)
    let cols = (width / 8) as usize;
    let rows = (height / 16) as usize;

    serial_write("[CONSOLE] About to create console manager\r\n");
    serial_write("[CONSOLE] cols=");
    // Print cols and rows as simple hex
    unsafe {
        let mut buf = [0u8; 8];
        let mut n = cols;
        for i in (0..8).rev() {
            let digit = (n & 0xF) as u8;
            buf[i] = if digit < 10 { b'0' + digit } else { b'A' + digit - 10 };
            n >>= 4;
        }
        syscall3(syscall::SYS_WRITE, 0, buf.as_ptr() as u64, 8);
    }
    serial_write(" rows=");
    unsafe {
        let mut buf = [0u8; 8];
        let mut n = rows;
        for i in (0..8).rev() {
            let digit = (n & 0xF) as u8;
            buf[i] = if digit < 10 { b'0' + digit } else { b'A' + digit - 10 };
            n >>= 4;
        }
        syscall3(syscall::SYS_WRITE, 0, buf.as_ptr() as u64, 8);
    }
    serial_write("\r\n");
    serial_write("[CONSOLE] Calling ConsoleManager::new...\r\n");
    // Create console manager with one terminal
    let mut console = ConsoleManager::new(cols, rows);
    serial_write("[CONSOLE] Console manager done\r\n");
    console.init_consoles(1);
    serial_write("[CONSOLE] Consoles initialized\r\n");

    // Display welcome message
    console.write_str("\x1b[2J\x1b[H"); // Clear screen, home cursor
    console.write_str("WATOS Console v0.2-DEBUG\r\n");
    console.write_str("========================\r\n\r\n");
    console.write_str("Type 'help' for commands.\r\n\r\n");

    // Initial render
    console.render(&mut framebuffer);
    serial_write("[CONSOLE] Ready\r\n");

    // Check for AUTOEXEC.CMD startup script
    run_autoexec(&mut console, &mut framebuffer);

    // Show prompt after autoexec
    write_prompt(&mut console);
    console.render(&mut framebuffer);

    // Command buffer
    let mut cmd_buffer = [0u8; 256];
    let mut cmd_len = 0usize;

    // Buffer for reading kernel console output
    let mut console_buf = [0u8; 256];

    // Cursor blink timer (PIT runs at ~18.2 Hz, so ~9 ticks = ~500ms)
    let mut last_blink_tick = get_ticks();
    const BLINK_INTERVAL: u64 = 9; // ~500ms at 18.2 Hz

    // Main loop
    loop {
        // Check if it's time to blink cursor
        let current_tick = get_ticks();
        if current_tick.wrapping_sub(last_blink_tick) >= BLINK_INTERVAL {
            last_blink_tick = current_tick;
            if console.tick() {
                // Only redraw the cursor cell, not the whole screen
                console.render_cursor(&mut framebuffer);
            }
        }
        // Poll for any output from kernel console buffer (from child processes)
        let bytes_read = read_console_output(&mut console_buf);
        if bytes_read > 0 {
            // Process output through terminal emulator and render
            if let Ok(s) = core::str::from_utf8(&console_buf[..bytes_read]) {
                console.write_str(s);
            } else {
                console.write(&console_buf[..bytes_read]);
            }
            console.render(&mut framebuffer);
        }

        // Read keyboard
        let scancode = read_scancode();
        if scancode != 0 {
            // Debug: minimal - just show we got something
            serial_write("[K]");
            if let Some(event) = console.process_scancode(scancode) {
                if event.pressed {
                    // Get character from keyboard
                    if let Some(ch) = console.keyboard().to_char(&event) {
                        match ch {
                            '\r' | '\n' => {
                                // Enter - process command
                                serial_write("[ENTER] cmd_len=");
                                // Print cmd_len as digit
                                if cmd_len < 10 {
                                    let digit = b'0' + cmd_len as u8;
                                    unsafe {
                                        core::arch::asm!(
                                            "int 0x80",
                                            in("eax") 1u32,  // SYS_WRITE
                                            in("rdi") 0u64,
                                            in("rsi") &digit as *const u8 as u64,
                                            in("rdx") 1u64,
                                            options(nostack)
                                        );
                                    }
                                }
                                serial_write("\r\n");
                                console.write_str("\r\n");

                                if cmd_len > 0 {
                                    let cmd = core::str::from_utf8(&cmd_buffer[..cmd_len]).unwrap_or("");
                                    serial_write("[CMD] ");
                                    serial_write(cmd);
                                    serial_write("\r\n");
                                    process_command(&mut console, cmd);
                                    cmd_len = 0;

                                    // Drain any pending output from the command before showing prompt
                                    loop {
                                        let bytes = read_console_output(&mut console_buf);
                                        if bytes == 0 {
                                            break;
                                        }
                                        if let Ok(s) = core::str::from_utf8(&console_buf[..bytes]) {
                                            console.write_str(s);
                                        } else {
                                            console.write(&console_buf[..bytes]);
                                        }
                                    }
                                    console.render(&mut framebuffer);
                                }

                                write_prompt(&mut console);
                            }
                            '\x08' => {
                                // Backspace
                                if cmd_len > 0 {
                                    cmd_len -= 1;
                                    console.write_str("\x08 \x08"); // Move back, space, move back
                                }
                            }
                            '\x7f' => {
                                // Delete (treat like backspace)
                                if cmd_len > 0 {
                                    cmd_len -= 1;
                                    console.write_str("\x08 \x08");
                                }
                            }
                            _ => {
                                // Regular character
                                if cmd_len < cmd_buffer.len() - 1 {
                                    cmd_buffer[cmd_len] = ch as u8;
                                    cmd_len += 1;

                                    // Echo character
                                    let mut buf = [0u8; 4];
                                    let s = ch.encode_utf8(&mut buf);
                                    console.write_str(s);
                                }
                            }
                        }
                    } else {
                        // Handle special keys
                        match event.key {
                            KeyCode::Up => console.write_str("\x1b[A"),
                            KeyCode::Down => console.write_str("\x1b[B"),
                            KeyCode::Right => console.write_str("\x1b[C"),
                            KeyCode::Left => console.write_str("\x1b[D"),
                            KeyCode::Home => console.write_str("\x1b[H"),
                            KeyCode::End => console.write_str("\x1b[F"),
                            KeyCode::PageUp => console.write_str("\x1b[5~"),
                            KeyCode::PageDown => console.write_str("\x1b[6~"),
                            _ => {}
                        }
                    }
                }

                // Render after input
                console.render(&mut framebuffer);
            }
        }
    }
}

/// Redirection info parsed from command line
struct Redirection<'a> {
    cmd: &'a str,           // Command without redirection
    output_file: Option<&'a str>,  // > or >> target
    append: bool,           // true if >>
    input_file: Option<&'a str>,   // < source
}

/// Parse redirection operators from command line
fn parse_redirections(cmd: &str) -> Redirection<'_> {
    let mut result = Redirection {
        cmd: cmd,
        output_file: None,
        append: false,
        input_file: None,
    };

    // Look for >> first (must check before >)
    if let Some(pos) = cmd.find(">>") {
        result.cmd = cmd[..pos].trim();
        let file = cmd[pos + 2..].trim();
        // Remove any subsequent redirections from filename
        let file = file.split('<').next().unwrap_or(file).trim();
        if !file.is_empty() {
            result.output_file = Some(file);
            result.append = true;
        }
    } else if let Some(pos) = cmd.find('>') {
        result.cmd = cmd[..pos].trim();
        let file = cmd[pos + 1..].trim();
        let file = file.split('<').next().unwrap_or(file).trim();
        if !file.is_empty() {
            result.output_file = Some(file);
            result.append = false;
        }
    }

    // Look for input redirection
    if let Some(pos) = result.cmd.find('<') {
        let before = result.cmd[..pos].trim();
        let file = result.cmd[pos + 1..].trim();
        // Remove any output redirection from input filename
        let file = file.split('>').next().unwrap_or(file).trim();
        if !file.is_empty() {
            result.input_file = Some(file);
        }
        result.cmd = before;
    }

    result
}

/// Open a file for writing (used for redirection)
fn open_file_write(path: &str, append: bool) -> i64 {
    // O_WRONLY | O_CREAT, plus O_APPEND or O_TRUNC
    let flags: u32 = if append {
        0x01 | 0x40 | 0x400  // O_WRONLY | O_CREAT | O_APPEND
    } else {
        0x01 | 0x40 | 0x200  // O_WRONLY | O_CREAT | O_TRUNC
    };
    unsafe {
        syscall3(syscall::SYS_OPEN, path.as_ptr() as u64, path.len() as u64, flags as u64) as i64
    }
}

/// Write to a file descriptor
fn write_fd(fd: i64, data: &[u8]) -> i64 {
    unsafe {
        syscall3(syscall::SYS_WRITE, fd as u64, data.as_ptr() as u64, data.len() as u64) as i64
    }
}

/// Close a file descriptor
fn close_fd(fd: i64) {
    unsafe {
        syscall2(syscall::SYS_CLOSE, fd as u64, 0);
    }
}

/// Change directory via syscall
fn chdir(path: &[u8]) -> u64 {
    unsafe {
        syscall2(syscall::SYS_CHDIR, path.as_ptr() as u64, path.len() as u64)
    }
}

/// Check if command is a drive letter (e.g., "C:" or "d:")
fn is_drive_letter(cmd: &str) -> bool {
    let bytes = cmd.as_bytes();
    bytes.len() == 2
        && bytes[1] == b':'
        && ((bytes[0] >= b'A' && bytes[0] <= b'Z') || (bytes[0] >= b'a' && bytes[0] <= b'z'))
}

/// Process a command
fn process_command(console: &mut ConsoleManager, cmd: &str) {
    serial_write("!!!PROC_CMD_START!!!\r\n");
    let cmd = cmd.trim();

    // Check for drive letter change (DOS-style: just type "C:" to change drive)
    if is_drive_letter(cmd) {
        serial_write("[CONSOLE] Drive change: ");
        serial_write(cmd);
        serial_write("\r\n");

        let result = chdir(cmd.as_bytes());
        if result != 0 {
            console.write_str("Invalid drive: ");
            console.write_str(cmd);
            console.write_str("\r\n");
        }
        return;
    }

    // Parse redirections
    let redir = parse_redirections(cmd);
    serial_write("!!!REDIR:");
    serial_write(if redir.output_file.is_some() { "YES" } else { "NO" });
    serial_write("!!!\r\n");
    let cmd = redir.cmd;

    // Parse command and arguments
    let (program, _args) = match cmd.find(' ') {
        Some(pos) => (&cmd[..pos], &cmd[pos + 1..]),
        None => (cmd, ""),
    };

    match program {
        "help" => {
            serial_write("[CONSOLE] Builtin: help\r\n");
            console.write_str("Built-in commands:\r\n");
            console.write_str("  help    - Show this help\r\n");
            console.write_str("  clear   - Clear screen\r\n");
            console.write_str("  colors  - Test colors\r\n");
            console.write_str("  ver     - Show version\r\n");
            console.write_str("  set     - Show environment variables\r\n");
            console.write_str("\r\nDrive navigation:\r\n");
            console.write_str("  C:, D:  - Change to drive\r\n");
            console.write_str("\r\nRedirection:\r\n");
            console.write_str("  cmd > file   - Write output to file\r\n");
            console.write_str("  cmd >> file  - Append output to file\r\n");
            console.write_str("\r\nExternal programs:\r\n");
            console.write_str("  ls, cat, cp, mv, rm, touch, mkdir\r\n");
            console.write_str("  echo, date, pwd, cd, df, ln, mkfifo\r\n");
            console.write_str("  ps, uptime, uname, drives, clear\r\n");
            console.write_str("  lsblk, mount, shutdown, reboot\r\n");
        }
        "clear" | "cls" => {
            console.write_str("\x1b[2J\x1b[H");
        }
        "colors" => {
            console.write_str("Color test:\r\n");
            // 16 standard colors
            for i in 0..16 {
                let code = if i < 8 { 30 + i } else { 90 + i - 8 };
                console.write_str("\x1b[");
                write_num(console, code);
                console.write_str("m*\x1b[0m");
            }
            console.write_str("\r\n");
            // Background colors
            for i in 0..16 {
                let code = if i < 8 { 40 + i } else { 100 + i - 8 };
                console.write_str("\x1b[");
                write_num(console, code);
                console.write_str("m \x1b[0m");
            }
            console.write_str("\r\n");
        }
        "ver" | "version" => {
            console.write_str("WATOS Console v0.1\r\n");
            console.write_str("Terminal: watos-terminal crate\r\n");
        }
        "set" | "env" | "export" => {
            // Show environment variables
            static mut CWD_BUF: [u8; 128] = [0u8; 128];
            let len = unsafe { getcwd(&mut CWD_BUF) };

            console.write_str("CWD=");
            if len > 0 {
                let cwd = unsafe { &CWD_BUF[..len] };
                if let Ok(s) = core::str::from_utf8(cwd) {
                    console.write_str(s);
                }
            }
            console.write_str("\r\n");
            console.write_str("PATH=/apps/system\r\n");
        }
        "" => {}
        _ => {
            // Try to execute as external program with full command line
            serial_write("[CONSOLE] Executing: ");
            serial_write(cmd);
            serial_write("\r\n");

            // Open output file if redirection requested
            let output_fd: Option<i64> = if let Some(file) = redir.output_file {
                let fd = open_file_write(file, redir.append);
                if fd < 0 {
                    console.write_str("Error: Cannot open '");
                    console.write_str(file);
                    console.write_str("' for writing\r\n");
                    return;
                }
                Some(fd)
            } else {
                None
            };

            // Pass full command line (program + args) to exec
            serial_write("!!!BEFORE_EXEC!!!\r\n");
            let result = exec_program(cmd);
            serial_write("!!!AFTER_EXEC!!!\r\n");

            // Debug: confirm we returned from exec
            serial_write("[CONSOLE] exec returned: ");
            if result == 0 {
                serial_write("success\r\n");
            } else if result == 1 {
                serial_write("error\r\n");
            } else if result == 2 {
                serial_write("not found\r\n");
            } else {
                serial_write("unknown\r\n");
            }

            // Reset keyboard state after child process returns
            // This prevents corruption from key events that occurred during child execution
            console.keyboard_mut().reset();

            // Drain any stale scancodes from the buffer
            while read_scancode() != 0 {}
            serial_write("[CONSOLE] Keyboard state reset\r\n");

            // Handle output - either redirect to file or display on console
            serial_write("!!!CHECK_OUTPUT_FD=");
            serial_write(if output_fd.is_some() { "SOME" } else { "NONE" });
            serial_write("!!!\r\n");
            let mut output_buf = [0u8; 256];
            if let Some(fd) = output_fd {
                serial_write("!!!TAKING_SOME_BRANCH!!!\r\n");
                // Redirect output to file
                loop {
                    let bytes = read_console_output(&mut output_buf);
                    if bytes == 0 {
                        break;
                    }
                    // Write to file instead of console
                    write_fd(fd, &output_buf[..bytes]);
                }
                close_fd(fd);
            } else {
                // No redirection - display output on console
                serial_write("None\r\n");
                serial_write("[CONSOLE] Draining output buffer...\r\n");
                loop {
                    let bytes = read_console_output(&mut output_buf);
                    serial_write("[CONSOLE] drain got ");
                    // Print bytes count
                    if bytes < 10 {
                        let digit = b'0' + bytes as u8;
                        unsafe {
                            syscall3(syscall::SYS_WRITE, 0, &digit as *const u8 as u64, 1);
                        }
                    } else {
                        serial_write("10+");
                    }
                    serial_write(" bytes\r\n");
                    if bytes == 0 {
                        break;
                    }
                    // Write to console display
                    if let Ok(s) = core::str::from_utf8(&output_buf[..bytes]) {
                        console.write_str(s);
                    } else {
                        console.write(&output_buf[..bytes]);
                    }
                }
                serial_write("[CONSOLE] Done draining\r\n");
            }

            match result {
                0 => {
                    // Success - program ran
                }
                1 => {
                    console.write_str("Error: Failed to execute '");
                    console.write_str(program);
                    console.write_str("'\r\n");
                }
                2 => {
                    console.write_str("Command not found: ");
                    console.write_str(program);
                    console.write_str("\r\n");
                }
                _ => {
                    console.write_str("Unknown error executing: ");
                    console.write_str(program);
                    console.write_str("\r\n");
                }
            }
        }
    }
}

/// Write a number to console
fn write_num(console: &mut ConsoleManager, n: u32) {
    if n >= 10 {
        write_num(console, n / 10);
    }
    let digit = (n % 10) as u8 + b'0';
    console.write(&[digit]);
}

/// Write hex to serial
fn serial_hex(val: u64) {
    let hex = b"0123456789ABCDEF";
    let mut buf = [0u8; 16];
    let mut i = 15;
    let mut v = val;

    if v == 0 {
        serial_write("0");
        return;
    }

    while v > 0 {
        buf[i] = hex[(v & 0xF) as usize];
        v >>= 4;
        if i > 0 {
            i -= 1;
        }
    }

    if let Ok(s) = core::str::from_utf8(&buf[i + 1..]) {
        serial_write(s);
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    serial_write("\r\n!!! CONSOLE PANIC !!!\r\n");
    exit(1);
}
