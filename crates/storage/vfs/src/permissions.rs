//! VFS Permission checking
//!
//! Provides Unix-style permission checking for VFS operations.
//! Supports owner/group/other permission bits and special bits (setuid, setgid, sticky).

use crate::{FileStat, VfsError, VfsResult};
use watos_syscall::numbers as syscall;

// Permission bit constants (Unix-style)

/// Owner read permission
pub const S_IRUSR: u32 = 0o400;
/// Owner write permission
pub const S_IWUSR: u32 = 0o200;
/// Owner execute permission
pub const S_IXUSR: u32 = 0o100;
/// Group read permission
pub const S_IRGRP: u32 = 0o040;
/// Group write permission
pub const S_IWGRP: u32 = 0o020;
/// Group execute permission
pub const S_IXGRP: u32 = 0o010;
/// Other read permission
pub const S_IROTH: u32 = 0o004;
/// Other write permission
pub const S_IWOTH: u32 = 0o002;
/// Other execute permission
pub const S_IXOTH: u32 = 0o001;

/// Set user ID on execution
pub const S_ISUID: u32 = 0o4000;
/// Set group ID on execution
pub const S_ISGID: u32 = 0o2000;
/// Sticky bit (restricted deletion)
pub const S_ISVTX: u32 = 0o1000;

/// All permission bits mask
pub const S_IRWXU: u32 = S_IRUSR | S_IWUSR | S_IXUSR;
pub const S_IRWXG: u32 = S_IRGRP | S_IWGRP | S_IXGRP;
pub const S_IRWXO: u32 = S_IROTH | S_IWOTH | S_IXOTH;

/// File type mask (upper bits of mode)
pub const S_IFMT: u32 = 0o170000;
pub const S_IFREG: u32 = 0o100000;
pub const S_IFDIR: u32 = 0o040000;
pub const S_IFLNK: u32 = 0o120000;
pub const S_IFBLK: u32 = 0o060000;
pub const S_IFCHR: u32 = 0o020000;
pub const S_IFIFO: u32 = 0o010000;
pub const S_IFSOCK: u32 = 0o140000;

/// Maximum number of supplementary groups
pub const NGROUPS_MAX: usize = 16;

/// Access mode for permission checking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessMode {
    /// Read access (R_OK)
    Read,
    /// Write access (W_OK)
    Write,
    /// Execute access (X_OK)
    Execute,
    /// Read and write access
    ReadWrite,
    /// Existence check only (F_OK)
    Exists,
}

/// Process credentials for permission checking
#[derive(Debug, Clone, Copy)]
pub struct Credentials {
    /// Real user ID
    pub uid: u32,
    /// Real group ID
    pub gid: u32,
    /// Effective user ID (for setuid programs)
    pub euid: u32,
    /// Effective group ID (for setgid programs)
    pub egid: u32,
    /// Supplementary groups
    pub groups: [u32; NGROUPS_MAX],
    /// Number of supplementary groups
    pub ngroups: usize,
}

impl Credentials {
    /// Create root credentials
    pub const fn root() -> Self {
        Credentials {
            uid: 0,
            gid: 0,
            euid: 0,
            egid: 0,
            groups: [0; NGROUPS_MAX],
            ngroups: 0,
        }
    }

    /// Create credentials for a specific user
    pub const fn new(uid: u32, gid: u32) -> Self {
        Credentials {
            uid,
            gid,
            euid: uid,
            egid: gid,
            groups: [0; NGROUPS_MAX],
            ngroups: 0,
        }
    }

    /// Check if this is root (effective uid = 0)
    pub fn is_root(&self) -> bool {
        self.euid == 0
    }

    /// Check if credentials are in a specific group
    pub fn in_group(&self, gid: u32) -> bool {
        // Check primary group
        if self.egid == gid {
            return true;
        }
        // Check supplementary groups
        for i in 0..self.ngroups {
            if self.groups[i] == gid {
                return true;
            }
        }
        false
    }

    /// Add a supplementary group
    pub fn add_group(&mut self, gid: u32) -> bool {
        if self.ngroups >= NGROUPS_MAX {
            return false;
        }
        // Don't add duplicates
        if self.in_group(gid) {
            return true;
        }
        self.groups[self.ngroups] = gid;
        self.ngroups += 1;
        true
    }
}

impl Default for Credentials {
    fn default() -> Self {
        Self::root()
    }
}

/// Get the current process credentials via syscalls
///
/// Uses SYS_GETUID/SYS_GETGID/SYS_GETEUID/SYS_GETEGID syscalls
/// to fetch the current process credentials.
pub fn get_current_credentials() -> Credentials {
    let uid = unsafe {
        let ret: u64;
        core::arch::asm!(
            "int 0x80",
            in("eax") syscall::SYS_GETUID,
            lateout("rax") ret,
            options(nostack)
        );
        ret as u32
    };

    let gid = unsafe {
        let ret: u64;
        core::arch::asm!(
            "int 0x80",
            in("eax") syscall::SYS_GETGID,
            lateout("rax") ret,
            options(nostack)
        );
        ret as u32
    };

    let euid = unsafe {
        let ret: u64;
        core::arch::asm!(
            "int 0x80",
            in("eax") syscall::SYS_GETEUID,
            lateout("rax") ret,
            options(nostack)
        );
        ret as u32
    };

    let egid = unsafe {
        let ret: u64;
        core::arch::asm!(
            "int 0x80",
            in("eax") syscall::SYS_GETEGID,
            lateout("rax") ret,
            options(nostack)
        );
        ret as u32
    };

    Credentials {
        uid,
        gid,
        euid,
        egid,
        groups: [0; NGROUPS_MAX],
        ngroups: 0,
    }
}

/// Check if credentials allow the requested access to a file
///
/// # Arguments
/// * `creds` - The process credentials
/// * `stat` - The file statistics (contains mode, uid, gid)
/// * `access` - The type of access being requested
///
/// # Returns
/// * `Ok(())` if access is allowed
/// * `Err(VfsError::PermissionDenied)` if access is denied
pub fn check_permission(creds: &Credentials, stat: &FileStat, access: AccessMode) -> VfsResult<()> {
    // Existence check always succeeds if we got here
    if access == AccessMode::Exists {
        return Ok(());
    }

    // Root can do anything (except execute requires at least one x bit)
    if creds.is_root() {
        if access == AccessMode::Execute {
            // Root can only execute if at least one execute bit is set
            let any_exec = stat.mode & (S_IXUSR | S_IXGRP | S_IXOTH);
            if any_exec == 0 {
                return Err(VfsError::PermissionDenied);
            }
        }
        return Ok(());
    }

    // Get permission bits based on identity
    let perm_bits = if creds.euid == stat.uid {
        // Owner permissions
        (stat.mode >> 6) & 0o7
    } else if creds.in_group(stat.gid) {
        // Group permissions
        (stat.mode >> 3) & 0o7
    } else {
        // Other permissions
        stat.mode & 0o7
    };

    // Check requested permission
    let required = match access {
        AccessMode::Read => 0o4,
        AccessMode::Write => 0o2,
        AccessMode::Execute => 0o1,
        AccessMode::ReadWrite => 0o6,
        AccessMode::Exists => 0o0,
    };

    if (perm_bits & required) == required {
        Ok(())
    } else {
        Err(VfsError::PermissionDenied)
    }
}

/// Check if credentials can modify file metadata (chmod/chown)
///
/// Only the file owner or root can change permissions.
/// Only root can change ownership.
pub fn can_chmod(creds: &Credentials, stat: &FileStat) -> bool {
    creds.is_root() || creds.euid == stat.uid
}

/// Check if credentials can change ownership (chown)
///
/// Only root can change file ownership.
pub fn can_chown(creds: &Credentials) -> bool {
    creds.is_root()
}

/// Check if credentials can delete a file in a directory
///
/// Requires write permission on the directory.
/// If sticky bit is set, only owner of file, owner of directory, or root can delete.
pub fn can_delete(
    creds: &Credentials,
    dir_stat: &FileStat,
    file_stat: &FileStat,
) -> VfsResult<()> {
    // Must have write permission on directory
    check_permission(creds, dir_stat, AccessMode::Write)?;

    // Check sticky bit
    if dir_stat.mode & S_ISVTX != 0 {
        // Sticky bit set: restricted deletion
        if !creds.is_root() && creds.euid != file_stat.uid && creds.euid != dir_stat.uid {
            return Err(VfsError::PermissionDenied);
        }
    }

    Ok(())
}

/// Extract permission bits from mode (lower 12 bits)
pub fn mode_permissions(mode: u32) -> u32 {
    mode & 0o7777
}

/// Extract file type from mode
pub fn mode_file_type(mode: u32) -> u32 {
    mode & S_IFMT
}

/// Format mode as octal string (e.g., "0755")
pub fn format_mode_octal(mode: u32) -> [u8; 4] {
    let perms = mode & 0o7777;
    let d0 = b'0' + ((perms >> 9) & 0o7) as u8;
    let d1 = b'0' + ((perms >> 6) & 0o7) as u8;
    let d2 = b'0' + ((perms >> 3) & 0o7) as u8;
    let d3 = b'0' + (perms & 0o7) as u8;
    [d0, d1, d2, d3]
}

/// Format mode as ls-style string (e.g., "rwxr-xr-x")
pub fn format_mode_string(mode: u32) -> [u8; 9] {
    let mut s = [b'-'; 9];

    // Owner
    if mode & S_IRUSR != 0 {
        s[0] = b'r';
    }
    if mode & S_IWUSR != 0 {
        s[1] = b'w';
    }
    if mode & S_IXUSR != 0 {
        s[2] = if mode & S_ISUID != 0 { b's' } else { b'x' };
    } else if mode & S_ISUID != 0 {
        s[2] = b'S';
    }

    // Group
    if mode & S_IRGRP != 0 {
        s[3] = b'r';
    }
    if mode & S_IWGRP != 0 {
        s[4] = b'w';
    }
    if mode & S_IXGRP != 0 {
        s[5] = if mode & S_ISGID != 0 { b's' } else { b'x' };
    } else if mode & S_ISGID != 0 {
        s[5] = b'S';
    }

    // Other
    if mode & S_IROTH != 0 {
        s[6] = b'r';
    }
    if mode & S_IWOTH != 0 {
        s[7] = b'w';
    }
    if mode & S_IXOTH != 0 {
        s[8] = if mode & S_ISVTX != 0 { b't' } else { b'x' };
    } else if mode & S_ISVTX != 0 {
        s[8] = b'T';
    }

    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file::FileType;

    fn make_stat(mode: u32, uid: u32, gid: u32) -> FileStat {
        FileStat {
            file_type: FileType::Regular,
            size: 0,
            nlink: 1,
            inode: 1,
            dev: 0,
            mode,
            uid,
            gid,
            blksize: 512,
            blocks: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
        }
    }

    #[test]
    fn test_root_access() {
        let creds = Credentials::root();
        let stat = make_stat(0o000, 1000, 1000);

        // Root can read/write anything
        assert!(check_permission(&creds, &stat, AccessMode::Read).is_ok());
        assert!(check_permission(&creds, &stat, AccessMode::Write).is_ok());

        // Root cannot execute without any x bit
        assert!(check_permission(&creds, &stat, AccessMode::Execute).is_err());

        // Root can execute if any x bit is set
        let stat_exec = make_stat(0o001, 1000, 1000);
        assert!(check_permission(&creds, &stat_exec, AccessMode::Execute).is_ok());
    }

    #[test]
    fn test_owner_access() {
        let creds = Credentials::new(1000, 1000);
        let stat = make_stat(0o700, 1000, 2000);

        assert!(check_permission(&creds, &stat, AccessMode::Read).is_ok());
        assert!(check_permission(&creds, &stat, AccessMode::Write).is_ok());
        assert!(check_permission(&creds, &stat, AccessMode::Execute).is_ok());
    }

    #[test]
    fn test_group_access() {
        let creds = Credentials::new(2000, 1000);
        let stat = make_stat(0o070, 1000, 1000);

        assert!(check_permission(&creds, &stat, AccessMode::Read).is_ok());
        assert!(check_permission(&creds, &stat, AccessMode::Write).is_ok());
        assert!(check_permission(&creds, &stat, AccessMode::Execute).is_ok());
    }

    #[test]
    fn test_other_access() {
        let creds = Credentials::new(2000, 2000);
        let stat = make_stat(0o007, 1000, 1000);

        assert!(check_permission(&creds, &stat, AccessMode::Read).is_ok());
        assert!(check_permission(&creds, &stat, AccessMode::Write).is_ok());
        assert!(check_permission(&creds, &stat, AccessMode::Execute).is_ok());
    }

    #[test]
    fn test_permission_denied() {
        let creds = Credentials::new(2000, 2000);
        let stat = make_stat(0o700, 1000, 1000);

        assert!(check_permission(&creds, &stat, AccessMode::Read).is_err());
        assert!(check_permission(&creds, &stat, AccessMode::Write).is_err());
        assert!(check_permission(&creds, &stat, AccessMode::Execute).is_err());
    }

    #[test]
    fn test_format_mode() {
        assert_eq!(format_mode_octal(0o755), [b'0', b'7', b'5', b'5']);
        assert_eq!(format_mode_octal(0o644), [b'0', b'6', b'4', b'4']);

        assert_eq!(
            format_mode_string(0o755),
            [b'r', b'w', b'x', b'r', b'-', b'x', b'r', b'-', b'x']
        );
        assert_eq!(
            format_mode_string(0o644),
            [b'r', b'w', b'-', b'r', b'-', b'-', b'r', b'-', b'-']
        );
    }
}
