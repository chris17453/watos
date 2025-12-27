//! Color handling for terminal cells
//!
//! Uses u32 ARGB format internally. No floating point.

/// ARGB color (0xAARRGGBB format)
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Color(pub u32);

impl Color {
    pub const TRANSPARENT: Self = Self(0x00000000);
    pub const BLACK: Self = Self(0xFF000000);
    pub const WHITE: Self = Self(0xFFFFFFFF);
    pub const RED: Self = Self(0xFFFF0000);
    pub const GREEN: Self = Self(0xFF00FF00);
    pub const BLUE: Self = Self(0xFF0000FF);
    pub const YELLOW: Self = Self(0xFFFFFF00);
    pub const CYAN: Self = Self(0xFF00FFFF);
    pub const MAGENTA: Self = Self(0xFFFF00FF);

    /// Create from ARGB components
    #[inline]
    pub const fn argb(a: u8, r: u8, g: u8, b: u8) -> Self {
        Self(((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32))
    }

    /// Create opaque RGB color
    #[inline]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::argb(255, r, g, b)
    }

    /// Create from raw u32 (0xAARRGGBB)
    #[inline]
    pub const fn from_u32(val: u32) -> Self {
        Self(val)
    }

    /// Get alpha component
    #[inline]
    pub const fn a(self) -> u8 {
        ((self.0 >> 24) & 0xFF) as u8
    }

    /// Get red component
    #[inline]
    pub const fn r(self) -> u8 {
        ((self.0 >> 16) & 0xFF) as u8
    }

    /// Get green component
    #[inline]
    pub const fn g(self) -> u8 {
        ((self.0 >> 8) & 0xFF) as u8
    }

    /// Get blue component
    #[inline]
    pub const fn b(self) -> u8 {
        (self.0 & 0xFF) as u8
    }

    /// Convert to raw u32
    #[inline]
    pub const fn to_u32(self) -> u32 {
        self.0
    }

    /// Convert to BGR format (for some framebuffers)
    #[inline]
    pub const fn to_bgr(self) -> u32 {
        let a = self.a() as u32;
        let r = self.r() as u32;
        let g = self.g() as u32;
        let b = self.b() as u32;
        (a << 24) | (b << 16) | (g << 8) | r
    }

    /// Check if fully transparent
    #[inline]
    pub const fn is_transparent(self) -> bool {
        self.a() == 0
    }

    /// Check if fully opaque
    #[inline]
    pub const fn is_opaque(self) -> bool {
        self.a() == 255
    }

    /// Simple alpha blend using integer math (no floats)
    /// Formula: result = src * alpha + dst * (255 - alpha) / 255
    pub fn blend(self, background: Self) -> Self {
        let alpha = self.a() as u32;
        if alpha == 255 {
            return self;
        }
        if alpha == 0 {
            return background;
        }

        let inv_alpha = 255 - alpha;
        let r = (self.r() as u32 * alpha + background.r() as u32 * inv_alpha) / 255;
        let g = (self.g() as u32 * alpha + background.g() as u32 * inv_alpha) / 255;
        let b = (self.b() as u32 * alpha + background.b() as u32 * inv_alpha) / 255;

        Self::rgb(r as u8, g as u8, b as u8)
    }
}

/// Standard 16-color ANSI palette
pub const ANSI_COLORS: [Color; 16] = [
    Color::rgb(0, 0, 0),       // 0: Black
    Color::rgb(170, 0, 0),     // 1: Red
    Color::rgb(0, 170, 0),     // 2: Green
    Color::rgb(170, 85, 0),    // 3: Yellow/Brown
    Color::rgb(0, 0, 170),     // 4: Blue
    Color::rgb(170, 0, 170),   // 5: Magenta
    Color::rgb(0, 170, 170),   // 6: Cyan
    Color::rgb(170, 170, 170), // 7: White (light gray)
    Color::rgb(85, 85, 85),    // 8: Bright Black (dark gray)
    Color::rgb(255, 85, 85),   // 9: Bright Red
    Color::rgb(85, 255, 85),   // 10: Bright Green
    Color::rgb(255, 255, 85),  // 11: Bright Yellow
    Color::rgb(85, 85, 255),   // 12: Bright Blue
    Color::rgb(255, 85, 255),  // 13: Bright Magenta
    Color::rgb(85, 255, 255),  // 14: Bright Cyan
    Color::rgb(255, 255, 255), // 15: Bright White
];

/// Get color from 256-color palette
pub fn color_256(index: u8) -> Color {
    match index {
        // Standard 16 colors
        0..=15 => ANSI_COLORS[index as usize],
        // 216-color cube (6x6x6)
        16..=231 => {
            let idx = index - 16;
            let r = (idx / 36) % 6;
            let g = (idx / 6) % 6;
            let b = idx % 6;
            // Map 0-5 to 0, 95, 135, 175, 215, 255
            let to_val = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            Color::rgb(to_val(r), to_val(g), to_val(b))
        }
        // Grayscale (24 levels)
        232..=255 => {
            let gray = 8 + (index - 232) * 10;
            Color::rgb(gray, gray, gray)
        }
    }
}
