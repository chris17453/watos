1. Your IR must model architectural state, not shortcuts
Your IR must explicitly represent:
CS, DS, ES, SS base + limit
CPL (even if always 0)
EFLAGS bit-accurate
CR0 (at least PE bit)
GDTR / IDTR
A20 state
If any of these are implicit, DOS4G will break.
Rule:
IR executes x86 semantics, not “host-optimized semantics”.
2. Instruction boundaries must be preserved
Because you are translating once:
Every IR instruction must map to exactly one x86 instruction
No instruction fusion
No reordering
No speculative elimination
DOS4G uses:
self-modifying code
runtime patching
mode switches
If instruction boundaries blur, you die.
3. IR invalidation model (critical)
You are not JITing every time, but you must invalidate IR blocks on:
any write to memory that has ever been executed as code
any change to CS base/limit
any PE bit transition
LGDT / LIDT
far JMP / far CALL / IRET across modes
Minimal rule:
store → if address ∈ executable_region → invalidate IR

This is non-negotiable.
4. Mode switches must flush everything
When DOS4G does:
mov cr0, eax
jmp far selector:offset

You must:
discard all decoded IR
re-decode under new mode rules
switch operand/address size defaults
update segment bases
Treat mode switch as a hard execution barrier.
5. Protected-mode segmentation must be real
In IR, every memory access must be:
linear = segment.base + offset
check offset <= segment.limit

Even if you later optimize flat segments.
DOS4G relies on:
limit checks
descriptor correctness
exact fault timing
You may optimize later, but semantics come first.
6. Exceptions must be first-class IR events
Do NOT lower exceptions to “host traps”.
Your IR needs explicit nodes for:
#GP (with error code)
#UD
#PF (even if paging disabled)
#DF
And they must:
push correct stack frames
vector through the IDT the program installed
respect 16-bit vs 32-bit gate sizes
DOS4G installs its own IDT and expects to own the CPU.
7. IR must preserve undefined behavior
Do NOT “sanitize”:
undefined opcodes
reserved flag bits
partial register writes
flag garbage
Many extenders use UB as feature probes.
Correct x86 UB > clean IR.
8. A20 must affect IR address calculation
This is subtle but important.
Your IR memory address calculation must include:
if !a20_enabled:
    linear &= 0xFFFFF

DOS4G will:
rely on wrap before enabling A20
rely on no wrap afterward
Fake A20 = silent corruption.
9. IR scheduling must be strictly in-order
No reordering across:
CLI / STI
PUSHF / POPF
segment loads
control register writes
Even if host CPU could reorder safely.
x86 is strongly ordered here.
10. What you can safely optimize
Once correctness works, these are safe:
constant-fold flat segments
inline linear = offset when base=0
skip limit checks when limit=4GB
cache decoded descriptors
reuse IR until invalidation
But only after correctness.
Minimal mental model
Think of your system as:
x86 → precise IR → interpreter

NOT:
x86 → optimized IR → kind-of-x86
DOS4G punishes “kind of”.