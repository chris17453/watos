WATOS Dos16 Runtime Design and Plan

Goal:
Run multiple 16-bit DOS programs concurrently in a managed NTVDM-like subsystem with minimal resource overhead and DOS API correctness (INT 21h/10h/16h text/keyboard + console/file I/O).

High-level architecture:
- CLI -> FileResolver -> BinaryInspector -> RuntimeSelector -> RuntimeExecutor
- Dos16Runtime: interpreter-based CPU16 with per-task 1MiB memory image, IVT + PSP stored in task memory, and scheduler-driven execution slices.

Interpreter rules (current):
- Deterministic interpreter, fixed instruction budget per tick (configurable).
- Handle INTs by lowering to kernel-provided handlers (INT 21h -> kernel file I/O; INT 10h -> text output; INT 16h -> keyboard).
- Preserve key flags (CF, ZF, SF, OF) conservatively; lazy for others.
- Self-modifying code: mark pages as writable; simple invalidation policy (future improvement / JIT).

Loader behavior:
- COM: load at segment:offset with IP=0x100, PSP prepared at 0x0.
- EXE (MZ): parse relocation table, map initial image into memory, apply relocations into task image.

INT 21h subset (initial):
- AH=09: display $-terminated string (console)
- AH=3C: create file (map to WFS)
- AH=3D: open file
- AH=3F: read file
- AH=40: write file
- AH=3E: close file
- AH=4C: terminate with return code
(Expand as needed; unsupported calls return error codes)

Task model & scheduling:
- DosTask holds registers, memory, state, and vtables for I/O mapping.
- Scheduler: round-robin with per-task instruction budget; foreground tasks may be prioritized.
- On crash, task is terminated and cleaned up; kernel remains unaffected.

Milestones (priority order):
1. INT 21h file I/O -> map to existing WFS (3-5 days estimate)
2. EXE loader and relocation support (2-3 days)
3. INT 16h keyboard bridging + non-blocking reads (1-2 days)
4. INT 10h → framebuffer text + cursor (2-3 days)
5. Robust PSP/IVT and environment support (2 days)
6. Instrumentation, tests, and sample programs (ongoing)

Implementation notes:
- Keep runtime isolated from kernel hardware access; all I/O flows through safe kernel shims.
- Implement API adapters so future JIT or native accelerators can plug into the same runtime.
- Maintain architecture independence: DOS code always interpreted; RU native binaries run natively per-arch.

Contact:
File: ai-temp/TASKS.md contains the live checklist.
