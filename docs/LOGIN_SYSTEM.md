# WATOS Login System

## Overview

WATOS now includes a login system with user management and console session switching support. This replaces the previous direct-boot-to-terminal behavior with a secure authentication flow.

## Features

### User Management
- User accounts with username, password, UID, GUID, and group memberships
- Built-in users:
  - `root` / `root` (UID 0, GID 0) - Administrator account
  - `guest` / `guest` (UID 1000, GID 100) - Guest account
- Password hashing (basic XOR-based, to be replaced with proper crypto)
- Group management with GID-based permissions

### Login Application
- Interactive login prompt
- Username and password input (password masked with asterisks)
- Authentication against user database
- Automatic launch of console/shell upon successful login
- Failed login protection with retry limit

### Console Session Management
- Support for up to 12 virtual console sessions (tty0-tty11)
- Each session can be associated with a different user
- Sessions are automatically created on first switch
- Alt+F1 through Alt+F12 key combinations for switching (infrastructure ready)

## System Calls

New syscalls have been added for authentication and session management:

### Authentication Syscalls
- `SYS_AUTHENTICATE` (120) - Authenticate user by username and password
- `SYS_SETUID` (121) - Set current process UID
- `SYS_GETUID` (122) - Get current process UID
- `SYS_GETGID` (123) - Get current process GID
- `SYS_SETGID` (124) - Set current process GID

### Session Management Syscalls
- `SYS_SESSION_CREATE` (130) - Create a new console session
- `SYS_SESSION_SWITCH` (131) - Switch to a console session by ID
- `SYS_SESSION_GET_CURRENT` (132) - Get current active session ID

## Boot Process

1. **Kernel Initialization**
   - Initialize user management subsystem with default users
   - Initialize console session manager with tty0

2. **Login Launch**
   - If `login` app is found in preloaded apps, launch it
   - Otherwise, fall back to launching TERM.EXE directly (legacy behavior)

3. **User Authentication**
   - User enters username and password
   - System validates credentials via `SYS_AUTHENTICATE`
   - On success, process UID/GID are set via `SYS_SETUID`/`SYS_SETGID`
   - Console/terminal is launched for authenticated user

4. **Session Use**
   - User works in their console session (tty0 by default)
   - Can switch to other sessions with Alt+F keys (when terminal supports it)
   - Each session maintains independent state

## Security Considerations

### Current Implementation
- Password hashing uses simple XOR-based algorithm (placeholder)
- Passwords stored in kernel memory
- No salt or iteration count
- **NOT suitable for production use**

### Recommended Improvements
1. Replace XOR hash with proper cryptographic hash (e.g., Argon2, bcrypt)
2. Add salt to password hashes
3. Implement secure password storage
4. Add rate limiting for failed login attempts
5. Add audit logging for authentication events
6. Implement privilege separation between users
7. Add permission checks for file and device access

## Usage Example

### Default Login
```
===============================================
     WATOS - Welcome to the Operating System  
===============================================

Login: root
Password: ****

Login successful! Welcome, root

C:\>
```

### Adding New Users (programmatic)
```rust
use watos_users::{add_user, GID_USERS};

// Add a new user
let uid = add_user(b"alice", b"password123", GID_USERS);
```

### Switching Sessions (in terminal code)
```rust
// Detect Alt+F2 keypress
if alt_pressed && f2_key {
    unsafe {
        syscall1(SYS_SESSION_SWITCH, 1); // Switch to tty1
    }
}
```

## Architecture

```
┌─────────────────────────────────────────────┐
│  Login App (crates/apps/login)              │
│  - Prompt for credentials                   │
│  - Call SYS_AUTHENTICATE                    │
│  - Launch console on success                │
└─────────────────────────────────────────────┘
              │
              ↓ syscall
┌─────────────────────────────────────────────┐
│  Kernel (src/main.rs)                       │
│  - Handle syscalls                          │
│  - Manage process UID/GID                   │
│  - Coordinate sessions                      │
└─────────────────────────────────────────────┘
       │                           │
       ↓                           ↓
┌───────────────────┐   ┌───────────────────┐
│  User Management  │   │  Console Sessions │
│  (sys/users)      │   │  (sys/console)    │
│  - User database  │   │  - 12 sessions    │
│  - Authentication │   │  - Switching      │
│  - Groups         │   │  - Per-user state │
└───────────────────┘   └───────────────────┘
```

## File Locations

- User management: `crates/sys/users/`
- Login application: `crates/apps/login/`
- Console sessions: `crates/sys/console/`
- Syscall definitions: `crates/core/syscall/src/lib.rs`
- Kernel handlers: `src/main.rs` (syscall handlers)
- Process UID/GID: `crates/sys/process/src/lib.rs`

## Testing

To test the login system:

1. Build the system: `./scripts/build.sh`
2. Boot in QEMU: `./scripts/boot_test.sh -i`
3. At login prompt, enter:
   - Username: `root` or `guest`
   - Password: `root` or `guest`
4. System should authenticate and launch terminal

## Future Enhancements

- Proper cryptographic password hashing
- User management utilities (adduser, passwd, etc.)
- File ownership and permissions
- sudo/su functionality
- Session persistence across reboots
- PAM-like authentication modules
- Keyboard handler updates for Alt+F key detection
- Terminal updates to support session switching
