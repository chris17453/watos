//! WATOS Virtual Terminal Subsystem
//!
//! Provides kernel-level virtual terminals (like Linux /dev/tty1-N)
//! Each VT has its own text buffer and can be switched between.
//! The kernel VT driver renders the active VT to the framebuffer.

#![no_std]

pub mod vt;
pub mod manager;
pub mod renderer;

pub use vt::{VirtualTerminal, Color, Cell, VT_WIDTH, VT_HEIGHT};
pub use manager::{VTManager, MAX_VTS};
pub use renderer::{VTRenderer, Framebuffer, KernelFramebuffer};

use core::sync::atomic::{AtomicBool, Ordering};

static VT_INITIALIZED: AtomicBool = AtomicBool::new(false);
static mut VT_MANAGER: Option<VTManager> = None;
static mut VT_RENDERER: Option<VTRenderer> = None;
static mut FRAMEBUFFER: Option<KernelFramebuffer> = None;

/// Initialize the VT subsystem
pub fn init(fb_addr: usize, fb_width: u32, fb_height: u32, fb_pitch: u32, fb_bpp: u32, is_bgr: bool) {
    if VT_INITIALIZED.swap(true, Ordering::SeqCst) {
        return; // Already initialized
    }

    unsafe {
        VT_MANAGER = Some(VTManager::new());
        VT_RENDERER = Some(VTRenderer::new());
        FRAMEBUFFER = Some(KernelFramebuffer::new(fb_addr, fb_width, fb_height, fb_pitch, fb_bpp, is_bgr));

        watos_arch::serial_write(b"[VT] Virtual terminal subsystem initialized (");
        watos_arch::serial_hex(MAX_VTS as u64);
        watos_arch::serial_write(b" VTs)\r\n");
    }
}

/// Write data to a specific VT (1-based)
pub fn vt_write(vt_num: usize, data: &[u8]) {
    unsafe {
        if let Some(manager) = &mut VT_MANAGER {
            manager.write_vt(vt_num, data);

            // Auto-render if this is the active VT
            if manager.active_vt_num() == vt_num {
                vt_render();
            }
        }
    }
}

/// Write data to the active VT
pub fn vt_write_active(data: &[u8]) {
    unsafe {
        if let Some(manager) = &mut VT_MANAGER {
            manager.write_active(data);
            vt_render();
        }
    }
}

/// Switch to a different VT (1-based)
pub fn vt_switch(vt_num: usize) -> bool {
    unsafe {
        if let Some(manager) = &mut VT_MANAGER {
            let result = manager.switch_vt(vt_num);
            if result {
                vt_render(); // Render the newly active VT
            }
            result
        } else {
            false
        }
    }
}

/// Get active VT number (1-based)
pub fn vt_active() -> usize {
    unsafe {
        if let Some(manager) = &VT_MANAGER {
            manager.active_vt_num()
        } else {
            1
        }
    }
}

/// Render the active VT to the framebuffer
pub fn vt_render() {
    unsafe {
        if let (Some(manager), Some(renderer), Some(fb)) = (&VT_MANAGER, &VT_RENDERER, &mut FRAMEBUFFER) {
            let vt = manager.active_vt();
            if vt.is_dirty() {
                renderer.render(fb, vt);
            }
        }
    }
}

/// Clear a specific VT (1-based)
pub fn vt_clear(vt_num: usize) -> bool {
    unsafe {
        if let Some(manager) = &mut VT_MANAGER {
            let result = manager.clear_vt(vt_num);
            if result && manager.active_vt_num() == vt_num {
                vt_render();
            }
            result
        } else {
            false
        }
    }
}
