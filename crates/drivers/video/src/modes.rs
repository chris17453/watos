//! Video Mode Definitions and Enumeration

use watos_driver_traits::video::{VideoMode, PixelFormat};

/// Standard VGA text modes
pub const TEXT_80X25: VideoMode = VideoMode {
    width: 80,
    height: 25,
    bpp: 4,
    format: PixelFormat::Indexed,
};

pub const TEXT_80X50: VideoMode = VideoMode {
    width: 80,
    height: 50,
    bpp: 4,
    format: PixelFormat::Indexed,
};

pub const TEXT_132X25: VideoMode = VideoMode {
    width: 132,
    height: 25,
    bpp: 4,
    format: PixelFormat::Indexed,
};

pub const TEXT_132X50: VideoMode = VideoMode {
    width: 132,
    height: 50,
    bpp: 4,
    format: PixelFormat::Indexed,
};

/// Standard VGA graphics modes
pub const VGA_MODE_13H: VideoMode = VideoMode {
    width: 320,
    height: 200,
    bpp: 8,
    format: PixelFormat::Indexed,
};

pub const VGA_MODE_12H: VideoMode = VideoMode {
    width: 640,
    height: 480,
    bpp: 4,
    format: PixelFormat::Indexed,
};

pub const VGA_MODE_10H: VideoMode = VideoMode {
    width: 640,
    height: 350,
    bpp: 4,
    format: PixelFormat::Indexed,
};

/// SVGA modes - 800x600
pub const SVGA_800X600X8: VideoMode = VideoMode {
    width: 800,
    height: 600,
    bpp: 8,
    format: PixelFormat::Indexed,
};

pub const SVGA_800X600X16: VideoMode = VideoMode {
    width: 800,
    height: 600,
    bpp: 16,
    format: PixelFormat::Rgb,
};

pub const SVGA_800X600X24: VideoMode = VideoMode {
    width: 800,
    height: 600,
    bpp: 24,
    format: PixelFormat::Rgb,
};

pub const SVGA_800X600X32: VideoMode = VideoMode {
    width: 800,
    height: 600,
    bpp: 32,
    format: PixelFormat::Rgb,
};

/// SVGA modes - 1024x768
pub const SVGA_1024X768X8: VideoMode = VideoMode {
    width: 1024,
    height: 768,
    bpp: 8,
    format: PixelFormat::Indexed,
};

pub const SVGA_1024X768X16: VideoMode = VideoMode {
    width: 1024,
    height: 768,
    bpp: 16,
    format: PixelFormat::Rgb,
};

pub const SVGA_1024X768X24: VideoMode = VideoMode {
    width: 1024,
    height: 768,
    bpp: 24,
    format: PixelFormat::Rgb,
};

pub const SVGA_1024X768X32: VideoMode = VideoMode {
    width: 1024,
    height: 768,
    bpp: 32,
    format: PixelFormat::Rgb,
};

/// SVGA modes - 1280x1024
pub const SVGA_1280X1024X8: VideoMode = VideoMode {
    width: 1280,
    height: 1024,
    bpp: 8,
    format: PixelFormat::Indexed,
};

pub const SVGA_1280X1024X16: VideoMode = VideoMode {
    width: 1280,
    height: 1024,
    bpp: 16,
    format: PixelFormat::Rgb,
};

pub const SVGA_1280X1024X24: VideoMode = VideoMode {
    width: 1280,
    height: 1024,
    bpp: 24,
    format: PixelFormat::Rgb,
};

pub const SVGA_1280X1024X32: VideoMode = VideoMode {
    width: 1280,
    height: 1024,
    bpp: 32,
    format: PixelFormat::Rgb,
};

/// Full HD modes - 1920x1080
pub const SVGA_1920X1080X24: VideoMode = VideoMode {
    width: 1920,
    height: 1080,
    bpp: 24,
    format: PixelFormat::Rgb,
};

pub const SVGA_1920X1080X32: VideoMode = VideoMode {
    width: 1920,
    height: 1080,
    bpp: 32,
    format: PixelFormat::Rgb,
};

/// All available modes
pub const ALL_MODES: &[VideoMode] = &[
    // Text modes
    TEXT_80X25,
    TEXT_80X50,
    TEXT_132X25,
    TEXT_132X50,
    // VGA graphics modes
    VGA_MODE_13H,
    VGA_MODE_12H,
    VGA_MODE_10H,
    // SVGA 800x600
    SVGA_800X600X8,
    SVGA_800X600X16,
    SVGA_800X600X24,
    SVGA_800X600X32,
    // SVGA 1024x768
    SVGA_1024X768X8,
    SVGA_1024X768X16,
    SVGA_1024X768X24,
    SVGA_1024X768X32,
    // SVGA 1280x1024
    SVGA_1280X1024X8,
    SVGA_1280X1024X16,
    SVGA_1280X1024X24,
    SVGA_1280X1024X32,
    // Full HD
    SVGA_1920X1080X24,
    SVGA_1920X1080X32,
];

/// Find a mode by resolution and bpp
pub fn find_mode(width: u32, height: u32, bpp: u8) -> Option<VideoMode> {
    ALL_MODES.iter()
        .find(|m| m.width == width && m.height == height && m.bpp == bpp)
        .copied()
}
