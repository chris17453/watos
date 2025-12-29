/// VT Manager - manages multiple virtual terminals

use crate::vt::VirtualTerminal;

pub const MAX_VTS: usize = 6; // Like Linux: tty1-tty6

pub struct VTManager {
    vts: [VirtualTerminal; MAX_VTS],
    active_vt: usize, // 0-based index
}

impl VTManager {
    pub fn new() -> Self {
        let mut vts: [VirtualTerminal; MAX_VTS] = [
            VirtualTerminal::new(1),
            VirtualTerminal::new(2),
            VirtualTerminal::new(3),
            VirtualTerminal::new(4),
            VirtualTerminal::new(5),
            VirtualTerminal::new(6),
        ];

        // Set VT 1 as active by default
        vts[0].set_active(true);

        VTManager {
            vts,
            active_vt: 0,
        }
    }

    /// Get active VT number (1-based)
    pub fn active_vt_num(&self) -> usize {
        self.active_vt + 1
    }

    /// Get mutable reference to a VT by number (1-based)
    pub fn get_vt_mut(&mut self, vt_num: usize) -> Option<&mut VirtualTerminal> {
        if vt_num >= 1 && vt_num <= MAX_VTS {
            Some(&mut self.vts[vt_num - 1])
        } else {
            None
        }
    }

    /// Get immutable reference to a VT by number (1-based)
    pub fn get_vt(&self, vt_num: usize) -> Option<&VirtualTerminal> {
        if vt_num >= 1 && vt_num <= MAX_VTS {
            Some(&self.vts[vt_num - 1])
        } else {
            None
        }
    }

    /// Get active VT (mutable)
    pub fn active_vt_mut(&mut self) -> &mut VirtualTerminal {
        &mut self.vts[self.active_vt]
    }

    /// Get active VT (immutable)
    pub fn active_vt(&self) -> &VirtualTerminal {
        &self.vts[self.active_vt]
    }

    /// Switch to a different VT (1-based)
    pub fn switch_vt(&mut self, vt_num: usize) -> bool {
        if vt_num >= 1 && vt_num <= MAX_VTS && (vt_num - 1) != self.active_vt {
            // Deactivate current VT
            self.vts[self.active_vt].set_active(false);

            // Activate new VT
            self.active_vt = vt_num - 1;
            self.vts[self.active_vt].set_active(true);

            unsafe {
                watos_arch::serial_write(b"[VT] Switched to VT ");
                watos_arch::serial_hex(vt_num as u64);
                watos_arch::serial_write(b"\r\n");
            }

            true
        } else {
            false
        }
    }

    /// Write data to a specific VT (1-based)
    pub fn write_vt(&mut self, vt_num: usize, data: &[u8]) -> bool {
        if let Some(vt) = self.get_vt_mut(vt_num) {
            vt.write(data);
            true
        } else {
            false
        }
    }

    /// Write data to the active VT
    pub fn write_active(&mut self, data: &[u8]) {
        self.vts[self.active_vt].write(data);
    }

    /// Clear a specific VT (1-based)
    pub fn clear_vt(&mut self, vt_num: usize) -> bool {
        if let Some(vt) = self.get_vt_mut(vt_num) {
            vt.clear();
            true
        } else {
            false
        }
    }

    /// Get all VTs (for rendering)
    pub fn vts(&self) -> &[VirtualTerminal; MAX_VTS] {
        &self.vts
    }
}
