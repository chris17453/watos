//! User Management Subsystem for WATOS
//!
//! Provides user authentication, authorization, and session management.
//! Supports multiple users with passwords, UIDs, GUIDs, and group memberships.

#![no_std]

use spin::Mutex;

/// Maximum number of users in the system
pub const MAX_USERS: usize = 32;

/// Maximum number of groups in the system
pub const MAX_GROUPS: usize = 16;

/// Maximum username length
pub const MAX_USERNAME_LEN: usize = 32;

/// Maximum password length (for comparison)
pub const MAX_PASSWORD_LEN: usize = 64;

/// Maximum group name length
pub const MAX_GROUPNAME_LEN: usize = 32;

/// User ID type
pub type Uid = u32;

/// Group ID type
pub type Gid = u32;

/// GUID (Global Unique ID) type
pub type Guid = u128;

/// Reserved UIDs
pub const UID_ROOT: Uid = 0;
pub const UID_GUEST: Uid = 1000;

/// Reserved GIDs
pub const GID_ROOT: Gid = 0;
pub const GID_USERS: Gid = 100;

/// User entry in the user database
#[derive(Clone, Copy)]
pub struct User {
    /// User ID (unique)
    pub uid: Uid,
    /// Primary group ID
    pub gid: Gid,
    /// Global unique identifier
    pub guid: Guid,
    /// Username (null-terminated)
    pub username: [u8; MAX_USERNAME_LEN],
    pub username_len: usize,
    /// Password hash (simple XOR hash for now - replace with proper hash later)
    pub password_hash: u64,
    /// Supplementary groups (up to 8)
    pub groups: [Gid; 8],
    pub group_count: usize,
    /// Is this entry active?
    pub active: bool,
}

impl User {
    pub const fn empty() -> Self {
        User {
            uid: 0,
            gid: 0,
            guid: 0,
            username: [0; MAX_USERNAME_LEN],
            username_len: 0,
            password_hash: 0,
            groups: [0; 8],
            group_count: 0,
            active: false,
        }
    }

    /// Get username as a byte slice
    pub fn username_bytes(&self) -> &[u8] {
        &self.username[..self.username_len]
    }

    /// Check if user is in a specific group
    pub fn in_group(&self, gid: Gid) -> bool {
        if self.gid == gid {
            return true;
        }
        for i in 0..self.group_count {
            if self.groups[i] == gid {
                return true;
            }
        }
        false
    }
}

/// Group entry in the group database
#[derive(Clone, Copy)]
pub struct Group {
    /// Group ID (unique)
    pub gid: Gid,
    /// Group name (null-terminated)
    pub name: [u8; MAX_GROUPNAME_LEN],
    pub name_len: usize,
    /// Is this entry active?
    pub active: bool,
}

impl Group {
    pub const fn empty() -> Self {
        Group {
            gid: 0,
            name: [0; MAX_GROUPNAME_LEN],
            name_len: 0,
            active: false,
        }
    }

    /// Get group name as a byte slice
    pub fn name_bytes(&self) -> &[u8] {
        &self.name[..self.name_len]
    }
}

/// User database
pub struct UserDatabase {
    users: [User; MAX_USERS],
    groups: [Group; MAX_GROUPS],
    next_uid: Uid,
    next_gid: Gid,
}

impl UserDatabase {
    pub const fn new() -> Self {
        UserDatabase {
            users: [User::empty(); MAX_USERS],
            groups: [Group::empty(); MAX_GROUPS],
            next_uid: 1001, // Start user UIDs at 1001
            next_gid: 101,  // Start user GIDs at 101
        }
    }

    /// Initialize with default users and groups
    pub fn init(&mut self) {
        // Create root group
        self.add_group_with_id(GID_ROOT, b"root");
        
        // Create users group
        self.add_group_with_id(GID_USERS, b"users");

        // Create root user (password: "root")
        self.add_user_with_id(UID_ROOT, b"root", b"root", GID_ROOT, generate_guid());

        // Create guest user (password: "guest")
        self.add_user_with_id(UID_GUEST, b"guest", b"guest", GID_USERS, generate_guid());
    }

    /// Add a group with a specific GID
    fn add_group_with_id(&mut self, gid: Gid, name: &[u8]) -> bool {
        if name.is_empty() || name.len() > MAX_GROUPNAME_LEN {
            return false;
        }

        // Find free slot
        for group in &mut self.groups {
            if !group.active {
                group.gid = gid;
                group.name[..name.len()].copy_from_slice(name);
                group.name_len = name.len();
                group.active = true;
                return true;
            }
        }
        false
    }

    /// Add a user with a specific UID
    fn add_user_with_id(&mut self, uid: Uid, username: &[u8], password: &[u8], gid: Gid, guid: Guid) -> bool {
        if username.is_empty() || username.len() > MAX_USERNAME_LEN {
            return false;
        }

        // Find free slot
        for user in &mut self.users {
            if !user.active {
                user.uid = uid;
                user.gid = gid;
                user.guid = guid;
                user.username[..username.len()].copy_from_slice(username);
                user.username_len = username.len();
                user.password_hash = hash_password(password);
                user.group_count = 0;
                user.active = true;
                return true;
            }
        }
        false
    }

    /// Add a new user (assigns next available UID)
    pub fn add_user(&mut self, username: &[u8], password: &[u8], gid: Gid) -> Option<Uid> {
        let uid = self.next_uid;
        let guid = generate_guid();
        
        if self.add_user_with_id(uid, username, password, gid, guid) {
            self.next_uid += 1;
            Some(uid)
        } else {
            None
        }
    }

    /// Authenticate a user by username and password
    pub fn authenticate(&self, username: &[u8], password: &[u8]) -> Option<Uid> {
        let password_hash = hash_password(password);

        for user in &self.users {
            if user.active && user.username_len == username.len() {
                // Case-sensitive username comparison
                if user.username[..user.username_len] == username[..] {
                    if user.password_hash == password_hash {
                        return Some(user.uid);
                    }
                    return None; // Wrong password
                }
            }
        }
        None // User not found
    }

    /// Get user by UID
    pub fn get_user(&self, uid: Uid) -> Option<&User> {
        self.users.iter().find(|u| u.active && u.uid == uid)
    }

    /// Get user by username
    pub fn get_user_by_name(&self, username: &[u8]) -> Option<&User> {
        self.users.iter().find(|u| {
            u.active && u.username_len == username.len() && u.username[..u.username_len] == username[..]
        })
    }

    /// Get group by GID
    pub fn get_group(&self, gid: Gid) -> Option<&Group> {
        self.groups.iter().find(|g| g.active && g.gid == gid)
    }
}

/// Simple password hashing (XOR-based - NOT secure, replace with proper crypto)
/// This is just a placeholder for demonstration purposes
fn hash_password(password: &[u8]) -> u64 {
    let mut hash: u64 = 0x5555AAAA5555AAAA;
    for (i, &byte) in password.iter().enumerate() {
        hash ^= (byte as u64) << ((i % 8) * 8);
        hash = hash.rotate_left(7);
    }
    hash
}

/// Generate a simple GUID (based on a counter - not truly unique, replace with proper UUID)
fn generate_guid() -> Guid {
    static mut GUID_COUNTER: u128 = 0;
    unsafe {
        GUID_COUNTER += 1;
        GUID_COUNTER
    }
}

/// Global user database
static USER_DB: Mutex<UserDatabase> = Mutex::new(UserDatabase::new());

/// Initialize the user management system
pub fn init() {
    USER_DB.lock().init();
}

/// Authenticate a user
pub fn authenticate(username: &[u8], password: &[u8]) -> Option<Uid> {
    USER_DB.lock().authenticate(username, password)
}

/// Get user information by UID
pub fn get_user(uid: Uid) -> Option<User> {
    USER_DB.lock().get_user(uid).copied()
}

/// Get user information by username
pub fn get_user_by_name(username: &[u8]) -> Option<User> {
    USER_DB.lock().get_user_by_name(username).copied()
}

/// Add a new user
pub fn add_user(username: &[u8], password: &[u8], gid: Gid) -> Option<Uid> {
    USER_DB.lock().add_user(username, password, gid)
}
