# WATOS Debugging Guide

## Application Reboot/Instability Issues

This document explains the debugging infrastructure added to diagnose and fix application reboots and system instability in WATOS.

## Problem Overview

Applications like `ECHO.EXE` were causing system reboots when executed. The root causes were:

1. **Interrupt Handler Mapping**: INT 0x80 syscall handler wasn't accessible after page table switch
2. **Mixed Execution State**: Kernel mode + user page tables created unstable CPU state
3. **Stack Confusion**: Kernel stack potentially unmapped in user page table
4. **Insufficient Logging**: No visibility into where execution was failing

## Debug Features Added

### 1. Serial Port Debug Logging

All critical execution points now log to serial port (0x3F8):

**Process Execution** (`src/process/mod.rs`):
```
[PROCESS] Starting PID=X Entry=0xADDR Stack=0xADDR PML4=0xADDR
[PROCESS] Current CR3=0xADDR
[PROCESS] Switching to process page table...
[PROCESS] Page table switched, CR3=0xADDR
[PROCESS] Verifying code is accessible...
[PROCESS] First 16 bytes at entry: XX XX XX ...
[PROCESS] Verifying kernel code accessible...
[PROCESS] Syscall handler at: 0xADDR
[PROCESS] Handler first byte: 0xXX OK
[PROCESS] Calling entry point...
```

**Syscall Handler** (`src/interrupts.rs`):
```
[SYSCALL] XX from PID=YY
```

### 2. Execution Flow Verification

Before calling the process entry point, the system now:

1. **Verifies code is mapped**: Reads first 16 bytes at entry point
2. **Checks kernel accessibility**: Reads first byte of syscall handler
3. **Validates page table switch**: Compares CR3 before/after
4. **Confirms stack setup**: Uses dedicated process stack

If any verification fails, a page fault occurs immediately with diagnostic output.

### 3. Memory Model

The fixed memory model ensures stability:

```
Physical Memory Layout:
  0x000000 - 0x3FFFFF   Kernel code/data (first 4MB)
  0x400000 - 0x4FFFFF   Process 1 space (1MB)
  0x500000 - 0x5FFFFF   Process 2 space (1MB)
  ...

Virtual Memory (Process Page Table):
  0x000000 - 0x3FFFFFFF   Identity-mapped (first 1GB) - includes kernel
  0xFFFFFFFF80000000+     High virtual kernel space (also first 1GB)
  
  Process-specific:
    0x400000 - 0x4FFFFF   Process code/data
    0x4F0000 - 0x4FFFFF   Process stack (grows down)
```

Key features:
- **Identity mapping** of first 1GB ensures kernel remains accessible
- **High virtual mapping** for canonical kernel space
- **User pages** marked with USER flag for memory protection
- **Kernel pages** marked GLOBAL for TLB efficiency

### 4. Process Execution Model

**Current (Fixed) Model**:
- Process runs in **Ring 0** (kernel mode)
- Uses **process-specific page table**
- Kernel code remains **always mapped**
- Syscalls work via **INT 0x80** (no privilege change needed)
- Memory protection via **page table permissions**

This avoids the complexity of:
- Ring 3 → Ring 0 transitions
- TSS (Task State Segment) setup
- Separate user/kernel stacks
- SYSENTER/SYSCALL instructions

## Debugging Workflows

### Method 1: Serial Log Analysis (Recommended)

1. Build the system:
```bash
./scripts/build.sh
```

2. Run with serial logging:
```bash
./scripts/boot_test.sh
```

3. Check the serial log:
```bash
cat ai-temp/logs/serial_YYYYMMDD_HHMMSS.log
```

Look for:
- `[PROCESS]` lines showing execution flow
- `[SYSCALL]` lines showing system calls
- Last message before crash/reboot
- Page fault messages if verification fails

### Method 2: Interactive QEMU Session

1. Start interactive session:
```bash
./scripts/boot_test.sh --interactive
```

2. Type commands at the DOS64 prompt:
```
C:\> echo
C:\> run echo.exe
```

3. Observe output in real-time

### Method 3: EXE File Analysis (Offline)

Validate EXE files without booting:

```bash
# Build the tester (once toolchain issues resolved)
cd tools/exe-tester
cargo build --release

# Analyze an EXE
./target/release/exe-tester ../../rootfs/ECHO.EXE \
    --dump-entry \
    --check-syscalls
```

This checks:
- ELF64 header validity
- Program segment structure
- Entry point code
- Syscall (INT 0x80) presence
- Virtual address sanity

## Common Issues and Fixes

### Issue: System Reboots Immediately

**Symptoms**: No process output, instant reboot

**Likely Causes**:
1. Entry point unmapped → page fault in _start
2. Syscall handler unmapped → page fault during INT 0x80
3. Stack unmapped → page fault on first push/pop

**Debug Steps**:
1. Check serial log for last `[PROCESS]` message
2. If it stops at "Calling entry point", the entry code failed
3. If it stops at "Verifying kernel code", syscall handler is unmapped
4. Look for verification failure messages

**Fix**:
- Ensure `map_kernel_space()` in MMU maps first 1GB
- Verify ELF segments don't overlap with kernel

### Issue: Hang (No Output)

**Symptoms**: Process starts but produces no output

**Likely Causes**:
1. Syscall returns error (handle not valid)
2. Process waiting for input
3. Infinite loop in user code

**Debug Steps**:
1. Check if `[SYSCALL]` messages appear in log
2. If no syscalls, process might be looping
3. If syscalls present, check return values

**Fix**:
- Ensure process calls `SYS_CONSOLE_OUT` before `SYS_WRITE`
- Check syscall return value (< 0x100000000 = success)

### Issue: Wrong Output

**Symptoms**: Process runs but output is garbled/wrong

**Likely Causes**:
1. Wrong syscall arguments
2. Buffer pointer issues
3. Handle not properly created

**Debug Steps**:
1. Check `[SYSCALL]` logs for syscall sequence
2. Verify handle creation (`SYS_CONSOLE_OUT`)
3. Check buffer addresses passed to `SYS_WRITE`

**Fix**:
- Follow handle-based I/O pattern (see echo.exe source)
- Ensure buffers are in mapped memory

## Expected Execution Sequence (echo.exe)

Correct execution should show:

```
[PROCESS] Starting PID=1 Entry=0x401234 ...
[PROCESS] Verifying code is accessible...
[PROCESS] First 16 bytes at entry: 55 48 89 E5 ...
[PROCESS] Verifying kernel code accessible...
[PROCESS] Handler first byte: 0x50 OK
[PROCESS] Calling entry point...
[SYSCALL] 15 from PID=1          // SYS_CONSOLE_OUT
[SYSCALL] 01 from PID=1          // SYS_WRITE
Hello from WATOS echo!            // Output appears
[SYSCALL] 01 from PID=1          // SYS_WRITE
This demonstrates handle-based I/O.
[SYSCALL] 01 from PID=1          // SYS_WRITE
Process requested console access explicitly.
[SYSCALL] 06 from PID=1          // SYS_EXIT
```

## Architecture Notes

### Why Kernel Mode Execution?

Running processes in kernel mode (Ring 0) simplifies the implementation:

**Advantages**:
- No privilege level transitions needed
- No TSS setup required
- Simpler interrupt handling (same privilege level)
- Easier debugging (all code visible)

**Security**:
- Memory protection still enforced via page tables
- Processes can only access their mapped pages
- Kernel memory marked read-only where appropriate
- Different from Linux/Windows approach but valid for embedded/hobby OS

**Disadvantages**:
- Less CPU protection (no hardware privilege enforcement)
- Requires careful page table setup
- Debugging errors can be harder (triple faults instead of GPF)

### Future Enhancements

For production use, consider:

1. **Ring 3 User Mode**: Full privilege separation
   - Requires TSS setup
   - Separate user/kernel stacks
   - SYSENTER/SYSCALL instructions
   - More complex but more secure

2. **Better Error Handling**: 
   - Triple fault detection
   - Watchdog timer
   - Process crash dumps
   - Kernel debugger interface

3. **Performance**:
   - Lazy TLB flushing
   - Process-local mappings
   - Copy-on-write pages
   - Demand paging

## Testing Checklist

Before claiming echo.exe works:

- [ ] System boots to DOS64 prompt
- [ ] Typing `echo` finds ECHO.EXE
- [ ] Typing `echo` or `run echo.exe` executes it
- [ ] Output appears: "Hello from WATOS echo!"
- [ ] Second line appears: "This demonstrates handle-based I/O."
- [ ] Third line appears: "Process requested console access explicitly."
- [ ] System returns to prompt (no reboot)
- [ ] Can run echo.exe again successfully
- [ ] Serial log shows complete execution sequence
- [ ] No error messages in serial log

## Additional Resources

- **ELF64 Specification**: https://refspecs.linuxfoundation.org/elf/elf.pdf
- **x86-64 Paging**: Intel SDM Volume 3, Chapter 4
- **Syscall ABI**: See `crates/watos-syscall/src/lib.rs`
- **Process Model**: See `src/process/mod.rs`
- **Memory Management**: See `src/mmu.rs`

## Support

If issues persist:

1. Capture full serial log
2. Note the last `[PROCESS]` or `[SYSCALL]` message
3. Check for page faults or CPU exceptions
4. Verify EXE file with exe-tester (once working)
5. Compare with working applications (if any)

Remember: WATOS is a bare-metal OS with no operating system below it. Every crash is a learning opportunity!
