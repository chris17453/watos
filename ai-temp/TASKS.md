# WATOS - DOS16 Task List

## Completed
- [x] Runtime registry and BinaryFormat detector
- [x] Dos16Runtime skeleton (task struct, Cpu16, memory image)
- [x] CLI `run <file>` integration (reads file, detect, schedule)
- [x] Runtime polling wired into main loop

## In progress / Pending
- [ ] DOS EXE loader (MZ parsing + relocations)
- [ ] INT 21h full file I/O (open/read/write/close, directories)
- [ ] INT 16h keyboard input bridging (blocking & non-blocking)
- [ ] INT 10h framebuffer text output & cursor handling
- [ ] PSP, IVT and per-task environment (environment block, args)
- [ ] Memory protection / per-task isolation improvements
- [ ] Instruction-budget tuning, priorities, and scheduler fairness
- [ ] Self-modifying code invalidation support (future JIT)
- [ ] Tests, sample DOS binaries, and documentation

## Next immediate actions
1. Implement INT 21h file I/O and map to WFS (high priority).
2. Implement EXE loader (MZ relocations) and EXE vs COM handling.
3. Add keyboard bridging and INT 16h support.

## Notes
- Tasks are recorded when run via the runtime registry. Terminated tasks are removed by the runtime poller.
- Keep changes minimal and incremental; prefer adding tests for each subsystem.
