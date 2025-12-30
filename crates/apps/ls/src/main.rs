//! WATOS ls command - list directory contents
//!
//! Usage: ls [OPTIONS] [PATH]
//!
//! Options:
//!   -l    Long format (detailed listing)
//!   -a    Show hidden files (starting with .)
//!   -1    One entry per line
//!   -h    Human-readable sizes (with -l)
//!   -R    Recursive listing
//!   -F    Append indicator (/ for dirs, * for executables)
//!   -C    Use colors (default on)
//!   --no-color  Disable colors

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use watos_syscall::numbers as syscall;

// ============================================================================
// Syscall wrappers
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

fn write_str(s: &str) {
    unsafe {
        syscall3(syscall::SYS_WRITE, 1, s.as_ptr() as u64, s.len() as u64);
    }
}

fn write_bytes(b: &[u8]) {
    unsafe {
        syscall3(syscall::SYS_WRITE, 1, b.as_ptr() as u64, b.len() as u64);
    }
}

fn exit(code: i32) -> ! {
    unsafe {
        let _: u64;
        core::arch::asm!(
            "int 0x80",
            in("eax") syscall::SYS_EXIT,
            in("rdi") code as u64,
            lateout("rax") _,
            options(nostack)
        );
    }
    loop {}
}

fn get_args(buf: &mut [u8]) -> usize {
    unsafe { syscall2(syscall::SYS_GETARGS, buf.as_mut_ptr() as u64, buf.len() as u64) as usize }
}

fn readdir(path: &[u8], buf: &mut [u8]) -> usize {
    unsafe {
        syscall3(
            syscall::SYS_READDIR,
            path.as_ptr() as u64,
            path.len() as u64,
            buf.as_mut_ptr() as u64,
        ) as usize
    }
}

fn getcwd(buf: &mut [u8]) -> usize {
    unsafe { syscall2(syscall::SYS_GETCWD, buf.as_mut_ptr() as u64, buf.len() as u64) as usize }
}

// ============================================================================
// ANSI color codes
// ============================================================================

const COLOR_RESET: &str = "\x1b[0m";
const COLOR_BLUE: &str = "\x1b[34m";        // Directories
const COLOR_CYAN: &str = "\x1b[36m";        // Symlinks
const COLOR_GREEN: &str = "\x1b[32m";       // Executables
const COLOR_RED: &str = "\x1b[31m";         // Archives
const COLOR_MAGENTA: &str = "\x1b[35m";     // Images/media
const COLOR_YELLOW: &str = "\x1b[33m";      // Devices
const COLOR_BOLD: &str = "\x1b[1m";

// ============================================================================
// Options
// ============================================================================

struct Options {
    long_format: bool,      // -l
    show_hidden: bool,      // -a
    one_per_line: bool,     // -1
    human_readable: bool,   // -h
    recursive: bool,        // -R
    show_indicator: bool,   // -F
    use_colors: bool,       // -C (default on)
}

impl Options {
    fn new() -> Self {
        Options {
            long_format: false,
            show_hidden: false,
            one_per_line: false,
            human_readable: false,
            recursive: false,
            show_indicator: false,
            use_colors: true,
        }
    }
}

// ============================================================================
// Entry types and parsing
// ============================================================================

#[derive(Clone, Copy, PartialEq)]
enum EntryType {
    File,
    Directory,
    Symlink,
    Device,
    Pipe,
    Socket,
    Unknown,
}

struct DirEntry<'a> {
    name: &'a [u8],
    size: u64,
    entry_type: EntryType,
}

fn parse_entry_type(c: u8) -> EntryType {
    match c {
        b'F' => EntryType::File,
        b'D' => EntryType::Directory,
        b'L' => EntryType::Symlink,
        b'C' | b'B' => EntryType::Device,
        b'P' => EntryType::Pipe,
        b'S' => EntryType::Socket,
        _ => EntryType::Unknown,
    }
}

fn get_extension(name: &[u8]) -> Option<&[u8]> {
    let mut dot_pos = None;
    for (i, &c) in name.iter().enumerate().rev() {
        if c == b'.' {
            dot_pos = Some(i);
            break;
        }
    }
    dot_pos.map(|pos| &name[pos + 1..])
}

fn is_hidden(name: &[u8]) -> bool {
    !name.is_empty() && name[0] == b'.'
}

fn is_executable_ext(ext: &[u8]) -> bool {
    matches!(ext, b"exe" | b"EXE" | b"bin" | b"BIN" | b"sh" | b"SH" | b"com" | b"COM")
}

fn is_archive_ext(ext: &[u8]) -> bool {
    matches!(ext, b"zip" | b"ZIP" | b"tar" | b"TAR" | b"gz" | b"GZ" |
                  b"bz2" | b"BZ2" | b"xz" | b"XZ" | b"7z" | b"rar" | b"RAR")
}

fn is_media_ext(ext: &[u8]) -> bool {
    matches!(ext, b"png" | b"PNG" | b"jpg" | b"JPG" | b"jpeg" | b"JPEG" |
                  b"gif" | b"GIF" | b"bmp" | b"BMP" | b"mp3" | b"MP3" |
                  b"mp4" | b"MP4" | b"wav" | b"WAV" | b"mkv" | b"MKV" |
                  b"avi" | b"AVI" | b"mov" | b"MOV")
}

// ============================================================================
// Formatting helpers
// ============================================================================

fn format_size_human(size: u64, buf: &mut [u8]) -> usize {
    const UNITS: &[u8] = b"BKMGTPE";
    let mut s = size;
    let mut unit_idx = 0;

    while s >= 1024 && unit_idx < UNITS.len() - 1 {
        s /= 1024;
        unit_idx += 1;
    }

    let len = format_u64(s, buf);
    if unit_idx > 0 && len < buf.len() - 1 {
        buf[len] = UNITS[unit_idx];
        len + 1
    } else {
        len
    }
}

fn format_u64(mut n: u64, buf: &mut [u8]) -> usize {
    if n == 0 {
        buf[0] = b'0';
        return 1;
    }

    let mut tmp = [0u8; 20];
    let mut i = 0;
    while n > 0 {
        tmp[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }

    // Reverse into buf
    for j in 0..i {
        buf[j] = tmp[i - 1 - j];
    }
    i
}

fn write_padded_right(s: &[u8], width: usize) {
    let padding = if s.len() < width { width - s.len() } else { 0 };
    for _ in 0..padding {
        write_str(" ");
    }
    write_bytes(s);
}

fn write_padded_left(s: &[u8], width: usize) {
    write_bytes(s);
    let padding = if s.len() < width { width - s.len() } else { 0 };
    for _ in 0..padding {
        write_str(" ");
    }
}

// ============================================================================
// Entry display
// ============================================================================

fn get_color_for_entry(entry: &DirEntry, opts: &Options) -> &'static str {
    if !opts.use_colors {
        return "";
    }

    match entry.entry_type {
        EntryType::Directory => COLOR_BLUE,
        EntryType::Symlink => COLOR_CYAN,
        EntryType::Device => COLOR_YELLOW,
        EntryType::Pipe | EntryType::Socket => COLOR_YELLOW,
        EntryType::File => {
            if let Some(ext) = get_extension(entry.name) {
                if is_executable_ext(ext) {
                    COLOR_GREEN
                } else if is_archive_ext(ext) {
                    COLOR_RED
                } else if is_media_ext(ext) {
                    COLOR_MAGENTA
                } else {
                    ""
                }
            } else {
                ""
            }
        }
        EntryType::Unknown => "",
    }
}

fn get_indicator(entry: &DirEntry) -> &'static str {
    match entry.entry_type {
        EntryType::Directory => "/",
        EntryType::Symlink => "@",
        EntryType::Pipe => "|",
        EntryType::Socket => "=",
        EntryType::File => {
            if let Some(ext) = get_extension(entry.name) {
                if is_executable_ext(ext) {
                    "*"
                } else {
                    ""
                }
            } else {
                ""
            }
        }
        _ => "",
    }
}

fn type_char(t: EntryType) -> u8 {
    match t {
        EntryType::File => b'-',
        EntryType::Directory => b'd',
        EntryType::Symlink => b'l',
        EntryType::Device => b'c',
        EntryType::Pipe => b'p',
        EntryType::Socket => b's',
        EntryType::Unknown => b'?',
    }
}

fn display_entry_long(entry: &DirEntry, opts: &Options) {
    // Type indicator
    let tc = [type_char(entry.entry_type)];
    write_bytes(&tc);

    // Permissions (placeholder - we don't have real perms yet)
    if entry.entry_type == EntryType::Directory {
        write_str("rwxr-xr-x");
    } else {
        write_str("rw-r--r--");
    }
    write_str(" ");

    // Size
    let mut size_buf = [0u8; 16];
    let size_len = if opts.human_readable {
        format_size_human(entry.size, &mut size_buf)
    } else {
        format_u64(entry.size, &mut size_buf)
    };
    write_padded_right(&size_buf[..size_len], 10);
    write_str(" ");

    // Name with color
    let color = get_color_for_entry(entry, opts);
    if !color.is_empty() {
        write_str(color);
    }
    write_bytes(entry.name);
    if opts.show_indicator {
        write_str(get_indicator(entry));
    }
    if !color.is_empty() {
        write_str(COLOR_RESET);
    }

    write_str("\r\n");
}

fn display_entry_short(entry: &DirEntry, opts: &Options) {
    let color = get_color_for_entry(entry, opts);
    if !color.is_empty() {
        write_str(color);
    }
    write_bytes(entry.name);
    if opts.show_indicator {
        write_str(get_indicator(entry));
    }
    if !color.is_empty() {
        write_str(COLOR_RESET);
    }

    if opts.one_per_line || opts.long_format {
        write_str("\r\n");
    } else {
        // Pad to column width for multi-column display
        let name_len = entry.name.len() + if opts.show_indicator { 1 } else { 0 };
        let col_width = 16;
        let padding = if name_len < col_width { col_width - name_len } else { 2 };
        for _ in 0..padding {
            write_str(" ");
        }
    }
}

// ============================================================================
// Main entry parsing and display
// ============================================================================

fn parse_and_display_entries(entries: &[u8], opts: &Options) -> usize {
    let mut count = 0;
    let mut line_start = 0;
    let mut column = 0;
    const COLS_PER_LINE: usize = 4;

    for i in 0..entries.len() {
        if entries[i] == b'\n' {
            let line = &entries[line_start..i];
            if line.len() >= 3 {
                let entry_type = parse_entry_type(line[0]);

                // Find name and size
                let name_start = 2;
                let mut name_end = name_start;
                while name_end < line.len() && line[name_end] != b' ' {
                    name_end += 1;
                }
                let name = &line[name_start..name_end];

                // Skip hidden files if not showing them
                if !opts.show_hidden && is_hidden(name) {
                    line_start = i + 1;
                    continue;
                }

                // Parse size
                let size_start = if name_end + 1 < line.len() { name_end + 1 } else { name_end };
                let size_bytes = &line[size_start..];
                let mut size: u64 = 0;
                for &c in size_bytes {
                    if c >= b'0' && c <= b'9' {
                        size = size * 10 + (c - b'0') as u64;
                    }
                }

                let entry = DirEntry {
                    name,
                    size,
                    entry_type,
                };

                if opts.long_format {
                    display_entry_long(&entry, opts);
                } else {
                    display_entry_short(&entry, opts);
                    if !opts.one_per_line {
                        column += 1;
                        if column >= COLS_PER_LINE {
                            write_str("\r\n");
                            column = 0;
                        }
                    }
                }

                count += 1;
            }
            line_start = i + 1;
        }
    }

    // Final newline if we ended mid-row in multi-column mode
    if !opts.long_format && !opts.one_per_line && column > 0 {
        write_str("\r\n");
    }

    count
}

// ============================================================================
// Argument parsing
// ============================================================================

fn parse_options(args: &[u8]) -> (Options, usize) {
    let mut opts = Options::new();
    let mut i = 0;

    // Skip command name
    while i < args.len() && args[i] != b' ' {
        i += 1;
    }
    // Skip space
    while i < args.len() && args[i] == b' ' {
        i += 1;
    }

    // Parse options
    while i < args.len() {
        if args[i] == b'-' {
            i += 1;
            if i < args.len() && args[i] == b'-' {
                // Long option
                i += 1;
                let opt_start = i;
                while i < args.len() && args[i] != b' ' {
                    i += 1;
                }
                let opt = &args[opt_start..i];
                if opt == b"no-color" {
                    opts.use_colors = false;
                } else if opt == b"color" {
                    opts.use_colors = true;
                } else if opt == b"all" {
                    opts.show_hidden = true;
                } else if opt == b"human-readable" {
                    opts.human_readable = true;
                } else if opt == b"recursive" {
                    opts.recursive = true;
                }
            } else {
                // Short options (can be combined: -la)
                while i < args.len() && args[i] != b' ' {
                    match args[i] {
                        b'l' => opts.long_format = true,
                        b'a' => opts.show_hidden = true,
                        b'1' => opts.one_per_line = true,
                        b'h' => opts.human_readable = true,
                        b'R' => opts.recursive = true,
                        b'F' => opts.show_indicator = true,
                        b'C' => opts.use_colors = true,
                        _ => {}
                    }
                    i += 1;
                }
            }
            // Skip trailing space
            while i < args.len() && args[i] == b' ' {
                i += 1;
            }
        } else {
            // Not an option, must be path - return current position
            return (opts, i);
        }
    }

    // No path given - return position past end of args
    (opts, args.len())
}

// ============================================================================
// Main
// ============================================================================

#[no_mangle]
extern "C" fn _start() -> ! {
    static mut ARGS_BUF: [u8; 256] = [0u8; 256];
    static mut DIR_BUF: [u8; 4096] = [0u8; 4096];
    static mut CWD_BUF: [u8; 256] = [0u8; 256];

    let args_len = unsafe { get_args(&mut ARGS_BUF) };
    let args = unsafe { &ARGS_BUF[..args_len] };

    let (opts, path_start) = parse_options(args);

    // Get path (rest of args after options)
    let path = if path_start < args_len {
        &args[path_start..]
    } else {
        &[]
    };

    // Read directory
    let len = unsafe { readdir(path, &mut DIR_BUF) };

    if len == 0 {
        // Try to get current directory for error message
        let cwd_len = unsafe { getcwd(&mut CWD_BUF) };
        if path.is_empty() && cwd_len > 0 {
            let cwd = unsafe { &CWD_BUF[..cwd_len] };
            write_bytes(cwd);
            write_str(": ");
        } else if !path.is_empty() {
            write_bytes(path);
            write_str(": ");
        }
        write_str("(empty or not found)\r\n");
        exit(0);
    }

    let entries = unsafe { &DIR_BUF[..len] };
    let count = parse_and_display_entries(entries, &opts);

    if opts.long_format {
        // Print total count
        let mut count_buf = [0u8; 16];
        let count_len = format_u64(count as u64, &mut count_buf);
        write_str("total ");
        write_bytes(&count_buf[..count_len]);
        write_str("\r\n");
    }

    exit(0);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    exit(1);
}
