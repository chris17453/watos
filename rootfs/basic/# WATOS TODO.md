# WATOS TODO



## 0) Pick targets (do this first)
  - [ ] Port musl
  - [ ] Port glibc (much harder)
  - [ ] Provide Linux ABI so Linux-built musl/glibc runs (hardest)
- [ ] Define ABI details:
  - [ ] Syscall calling convention
  - [ ] Data model (LP64/ILP32)
  - [ ] Endianness
  - [ ] ELF psABI for your arch
- [ ] Define filesystem layout policy:
  - [ ] FHS-ish layout (/bin, /sbin, /lib, /usr, /etc, /var, /tmp, /dev, /proc, /sys)
  - [ ] Dynamic linker location expectations (/lib/ld-musl-*.so.1 or /lib64/ld-linux-x86-64.so.2 etc)

---

## 1) Boot + kernel basics (foundation)
- [ ] Boot loader / firmware path (UEFI/BIOS/DTB)
- [ ] Physical memory manager
- [ ] Virtual memory manager
- [ ] Kernel heap allocator (slab / buddy / etc)
- [ ] Scheduler (preemptive)
- [ ] Timer subsystem (ticks + high-res)
- [ ] SMP support (if multi-core)
- [ ] Interrupt subsystem
- [ ] Syscall entry/exit path for your arch
- [ ] Copyin/copyout + user pointer validation
- [ ] Per-process address space + page faults
- [ ] Kernel object reference counting / lifetime rules

---

## 2) Process model (Linux-ish semantics)
- [ ] Processes
  - [ ] PID allocation, PID namespaces optional
  - [ ] `exit`, `exit_group`
  - [ ] `getpid`, `getppid`
- [ ] Exec
  - [ ] `execve`, `execveat` (optional early)
  - [ ] argv/envp placement
  - [ ] `auxv` setup (AT_* entries)
- [ ] Fork/clone
  - [ ] `fork` and/or `clone` (recommended: implement `clone` and build `fork` on it)
  - [ ] `vfork` (optional but probed)
  - [ ] Copy-on-write address spaces
- [ ] Waiting
  - [ ] `wait4` and/or `waitid`
- [ ] Sessions + job control
  - [ ] `setsid`
  - [ ] process groups (`setpgid`, `getpgid`)
  - [ ] `tcsetpgrp` support via tty layer

---

## 3) ELF program loader (static first, then dynamic)
- [ ] Parse ELF headers
- [ ] Support PT_LOAD segments
- [ ] Support ET_EXEC
- [ ] Support ET_DYN (PIE)
- [ ] Relocations required for your arch (minimum set to boot userland)
- [ ] Stack setup: argc/argv/envp/auxv
- [ ] ABI-required registers initial state
- [ ] `AT_PHDR`, `AT_PHENT`, `AT_PHNUM`, `AT_ENTRY`, `AT_PAGESZ`
- [ ] `AT_UID`, `AT_EUID`, `AT_GID`, `AT_EGID`
- [ ] `AT_SECURE`
- [ ] `AT_RANDOM` (and actually place 16 random bytes)
- [ ] `AT_EXECFN` (optional)
- [ ] `AT_PLATFORM` (optional)
- [ ] `AT_HWCAP` / `AT_HWCAP2` (optional; can be 0)

---

## 4) Dynamic linking support (needed for most Linux tools)
- [ ] PT_INTERP support (dynamic linker path)
- [ ] Load shared objects:
  - [ ] Parse dynamic section (DT_NEEDED, DT_STRTAB, DT_SYMTAB, DT_HASH/GNU_HASH)
  - [ ] Map segments with correct permissions
  - [ ] Perform relocations: REL/RELA, PLT/GOT
  - [ ] Lazy vs eager binding (either is fine initially)
- [ ] Symbol resolution rules (ELF global/local, interpose behavior)
- [ ] TLS
  - [ ] Static TLS
  - [ ] Dynamic TLS (dlopen use cases)
  - [ ] Thread pointer + TLS model for arch
- [ ] `dlopen`/`dlsym` basics (even minimal)
- [ ] `vdso` page (optional early, but many libcs use it)
- [ ] `errno`/thread-local `errno` correctness (libc side, but depends on TLS)

---

## 5) Threading primitives (required for modern toolchains)
- [ ] Kernel threads / user threads mapping policy
- [ ] `clone` flags for threads (CLONE_VM, CLONE_FS, CLONE_FILES, CLONE_SIGHAND, CLONE_THREAD, etc)
- [ ] TID (`gettid`)
- [ ] Futex
  - [ ] `futex` WAIT/WAKE
  - [ ] PI futex (optional, later)
  - [ ] Robust futex list (for pthread cleanup)
- [ ] `set_tid_address`
- [ ] `tgkill` / `tkill` (for thread-directed signals)
- [ ] `sched_yield`

---

## 6) Signals (Linux userland expects specific behavior)
- [ ] `sigaction`
- [ ] Signal delivery and masks
  - [ ] `sigprocmask` / `rt_sigprocmask`
  - [ ] `sigsuspend`
- [ ] `sigaltstack`
- [ ] `rt_sigtimedwait` / `sigwaitinfo` (optional but useful)
- [ ] Signal frames compatible with your ABI
- [ ] `kill`, `tgkill`
- [ ] Default actions and core dumps (optional early)
- [ ] `signalfd` (later; systemd ecosystem expects it)

---

## 7) File descriptors + VFS core
- [ ] Per-process FD table
- [ ] `close`
- [ ] `dup`, `dup2`, `dup3`
- [ ] `fcntl` (FD_CLOEXEC, O_NONBLOCK, etc)
- [ ] `ioctl` dispatch framework

---

## 8) Path resolution + filesystem semantics (must match closely)
- [ ] Absolute + relative paths
- [ ] `.` and `..`
- [ ] Symlinks (and loop limits)
- [ ] Hard links
- [ ] Permissions + mode bits
  - [ ] rwx for user/group/other
  - [ ] setuid/setgid/sticky
  - [ ] `umask`
- [ ] Ownership: uid/gid
- [ ] Mount points (even if minimal)
- [ ] Case sensitivity + normalization policy (prefer Linux-like)
- [ ] Accurate `errno` behavior (critical for tools)

---

## 9) Core file syscalls (minimum viable Linux userland)
- [ ] Open/close/read/write
  - [ ] `openat` (prefer over legacy `open`)
  - [ ] `read`, `write`
  - [ ] `pread64`, `pwrite64`
  - [ ] `lseek`
- [ ] Stat and metadata
  - [ ] `newfstatat`
  - [ ] `fstat`
  - [ ] `statx` (optional early, increasingly used)
- [ ] Directory operations
  - [ ] `mkdirat`
  - [ ] `unlinkat`
  - [ ] `renameat` (and `renameat2` optional)
  - [ ] `linkat`
  - [ ] `symlinkat`
  - [ ] `readlinkat`
  - [ ] `getdents64`
- [ ] Permissions
  - [ ] `fchmod`, `fchmodat`
  - [ ] `fchown`, `fchownat`
  - [ ] `access`, `faccessat`
- [ ] File times
  - [ ] `utimensat`
- [ ] FS sync
  - [ ] `fsync`, `fdatasync`
- [ ] FS info
  - [ ] `statfs`, `fstatfs`

---

## 10) Memory management syscalls (must be right)
- [ ] `mmap`
- [ ] `munmap`
- [ ] `mprotect`
- [ ] `brk`
- [ ] `madvise` (optional early)
- [ ] `mincore` (optional)
- [ ] `mlock`/`munlock` (optional)
- [ ] Shared memory options:
  - [ ] `shm_open`/`shm_unlink` (POSIX; can be implemented via tmpfs)
  - [ ] `memfd_create` (nice to have)
- [ ] `setrlimit`/`getrlimit` interactions with VM

---

## 11) Time + clocks (userland probes these heavily)
- [ ] `clock_gettime` (CLOCK_REALTIME, CLOCK_MONOTONIC)
- [ ] `clock_nanosleep` or `nanosleep`
- [ ] `gettimeofday` (legacy)
- [ ] `time`
- [ ] `timerfd_*` (later)
- [ ] `adjtimex` (optional)

---

## 12) TTY, PTY, termios (non-negotiable for shells)
- [ ] Character device framework
- [ ] TTY layer
- [ ] `termios` ioctls (tcgetattr/tcsetattr behavior)
- [ ] Canonical mode, echo, signals (ISIG), special chars
- [ ] Line discipline (basic)
- [ ] Controlling terminal rules
- [ ] PTY support
  - [ ] `/dev/ptmx`
  - [ ] `/dev/pts` filesystem
  - [ ] grant/unlock semantics sufficient for libc
- [ ] `isatty` correctness (via ioctl)
- [ ] Pseudo terminals required for: bash job control, ssh, sudo, tmux/screen later

---

## 13) IPC primitives
- [ ] Pipes
  - [ ] `pipe`, `pipe2`
- [ ] UNIX domain sockets (AF_UNIX)
  - [ ] stream + datagram
  - [ ] filesystem namespace sockets
  - [ ] `sendmsg`/`recvmsg` ancillary data (SCM_RIGHTS for FD passing)
- [ ] POSIX message queues (optional)
- [ ] SysV IPC (optional; some legacy tools)
- [ ] `eventfd` (later; systemd ecosystem)
- [ ] `pidfd_*` (optional)

---

## 14) Polling/event syscalls
- [ ] `select` (legacy but common)
- [ ] `poll`
- [ ] `ppoll` (optional)
- [ ] `epoll_create1`, `epoll_ctl`, `epoll_wait` (important later)
- [ ] `inotify` (important later)
- [ ] `fanotify` (optional)

---

## 15) Networking (if you want typical Linux tools)
- [ ] Socket layer core
  - [ ] `socket`, `bind`, `connect`, `listen`, `accept4`
  - [ ] `getsockopt`/`setsockopt`
  - [ ] `shutdown`
  - [ ] `sendto`/`recvfrom`
  - [ ] `sendmsg`/`recvmsg`
- [ ] Protocol families
  - [ ] AF_INET (IPv4)
  - [ ] AF_INET6 (IPv6)
  - [ ] AF_UNIX (for local services)
- [ ] DNS resolver strategy (userland via libc; kernel just needs sockets)
- [ ] `getrandom` (for TLS/ssh etc)
- [ ] Netlink (later; required by modern Linux tooling ecosystem)

---

## 16) /dev devices expected by Linux tools
- [ ] devtmpfs or static /dev population
- [ ] `/dev/null`
- [ ] `/dev/zero`
- [ ] `/dev/random`
- [ ] `/dev/urandom`
- [ ] `/dev/tty`
- [ ] `/dev/console`
- [ ] `/dev/ptmx`
- [ ] `/dev/pts/*` slaves (via devpts)
- [ ] Block device nodes (if you support disks)
- [ ] Loop device (optional)
- [ ] TUN/TAP (optional but useful)

---

## 17) procfs (a lot of Linux userland assumes this exists)
Minimum useful set:
- [ ] `/proc/self`
- [ ] `/proc/self/fd` (symlinks to open FDs)
- [ ] `/proc/self/exe`
- [ ] `/proc/self/cwd`
- [ ] `/proc/self/environ` (optional)
- [ ] `/proc/self/cmdline`
- [ ] `/proc/self/maps`
- [ ] `/proc/self/status`
- [ ] `/proc/meminfo`
- [ ] `/proc/cpuinfo`
- [ ] `/proc/stat`
- [ ] `/proc/uptime`
- [ ] `/proc/loadavg`
- [ ] `/proc/mounts` or `/proc/self/mountinfo`
- [ ] `/proc/sys` interface (optional early, but expected later)

---

## 18) sysfs (needed for udev/systemd world)
- [ ] `/sys` mount
- [ ] Basic device enumeration nodes (at least enough for your device manager)
- [ ] Kernel parameters exposure (optional early)

---

## 19) Users, groups, credentials, security model
- [ ] uid/gid storage per process
- [ ] `getuid`, `geteuid`, `getgid`, `getegid`
- [ ] `setuid`, `setgid`, `setreuid`, `setregid` (as needed)
- [ ] Supplementary groups: `getgroups`, `setgroups`
- [ ] File permission checks in VFS
- [ ] setuid binaries support (optional but common)
- [ ] Capabilities (optional)
- [ ] LSM/SELinux/AppArmor (optional)
- [ ] `prctl` minimal set (many tools probe; you can return ENOSYS for unsupported)

---

## 20) Resource limits + accounting
- [ ] `getrlimit`, `setrlimit`
- [ ] `getrusage`
- [ ] `times` (optional)
- [ ] `sysinfo` (optional)
- [ ] `sched_getaffinity` / `sched_setaffinity` (optional)

---

## 21) Randomness (modern userland relies on this)
- [ ] `getrandom`
- [ ] Kernel CSPRNG seeded early enough
- [ ] `/dev/urandom` backed by same CSPRNG

---

## 22) Locale, encodings, and basic environment expectations (userland side)
- [ ] Environment variables passed correctly through exec
- [ ] `PATH` / `HOME` basics
- [ ] `TERM` support via terminfo database (userland packaging)
- [ ] Timezone files (`/usr/share/zoneinfo`) (userland)

---

## 23) Toolchain + build system bring-up (so you can build Linux tools)
- [ ] Cross toolchain (binutils + gcc/clang + target triples)
- [ ] Sysroot layout
- [ ] C runtime startup objects (crt1.o, crti.o, crtn.o) for your platform
- [ ] Port libc (musl recommended)
  - [ ] Provide syscall wrappers and errno
  - [ ] Provide pthread using futex
  - [ ] Provide dynamic linker (ld-musl) or compatible loader
- [ ] Build: `make`, `cmake`, `meson` support (package-level)
- [ ] Port baseline packages:
  - [ ] busybox (fast win)
  - [ ] coreutils
  - [ ] bash / dash
  - [ ] grep/sed/awk
  - [ ] tar, gzip, xz
  - [ ] findutils
  - [ ] diffutils, patch
  - [ ] util-linux subset (mount, login, etc as desired)
  - [ ] openssh (requires pty, sockets, randomness)
  - [ ] git (requires lots of POSIX + networking)

---

## 24) Linux binary compatibility (if you really mean "run Linux executables")
Syscall ABI parity:
- [ ] Match Linux syscall numbers for your arch (or provide a Linux-ABI syscall table)
- [ ] Match Linux struct layouts and flags used by syscalls:
  - [ ] `stat`, `statfs`, `dirent64`, `timespec`, `timeval`
  - [ ] `sigset_t`, `siginfo_t`
  - [ ] `sockaddr`, ancillary headers
  - [ ] `termios` structs and ioctl numbers
- [ ] Match ioctl numbers/behavior for:
  - [ ] tty/pty
  - [ ] file and socket ioctls used by tools
- [ ] Support Linux-specific syscalls commonly hit:
  - [ ] `futex`
  - [ ] `epoll_*`
  - [ ] `inotify_*`
  - [ ] `eventfd`
  - [ ] `timerfd_*`
  - [ ] `signalfd`
  - [ ] `prlimit64`
  - [ ] `getrandom`
  - [ ] `uname`
  - [ ] `prctl` (subset)
- [ ] Provide Linux dynamic linker expected path(s)
- [ ] Provide glibc expectations (if running glibc-linked binaries):
  - [ ] `vdso` behaviors for time (optional but common)
  - [ ] Thread/TLS correctness
  - [ ] Robust futex list support
- [ ] Provide Linux-style `/proc` and `/sys` enough for typical probes
- [ ] Implement `personality` (optional but some tools probe)
- [ ] Implement `seccomp` (optional; many won’t need but some sandboxed apps will)

---

## 25) Namespaces, cgroups, containers (optional, but modern Linux ecosystem uses)
- [ ] Mount namespace
- [ ] PID namespace
- [ ] Network namespace
- [ ] User namespace
- [ ] IPC namespace
- [ ] UTS namespace
- [ ] cgroups v1/v2 primitives
- [ ] `unshare`, `setns` syscalls
- [ ] `clone3` (optional, newer)

---

## 26) Filesystem features many tools assume later
- [ ] tmpfs
- [ ] devpts
- [ ] procfs
- [ ] sysfs
- [ ] ext2/3/4 or other on-disk FS
- [ ] permissions/ACLs (optional)
- [ ] xattrs (`getxattr`, `setxattr`, `listxattr`, `removexattr`)
- [ ] `O_TMPFILE` (optional)
- [ ] `sendfile` (optional but useful)

---

## 27) Debugging + tracing (massively helpful)
- [ ] Kernel log + userspace dmesg equivalent
- [ ] Serial console output
- [ ] Crash dumps / backtraces
- [ ] `strace`-like syscall tracing (even if internal)
- [ ] gdb support:
  - [ ] ptrace (`ptrace` syscall)
  - [ ] `/proc/<pid>/mem` (optional)
- [ ] core dumps support (optional)

---

## 28) Packaging + runtime environment
- [ ] Init process:
  - [ ] simple init (your own) or port something
- [ ] `/etc/passwd`, `/etc/group` parsing (userland)
- [ ] `ld.so.cache` handling (glibc world) or musl loader config
- [ ] Shared library search paths:
  - [ ] `DT_RPATH`/`DT_RUNPATH`
  - [ ] `LD_LIBRARY_PATH`
- [ ] Dynamic loader environment variables support as needed
- [ ] Basic shells + PATH defaults
- [ ] Mounts at boot: /proc, /sys, /dev, /tmp

---

## 29) Conformance + test suites (so you know what’s missing)
- [ ] POSIX tests (where applicable)
- [ ] musl test suite / libc regression tests
- [ ] LTP (Linux Test Project) subset (if doing Linux ABI)
- [ ] Run busybox tests
- [ ] Run coreutils tests (targeted)
- [ ] Add syscall/errno golden tests per API

---

## 30) “Common missing things” that break Linux tools
- [ ] Wrong `errno` on edge cases
- [ ] Incomplete `fcntl` flags semantics (CLOEXEC, NONBLOCK)
- [ ] Incomplete `execve` auxv and AT_RANDOM
- [ ] Missing `getdents64` quirks
- [ ] Missing PTY + job control
- [ ] Missing `futex` or robust futex list
- [ ] Missing `/proc/self/fd` and `/proc/self/exe`
- [ ] Missing AF_UNIX + SCM_RIGHTS
- [ ] Wrong `mmap` protections and alignment rules
- [ ] Wrong signal frame / altstack behavior
- [ ] Wrong struct packing/layout vs what libc expects
