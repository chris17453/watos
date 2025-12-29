//! Per-Session Virtual Framebuffer Management
//!
//! Provides isolated framebuffers for multi-user GUI sessions

use alloc::vec::Vec;
use alloc::vec;
use watos_driver_traits::video::{VideoMode, Color};

/// Maximum number of concurrent sessions
const MAX_SESSIONS: usize = 16;

/// Virtual framebuffer for a single session
pub struct VirtualFramebuffer {
    /// Session ID
    pub session_id: u32,
    /// Video mode
    pub mode: VideoMode,
    /// Framebuffer data
    pub buffer: Vec<u8>,
    /// Z-order for compositing (higher = on top)
    pub z_order: i32,
    /// Is this session active/visible?
    pub active: bool,
}

impl VirtualFramebuffer {
    /// Create a new virtual framebuffer
    pub fn new(session_id: u32, mode: VideoMode) -> Self {
        let bytes_per_pixel = (mode.bpp as usize + 7) / 8;
        let buffer_size = mode.width as usize * mode.height as usize * bytes_per_pixel;
        
        VirtualFramebuffer {
            session_id,
            mode,
            buffer: vec![0u8; buffer_size],
            z_order: 0,
            active: false,
        }
    }

    /// Get pixel at position
    pub fn get_pixel(&self, x: u32, y: u32) -> Color {
        if x >= self.mode.width || y >= self.mode.height {
            return 0;
        }

        let bytes_per_pixel = (self.mode.bpp as usize + 7) / 8;
        let offset = (y as usize * self.mode.width as usize + x as usize) * bytes_per_pixel;

        match bytes_per_pixel {
            1 => self.buffer[offset] as Color,
            2 => {
                let val = u16::from_le_bytes([self.buffer[offset], self.buffer[offset + 1]]);
                // Convert RGB565 to RGBA
                let r = ((val >> 11) & 0x1F) as u8;
                let g = ((val >> 5) & 0x3F) as u8;
                let b = (val & 0x1F) as u8;
                // Scale to 8-bit: r5->r8 (*255/31), g6->g8 (*255/63), b5->b8 (*255/31)
                let r8 = (r * 255 / 31) as u32;
                let g8 = (g * 255 / 63) as u32;
                let b8 = (b * 255 / 31) as u32;
                (r8 << 16) | (g8 << 8) | b8 | 0xFF000000
            }
            3 | 4 => {
                let b = self.buffer[offset];
                let g = self.buffer[offset + 1];
                let r = self.buffer[offset + 2];
                ((r as u32) << 16) | ((g as u32) << 8) | (b as u32) | 0xFF000000
            }
            _ => 0,
        }
    }

    /// Set pixel at position
    pub fn set_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x >= self.mode.width || y >= self.mode.height {
            return;
        }

        let bytes_per_pixel = (self.mode.bpp as usize + 7) / 8;
        let offset = (y as usize * self.mode.width as usize + x as usize) * bytes_per_pixel;

        let r = ((color >> 16) & 0xFF) as u8;
        let g = ((color >> 8) & 0xFF) as u8;
        let b = (color & 0xFF) as u8;

        match bytes_per_pixel {
            1 => {
                // For indexed color, just use the low byte
                self.buffer[offset] = (color & 0xFF) as u8;
            }
            2 => {
                // RGB565
                let r5 = (r >> 3) & 0x1F;
                let g6 = (g >> 2) & 0x3F;
                let b5 = (b >> 3) & 0x1F;
                let val = ((r5 as u16) << 11) | ((g6 as u16) << 5) | (b5 as u16);
                self.buffer[offset] = (val & 0xFF) as u8;
                self.buffer[offset + 1] = (val >> 8) as u8;
            }
            3 => {
                // RGB24
                self.buffer[offset] = b;
                self.buffer[offset + 1] = g;
                self.buffer[offset + 2] = r;
            }
            4 => {
                // RGBA32
                self.buffer[offset] = b;
                self.buffer[offset + 1] = g;
                self.buffer[offset + 2] = r;
                self.buffer[offset + 3] = 0xFF;
            }
            _ => {}
        }
    }

    /// Clear the framebuffer to a color
    pub fn clear(&mut self, color: Color) {
        let r = ((color >> 16) & 0xFF) as u8;
        let g = ((color >> 8) & 0xFF) as u8;
        let b = (color & 0xFF) as u8;

        let bytes_per_pixel = (self.mode.bpp as usize + 7) / 8;

        match bytes_per_pixel {
            1 => {
                self.buffer.fill((color & 0xFF) as u8);
            }
            2 => {
                let r5 = (r >> 3) & 0x1F;
                let g6 = (g >> 2) & 0x3F;
                let b5 = (b >> 3) & 0x1F;
                let val = ((r5 as u16) << 11) | ((g6 as u16) << 5) | (b5 as u16);
                for chunk in self.buffer.chunks_exact_mut(2) {
                    chunk[0] = (val & 0xFF) as u8;
                    chunk[1] = (val >> 8) as u8;
                }
            }
            3 => {
                for chunk in self.buffer.chunks_exact_mut(3) {
                    chunk[0] = b;
                    chunk[1] = g;
                    chunk[2] = r;
                }
            }
            4 => {
                for chunk in self.buffer.chunks_exact_mut(4) {
                    chunk[0] = b;
                    chunk[1] = g;
                    chunk[2] = r;
                    chunk[3] = 0xFF;
                }
            }
            _ => {}
        }
    }
}

/// Session manager
pub struct SessionManager {
    sessions: [Option<VirtualFramebuffer>; MAX_SESSIONS],
    active_session: Option<u32>,
    next_session_id: u32,
}

impl SessionManager {
    /// Create a new session manager
    pub const fn new() -> Self {
        SessionManager {
            sessions: [const { None }; MAX_SESSIONS],
            active_session: None,
            next_session_id: 1,
        }
    }

    /// Create a new session
    pub fn create_session(&mut self, mode: VideoMode) -> Option<u32> {
        // Find free slot
        for slot in &mut self.sessions {
            if slot.is_none() {
                let session_id = self.next_session_id;
                self.next_session_id += 1;
                *slot = Some(VirtualFramebuffer::new(session_id, mode));
                return Some(session_id);
            }
        }
        None
    }

    /// Destroy a session
    pub fn destroy_session(&mut self, session_id: u32) -> bool {
        for slot in &mut self.sessions {
            if let Some(ref fb) = slot {
                if fb.session_id == session_id {
                    *slot = None;
                    if self.active_session == Some(session_id) {
                        self.active_session = None;
                    }
                    return true;
                }
            }
        }
        false
    }

    /// Get a session framebuffer (mutable)
    pub fn get_session_mut(&mut self, session_id: u32) -> Option<&mut VirtualFramebuffer> {
        self.sessions.iter_mut()
            .find_map(|s| {
                if let Some(ref mut fb) = s {
                    if fb.session_id == session_id {
                        return Some(fb);
                    }
                }
                None
            })
    }

    /// Get a session framebuffer (immutable)
    pub fn get_session(&self, session_id: u32) -> Option<&VirtualFramebuffer> {
        self.sessions.iter()
            .find_map(|s| {
                if let Some(ref fb) = s {
                    if fb.session_id == session_id {
                        return Some(fb);
                    }
                }
                None
            })
    }

    /// Set the active session
    pub fn set_active_session(&mut self, session_id: u32) -> bool {
        if self.get_session(session_id).is_some() {
            // Deactivate old session
            if let Some(old_id) = self.active_session {
                if let Some(fb) = self.get_session_mut(old_id) {
                    fb.active = false;
                }
            }
            // Activate new session
            if let Some(fb) = self.get_session_mut(session_id) {
                fb.active = true;
                self.active_session = Some(session_id);
                return true;
            }
        }
        false
    }

    /// Get the active session ID
    pub fn get_active_session(&self) -> Option<u32> {
        self.active_session
    }

    /// Composite session to physical framebuffer
    pub fn composite_to_physical<F>(&self, session_id: u32, mut write_pixel: F)
    where
        F: FnMut(u32, u32, Color),
    {
        if let Some(fb) = self.get_session(session_id) {
            for y in 0..fb.mode.height {
                for x in 0..fb.mode.width {
                    let color = fb.get_pixel(x, y);
                    write_pixel(x, y, color);
                }
            }
        }
    }
}
