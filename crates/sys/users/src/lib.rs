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

/// Maximum GECOS field length (full name/comment)
pub const MAX_GECOS_LEN: usize = 128;

/// Maximum home directory path length
pub const MAX_HOME_LEN: usize = 64;

/// Maximum shell path length
pub const MAX_SHELL_LEN: usize = 64;

/// User entry in the user database
/// Compatible with /etc/passwd format: username:x:uid:gid:gecos:home:shell
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
    /// GECOS field (full name, comment)
    pub gecos: [u8; MAX_GECOS_LEN],
    pub gecos_len: usize,
    /// Home directory path
    pub home_dir: [u8; MAX_HOME_LEN],
    pub home_len: usize,
    /// Login shell path
    pub shell: [u8; MAX_SHELL_LEN],
    pub shell_len: usize,
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
            gecos: [0; MAX_GECOS_LEN],
            gecos_len: 0,
            home_dir: [0; MAX_HOME_LEN],
            home_len: 0,
            shell: [0; MAX_SHELL_LEN],
            shell_len: 0,
            groups: [0; 8],
            group_count: 0,
            active: false,
        }
    }

    /// Get username as a byte slice
    pub fn username_bytes(&self) -> &[u8] {
        &self.username[..self.username_len]
    }

    /// Get GECOS (full name/comment) as a byte slice
    pub fn gecos_bytes(&self) -> &[u8] {
        &self.gecos[..self.gecos_len]
    }

    /// Get home directory as a byte slice
    pub fn home_bytes(&self) -> &[u8] {
        &self.home_dir[..self.home_len]
    }

    /// Get shell as a byte slice
    pub fn shell_bytes(&self) -> &[u8] {
        &self.shell[..self.shell_len]
    }

    /// Set GECOS field
    pub fn set_gecos(&mut self, gecos: &[u8]) {
        let len = core::cmp::min(gecos.len(), MAX_GECOS_LEN);
        self.gecos[..len].copy_from_slice(&gecos[..len]);
        self.gecos_len = len;
    }

    /// Set home directory
    pub fn set_home(&mut self, home: &[u8]) {
        let len = core::cmp::min(home.len(), MAX_HOME_LEN);
        self.home_dir[..len].copy_from_slice(&home[..len]);
        self.home_len = len;
    }

    /// Set login shell
    pub fn set_shell(&mut self, shell: &[u8]) {
        let len = core::cmp::min(shell.len(), MAX_SHELL_LEN);
        self.shell[..len].copy_from_slice(&shell[..len]);
        self.shell_len = len;
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

        // Create wheel group (admin users)
        self.add_group_with_id(10, b"wheel");

        // Create root user (password: "root")
        self.add_user_full(
            UID_ROOT,
            b"root",
            b"root",
            GID_ROOT,
            b"System Administrator",
            b"/root",
            b"C:/apps/system/shell",
        );

        // Create guest user (password: "guest")
        self.add_user_full(
            UID_GUEST,
            b"guest",
            b"guest",
            GID_USERS,
            b"Guest User",
            b"/home/guest",
            b"C:/apps/system/shell",
        );
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

    /// Add a user with a specific UID (minimal info)
    fn add_user_with_id(&mut self, uid: Uid, username: &[u8], password: &[u8], gid: Gid, guid: Guid) -> bool {
        self.add_user_full_internal(uid, username, password, gid, guid, b"", b"/", b"/apps/system/shell")
    }

    /// Add a user with full /etc/passwd-compatible information
    pub fn add_user_full(
        &mut self,
        uid: Uid,
        username: &[u8],
        password: &[u8],
        gid: Gid,
        gecos: &[u8],
        home: &[u8],
        shell: &[u8],
    ) -> bool {
        self.add_user_full_internal(uid, username, password, gid, generate_guid(), gecos, home, shell)
    }

    /// Internal function to add a user with all fields
    fn add_user_full_internal(
        &mut self,
        uid: Uid,
        username: &[u8],
        password: &[u8],
        gid: Gid,
        guid: Guid,
        gecos: &[u8],
        home: &[u8],
        shell: &[u8],
    ) -> bool {
        if username.is_empty() || username.len() > MAX_USERNAME_LEN {
            return false;
        }

        // Find free slot
        for user in &mut self.users {
            if !user.active {
                user.uid = uid;
                user.gid = gid;
                user.guid = guid;

                // Username
                user.username[..username.len()].copy_from_slice(username);
                user.username_len = username.len();
                user.password_hash = hash_password(password);

                // GECOS
                let gecos_len = core::cmp::min(gecos.len(), MAX_GECOS_LEN);
                user.gecos[..gecos_len].copy_from_slice(&gecos[..gecos_len]);
                user.gecos_len = gecos_len;

                // Home directory
                let home_len = core::cmp::min(home.len(), MAX_HOME_LEN);
                user.home_dir[..home_len].copy_from_slice(&home[..home_len]);
                user.home_len = home_len;

                // Shell
                let shell_len = core::cmp::min(shell.len(), MAX_SHELL_LEN);
                user.shell[..shell_len].copy_from_slice(&shell[..shell_len]);
                user.shell_len = shell_len;

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

// ============================================================================
// /etc/passwd and /etc/group file parsing
// ============================================================================

/// Parse a u32 from a byte slice
fn parse_u32(bytes: &[u8]) -> Option<u32> {
    let mut result: u32 = 0;
    if bytes.is_empty() {
        return None;
    }
    for &b in bytes {
        if b < b'0' || b > b'9' {
            return None;
        }
        result = result.checked_mul(10)?.checked_add((b - b'0') as u32)?;
    }
    Some(result)
}

/// Split a line by a delimiter, returning parts as slices
/// Returns up to 8 parts (for passwd format: 7 fields)
fn split_line<'a>(line: &'a [u8], delim: u8) -> ([&'a [u8]; 8], usize) {
    let mut parts: [&[u8]; 8] = [&[]; 8];
    let mut count = 0;
    let mut start = 0;

    for i in 0..line.len() {
        if line[i] == delim && count < 7 {
            parts[count] = &line[start..i];
            count += 1;
            start = i + 1;
        }
    }

    // Last part
    if count < 8 {
        parts[count] = &line[start..];
        count += 1;
    }

    (parts, count)
}

/// Parse a single line from /etc/passwd format
/// Format: username:x:uid:gid:gecos:home:shell
///
/// Returns None if the line is invalid or a comment
pub fn parse_passwd_line(line: &[u8]) -> Option<User> {
    // Skip empty lines and comments
    if line.is_empty() || line[0] == b'#' {
        return None;
    }

    let (parts, count) = split_line(line, b':');

    // Need at least 7 fields
    if count < 7 {
        return None;
    }

    let username = parts[0];
    // parts[1] is password placeholder 'x'
    let uid_str = parts[2];
    let gid_str = parts[3];
    let gecos = parts[4];
    let home = parts[5];
    let shell = parts[6];

    // Validate username
    if username.is_empty() || username.len() > MAX_USERNAME_LEN {
        return None;
    }

    // Parse UID and GID
    let uid = parse_u32(uid_str)?;
    let gid = parse_u32(gid_str)?;

    let mut user = User::empty();
    user.uid = uid;
    user.gid = gid;
    user.guid = generate_guid();

    // Copy username
    user.username[..username.len()].copy_from_slice(username);
    user.username_len = username.len();

    // No password from passwd file (use shadow or set later)
    user.password_hash = 0;

    // Copy GECOS
    let gecos_len = core::cmp::min(gecos.len(), MAX_GECOS_LEN);
    user.gecos[..gecos_len].copy_from_slice(&gecos[..gecos_len]);
    user.gecos_len = gecos_len;

    // Copy home directory
    let home_len = core::cmp::min(home.len(), MAX_HOME_LEN);
    user.home_dir[..home_len].copy_from_slice(&home[..home_len]);
    user.home_len = home_len;

    // Copy shell
    let shell_len = core::cmp::min(shell.len(), MAX_SHELL_LEN);
    user.shell[..shell_len].copy_from_slice(&shell[..shell_len]);
    user.shell_len = shell_len;

    user.group_count = 0;
    user.active = true;

    Some(user)
}

/// Parse a single line from /etc/group format
/// Format: groupname:x:gid:member1,member2,...
///
/// Returns None if the line is invalid or a comment
pub fn parse_group_line(line: &[u8]) -> Option<Group> {
    // Skip empty lines and comments
    if line.is_empty() || line[0] == b'#' {
        return None;
    }

    let (parts, count) = split_line(line, b':');

    // Need at least 3 fields (members list can be empty)
    if count < 3 {
        return None;
    }

    let name = parts[0];
    // parts[1] is password placeholder 'x'
    let gid_str = parts[2];
    // parts[3] is members list (optional)

    // Validate group name
    if name.is_empty() || name.len() > MAX_GROUPNAME_LEN {
        return None;
    }

    // Parse GID
    let gid = parse_u32(gid_str)?;

    let mut group = Group::empty();
    group.gid = gid;

    // Copy name
    group.name[..name.len()].copy_from_slice(name);
    group.name_len = name.len();
    group.active = true;

    Some(group)
}

/// Load users from /etc/passwd file contents
pub fn load_passwd(data: &[u8]) {
    let mut db = USER_DB.lock();

    // Clear existing users (except keep active ones if re-loading)
    // For now, just add new users to empty slots

    let mut start = 0;
    for i in 0..=data.len() {
        if i == data.len() || data[i] == b'\n' {
            let line = &data[start..i];
            // Strip \r if present (Windows line endings)
            let line = if !line.is_empty() && line[line.len() - 1] == b'\r' {
                &line[..line.len() - 1]
            } else {
                line
            };

            if let Some(user) = parse_passwd_line(line) {
                // Find a free slot or update existing user with same UID
                let mut found = false;
                for u in &mut db.users {
                    if u.active && u.uid == user.uid {
                        // Update existing user
                        *u = user;
                        found = true;
                        break;
                    }
                }
                if !found {
                    // Add to free slot
                    for u in &mut db.users {
                        if !u.active {
                            *u = user;
                            break;
                        }
                    }
                }
            }

            start = i + 1;
        }
    }
}

/// Load groups from /etc/group file contents
pub fn load_group(data: &[u8]) {
    let mut db = USER_DB.lock();

    let mut start = 0;
    for i in 0..=data.len() {
        if i == data.len() || data[i] == b'\n' {
            let line = &data[start..i];
            // Strip \r if present (Windows line endings)
            let line = if !line.is_empty() && line[line.len() - 1] == b'\r' {
                &line[..line.len() - 1]
            } else {
                line
            };

            if let Some(group) = parse_group_line(line) {
                // Find a free slot or update existing group with same GID
                let mut found = false;
                for g in &mut db.groups {
                    if g.active && g.gid == group.gid {
                        // Update existing group
                        *g = group;
                        found = true;
                        break;
                    }
                }
                if !found {
                    // Add to free slot
                    for g in &mut db.groups {
                        if !g.active {
                            *g = group;
                            break;
                        }
                    }
                }
            }

            start = i + 1;
        }
    }
}

/// Get group by name
pub fn get_group_by_name(name: &[u8]) -> Option<Group> {
    USER_DB.lock().groups.iter().find(|g| {
        g.active && g.name_len == name.len() && g.name[..g.name_len] == name[..]
    }).copied()
}

/// Get group by GID
pub fn get_group(gid: Gid) -> Option<Group> {
    USER_DB.lock().get_group(gid).copied()
}

/// List all active users
pub fn list_users() -> [Option<User>; MAX_USERS] {
    let db = USER_DB.lock();
    let mut result = [None; MAX_USERS];
    for (i, user) in db.users.iter().enumerate() {
        if user.active {
            result[i] = Some(*user);
        }
    }
    result
}

/// List all active groups
pub fn list_groups() -> [Option<Group>; MAX_GROUPS] {
    let db = USER_DB.lock();
    let mut result = [None; MAX_GROUPS];
    for (i, group) in db.groups.iter().enumerate() {
        if group.active {
            result[i] = Some(*group);
        }
    }
    result
}

// ============================================================================
// Credential helpers for permission checking
// ============================================================================

/// Maximum supplementary groups (matching VFS NGROUPS_MAX)
pub const NGROUPS_MAX: usize = 16;

/// Credential information for permission checking
/// This is a standalone struct that can be used by VFS for access control
#[derive(Debug, Clone, Copy)]
pub struct CredentialInfo {
    /// Real user ID
    pub uid: u32,
    /// Real group ID
    pub gid: u32,
    /// Effective user ID
    pub euid: u32,
    /// Effective group ID
    pub egid: u32,
    /// Supplementary groups
    pub groups: [u32; NGROUPS_MAX],
    /// Number of supplementary groups
    pub ngroups: usize,
}

impl CredentialInfo {
    /// Create root credentials
    pub const fn root() -> Self {
        CredentialInfo {
            uid: 0,
            gid: 0,
            euid: 0,
            egid: 0,
            groups: [0; NGROUPS_MAX],
            ngroups: 0,
        }
    }

    /// Create credentials for a specific user/group
    pub const fn new(uid: u32, gid: u32) -> Self {
        CredentialInfo {
            uid,
            gid,
            euid: uid,
            egid: gid,
            groups: [0; NGROUPS_MAX],
            ngroups: 0,
        }
    }

    /// Check if this is root (uid 0)
    pub fn is_root(&self) -> bool {
        self.euid == 0
    }

    /// Check if in a specific group
    pub fn in_group(&self, gid: u32) -> bool {
        if self.egid == gid {
            return true;
        }
        for i in 0..self.ngroups {
            if self.groups[i] == gid {
                return true;
            }
        }
        false
    }
}

/// Get credential info for a specific UID
/// Looks up the user and their supplementary groups from the database
pub fn get_credential_info(uid: Uid, gid: Gid) -> CredentialInfo {
    let db = USER_DB.lock();

    let mut cred = CredentialInfo::new(uid, gid);

    // Look up user to get supplementary groups
    if let Some(user) = db.get_user(uid) {
        for i in 0..user.group_count {
            if cred.ngroups < NGROUPS_MAX {
                cred.groups[cred.ngroups] = user.groups[i];
                cred.ngroups += 1;
            }
        }
    }

    cred
}
