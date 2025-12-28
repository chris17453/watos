//! Extended file metadata for modern file operations
//!
//! This module provides extended metadata beyond traditional Unix file attributes.
//! These features are optional - filesystems that don't support them return defaults.
//!
//! # Features
//!
//! - **Description**: Human-readable file description
//! - **Tags**: Categorization tags for organization
//! - **Color**: Display color hint for terminal/GUI
//! - **Icon**: Icon identifier for GUI display
//! - **Custom attributes**: Key-value pairs for extensibility
//!
//! # Filesystem Support
//!
//! | Filesystem | Extended Metadata |
//! |------------|-------------------|
//! | WFS        | Full support      |
//! | FAT        | Not supported     |
//! | DevFS      | Static metadata   |
//! | ProcFS     | Dynamic metadata  |

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// Display color for files
///
/// Used by `ls` and file managers for visual distinction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FileColor {
    /// Default terminal color
    Default = 0,
    /// Black
    Black = 30,
    /// Red (executables, archives)
    Red = 31,
    /// Green (executables)
    Green = 32,
    /// Yellow (device files)
    Yellow = 33,
    /// Blue (directories)
    Blue = 34,
    /// Magenta (images, media)
    Magenta = 35,
    /// Cyan (symlinks)
    Cyan = 36,
    /// White
    White = 37,
    /// Bright Black (gray)
    BrightBlack = 90,
    /// Bright Red
    BrightRed = 91,
    /// Bright Green
    BrightGreen = 92,
    /// Bright Yellow
    BrightYellow = 93,
    /// Bright Blue
    BrightBlue = 94,
    /// Bright Magenta
    BrightMagenta = 95,
    /// Bright Cyan
    BrightCyan = 96,
    /// Bright White
    BrightWhite = 97,
}

impl Default for FileColor {
    fn default() -> Self {
        FileColor::Default
    }
}

impl FileColor {
    /// Get ANSI escape code for this color
    pub fn ansi_code(&self) -> &'static str {
        match self {
            FileColor::Default => "\x1b[0m",
            FileColor::Black => "\x1b[30m",
            FileColor::Red => "\x1b[31m",
            FileColor::Green => "\x1b[32m",
            FileColor::Yellow => "\x1b[33m",
            FileColor::Blue => "\x1b[34m",
            FileColor::Magenta => "\x1b[35m",
            FileColor::Cyan => "\x1b[36m",
            FileColor::White => "\x1b[37m",
            FileColor::BrightBlack => "\x1b[90m",
            FileColor::BrightRed => "\x1b[91m",
            FileColor::BrightGreen => "\x1b[92m",
            FileColor::BrightYellow => "\x1b[93m",
            FileColor::BrightBlue => "\x1b[94m",
            FileColor::BrightMagenta => "\x1b[95m",
            FileColor::BrightCyan => "\x1b[96m",
            FileColor::BrightWhite => "\x1b[97m",
        }
    }

    /// ANSI reset code
    pub fn reset() -> &'static str {
        "\x1b[0m"
    }
}

/// Icon identifier for files
///
/// Maps to font icons (like Nerd Fonts) or emojis for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum FileIcon {
    /// No specific icon
    None = 0,
    /// Generic file
    File = 1,
    /// Directory/folder
    Folder = 2,
    /// Symbolic link
    Link = 3,
    /// Executable
    Executable = 4,
    /// Text document
    Text = 5,
    /// Source code
    Code = 6,
    /// Image
    Image = 7,
    /// Audio
    Audio = 8,
    /// Video
    Video = 9,
    /// Archive (zip, tar, etc.)
    Archive = 10,
    /// PDF document
    Pdf = 11,
    /// Configuration file
    Config = 12,
    /// Database
    Database = 13,
    /// Binary/object file
    Binary = 14,
    /// Lock file
    Lock = 15,
    /// Git-related
    Git = 16,
    /// Documentation/README
    Doc = 17,
    /// Key/certificate
    Key = 18,
    /// Device file
    Device = 19,
    /// Pipe/FIFO
    Pipe = 20,
    /// Socket
    Socket = 21,
}

impl Default for FileIcon {
    fn default() -> Self {
        FileIcon::None
    }
}

impl FileIcon {
    /// Get emoji representation
    pub fn emoji(&self) -> &'static str {
        match self {
            FileIcon::None => "",
            FileIcon::File => "📄",
            FileIcon::Folder => "📁",
            FileIcon::Link => "🔗",
            FileIcon::Executable => "⚙️",
            FileIcon::Text => "📝",
            FileIcon::Code => "💻",
            FileIcon::Image => "🖼️",
            FileIcon::Audio => "🎵",
            FileIcon::Video => "🎬",
            FileIcon::Archive => "📦",
            FileIcon::Pdf => "📕",
            FileIcon::Config => "⚙️",
            FileIcon::Database => "🗃️",
            FileIcon::Binary => "📀",
            FileIcon::Lock => "🔒",
            FileIcon::Git => "🌿",
            FileIcon::Doc => "📖",
            FileIcon::Key => "🔑",
            FileIcon::Device => "💾",
            FileIcon::Pipe => "🔀",
            FileIcon::Socket => "🔌",
        }
    }

    /// Get Nerd Font icon (for compatible terminals)
    pub fn nerd_font(&self) -> &'static str {
        match self {
            FileIcon::None => "",
            FileIcon::File => "\u{f15b}",      //
            FileIcon::Folder => "\u{f07b}",    //
            FileIcon::Link => "\u{f0c1}",      //
            FileIcon::Executable => "\u{f085}", //
            FileIcon::Text => "\u{f15c}",      //
            FileIcon::Code => "\u{f121}",      //
            FileIcon::Image => "\u{f1c5}",     //
            FileIcon::Audio => "\u{f001}",     //
            FileIcon::Video => "\u{f03d}",     //
            FileIcon::Archive => "\u{f1c6}",   //
            FileIcon::Pdf => "\u{f1c1}",       //
            FileIcon::Config => "\u{f013}",    //
            FileIcon::Database => "\u{f1c0}",  //
            FileIcon::Binary => "\u{f471}",    //
            FileIcon::Lock => "\u{f023}",      //
            FileIcon::Git => "\u{f1d3}",       //
            FileIcon::Doc => "\u{f02d}",       //
            FileIcon::Key => "\u{f084}",       //
            FileIcon::Device => "\u{f0a0}",    //
            FileIcon::Pipe => "\u{f074}",      //
            FileIcon::Socket => "\u{f1e6}",    //
        }
    }
}

/// Extended file metadata
///
/// Additional metadata beyond standard Unix file attributes.
/// Filesystems that don't support extended metadata return default values.
#[derive(Debug, Clone, Default)]
pub struct ExtendedMetadata {
    /// Human-readable description of the file
    pub description: Option<String>,

    /// Categorization tags
    pub tags: Vec<String>,

    /// Display color hint
    pub color: FileColor,

    /// Icon identifier
    pub icon: FileIcon,

    /// Custom key-value attributes
    pub attributes: BTreeMap<String, String>,

    /// MIME type (if known)
    pub mime_type: Option<String>,

    /// Comment/notes
    pub comment: Option<String>,

    /// Author/creator
    pub author: Option<String>,

    /// Application that created this file
    pub creator_app: Option<String>,

    /// Rating (0-5 stars, 0 = unrated)
    pub rating: u8,

    /// Hidden from normal directory listings
    pub hidden: bool,

    /// System file (important, don't modify)
    pub system: bool,

    /// Archived/backed up
    pub archived: bool,
}

impl ExtendedMetadata {
    /// Create empty extended metadata
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with a description
    pub fn with_description(desc: &str) -> Self {
        ExtendedMetadata {
            description: Some(String::from(desc)),
            ..Default::default()
        }
    }

    /// Add a tag
    pub fn add_tag(&mut self, tag: &str) {
        self.tags.push(String::from(tag));
    }

    /// Set an attribute
    pub fn set_attr(&mut self, key: &str, value: &str) {
        self.attributes.insert(String::from(key), String::from(value));
    }

    /// Get an attribute
    pub fn get_attr(&self, key: &str) -> Option<&String> {
        self.attributes.get(key)
    }

    /// Check if file has any extended metadata
    pub fn has_metadata(&self) -> bool {
        self.description.is_some()
            || !self.tags.is_empty()
            || self.color != FileColor::Default
            || self.icon != FileIcon::None
            || !self.attributes.is_empty()
            || self.mime_type.is_some()
            || self.comment.is_some()
            || self.author.is_some()
            || self.creator_app.is_some()
            || self.rating > 0
            || self.hidden
            || self.system
            || self.archived
    }
}

/// Trait for filesystems that support extended metadata
pub trait ExtendedMetadataFs {
    /// Get extended metadata for a file
    fn get_extended_metadata(&self, path: &str) -> Option<ExtendedMetadata>;

    /// Set extended metadata for a file
    fn set_extended_metadata(&self, path: &str, meta: &ExtendedMetadata) -> crate::VfsResult<()>;

    /// Check if extended metadata is supported for this path
    fn supports_extended_metadata(&self, path: &str) -> bool;
}

/// Infer icon from file extension
pub fn icon_from_extension(ext: &str) -> FileIcon {
    match ext.to_lowercase().as_str() {
        // Text
        "txt" | "md" | "rst" | "org" => FileIcon::Text,

        // Code
        "rs" | "c" | "cpp" | "h" | "hpp" | "py" | "js" | "ts" | "go" | "java" |
        "rb" | "php" | "swift" | "kt" | "scala" | "hs" | "ml" | "ex" | "exs" |
        "sh" | "bash" | "zsh" | "fish" | "ps1" | "bat" | "cmd" => FileIcon::Code,

        // Config
        "json" | "yaml" | "yml" | "toml" | "ini" | "cfg" | "conf" | "xml" |
        "env" | "properties" => FileIcon::Config,

        // Images
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "ico" | "svg" | "webp" |
        "tiff" | "psd" | "ai" | "eps" => FileIcon::Image,

        // Audio
        "mp3" | "wav" | "ogg" | "flac" | "aac" | "m4a" | "wma" | "opus" => FileIcon::Audio,

        // Video
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" => FileIcon::Video,

        // Archives
        "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" | "tgz" | "tbz2" => FileIcon::Archive,

        // Documents
        "pdf" => FileIcon::Pdf,
        "doc" | "docx" | "odt" | "rtf" => FileIcon::Doc,

        // Database
        "db" | "sqlite" | "sql" | "mdb" => FileIcon::Database,

        // Binary
        "o" | "a" | "so" | "dll" | "dylib" | "lib" | "obj" => FileIcon::Binary,
        "exe" | "bin" | "elf" | "out" => FileIcon::Executable,

        // Security
        "pem" | "crt" | "cer" | "key" | "p12" | "pfx" | "gpg" | "asc" => FileIcon::Key,
        "lock" => FileIcon::Lock,

        // Git
        "gitignore" | "gitattributes" | "gitmodules" => FileIcon::Git,

        _ => FileIcon::File,
    }
}

/// Infer color from file type and extension
pub fn color_from_file(file_type: crate::FileType, ext: Option<&str>, is_executable: bool) -> FileColor {
    use crate::FileType;

    match file_type {
        FileType::Directory => FileColor::Blue,
        FileType::Symlink => FileColor::Cyan,
        FileType::CharDevice | FileType::BlockDevice => FileColor::Yellow,
        FileType::Fifo => FileColor::BrightYellow,
        FileType::Socket => FileColor::Magenta,
        FileType::Regular => {
            if is_executable {
                FileColor::Green
            } else if let Some(e) = ext {
                match e.to_lowercase().as_str() {
                    // Archives
                    "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" => FileColor::Red,
                    // Images
                    "png" | "jpg" | "jpeg" | "gif" | "bmp" | "svg" => FileColor::Magenta,
                    // Audio/Video
                    "mp3" | "wav" | "mp4" | "mkv" | "avi" => FileColor::BrightMagenta,
                    _ => FileColor::Default,
                }
            } else {
                FileColor::Default
            }
        }
        FileType::Unknown => FileColor::Default,
    }
}
