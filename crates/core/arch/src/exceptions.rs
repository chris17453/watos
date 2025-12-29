//! CPU Exception Handlers
//!
//! Provides handlers for all x86-64 CPU exceptions (vectors 0-31).
//! These are CRITICAL for debugging - without them, any exception = triple fault.

use core::arch::naked_asm;

/// Exception vector numbers
pub mod vector {
    pub const DIVIDE_ERROR: u8 = 0;
    pub const DEBUG: u8 = 1;
    pub const NMI: u8 = 2;
    pub const BREAKPOINT: u8 = 3;
    pub const OVERFLOW: u8 = 4;
    pub const BOUND_RANGE: u8 = 5;
    pub const INVALID_OPCODE: u8 = 6;
    pub const DEVICE_NOT_AVAILABLE: u8 = 7;
    pub const DOUBLE_FAULT: u8 = 8;
    pub const COPROCESSOR_SEGMENT: u8 = 9;  // Reserved
    pub const INVALID_TSS: u8 = 10;
    pub const SEGMENT_NOT_PRESENT: u8 = 11;
    pub const STACK_SEGMENT_FAULT: u8 = 12;
    pub const GENERAL_PROTECTION: u8 = 13;
    pub const PAGE_FAULT: u8 = 14;
    pub const X87_FPU: u8 = 16;
    pub const ALIGNMENT_CHECK: u8 = 17;
    pub const MACHINE_CHECK: u8 = 18;
    pub const SIMD_FPU: u8 = 19;
    pub const VIRTUALIZATION: u8 = 20;
    pub const CONTROL_PROTECTION: u8 = 21;
    pub const HYPERVISOR_INJECTION: u8 = 28;
    pub const VMM_COMMUNICATION: u8 = 29;
    pub const SECURITY: u8 = 30;
}


/// Common exception handler - prints info and halts
/// This is jumped to after exception-specific prologue
#[unsafe(naked)]
pub unsafe extern "C" fn exception_common() {
    naked_asm!(
        // Print newline
        "mov al, 0x0D",
        "mov dx, 0x3F8",
        "out dx, al",
        "mov al, 0x0A",
        "out dx, al",

        // Print "HALT"
        "mov al, 0x48", // 'H'
        "out dx, al",
        "mov al, 0x41", // 'A'
        "out dx, al",
        "mov al, 0x4C", // 'L'
        "out dx, al",
        "mov al, 0x54", // 'T'
        "out dx, al",
        "mov al, 0x0D",
        "out dx, al",
        "mov al, 0x0A",
        "out dx, al",

        // Disable interrupts and halt forever
        "cli",
        "2: hlt",
        "jmp 2b",
        options()
    );
}

// ============================================================================
// Exception handlers WITHOUT error code
// ============================================================================

/// Division Error (Vector 0)
#[unsafe(naked)]
pub unsafe extern "C" fn divide_error() {
    naked_asm!(
        "mov al, 0x44", // 'D'
        "mov dx, 0x3F8",
        "out dx, al",
        "mov al, 0x49", // 'I'
        "out dx, al",
        "mov al, 0x56", // 'V'
        "out dx, al",
        "mov al, 0x30", // '0'
        "out dx, al",
        "jmp {common}",
        common = sym exception_common,
        options()
    );
}

/// Debug (Vector 1)
#[unsafe(naked)]
pub unsafe extern "C" fn debug() {
    naked_asm!(
        "mov al, 0x44", // 'D'
        "mov dx, 0x3F8",
        "out dx, al",
        "mov al, 0x42", // 'B'
        "out dx, al",
        "mov al, 0x47", // 'G'
        "out dx, al",
        "mov al, 0x31", // '1'
        "out dx, al",
        "jmp {common}",
        common = sym exception_common,
        options()
    );
}

/// Non-Maskable Interrupt (Vector 2)
#[unsafe(naked)]
pub unsafe extern "C" fn nmi() {
    naked_asm!(
        "mov al, 0x4E", // 'N'
        "mov dx, 0x3F8",
        "out dx, al",
        "mov al, 0x4D", // 'M'
        "out dx, al",
        "mov al, 0x49", // 'I'
        "out dx, al",
        "mov al, 0x32", // '2'
        "out dx, al",
        "jmp {common}",
        common = sym exception_common,
        options()
    );
}

/// Breakpoint (Vector 3)
#[unsafe(naked)]
pub unsafe extern "C" fn breakpoint() {
    naked_asm!(
        "mov al, 0x42", // 'B'
        "mov dx, 0x3F8",
        "out dx, al",
        "mov al, 0x50", // 'P'
        "out dx, al",
        "mov al, 0x54", // 'T'
        "out dx, al",
        "mov al, 0x33", // '3'
        "out dx, al",
        "jmp {common}",
        common = sym exception_common,
        options()
    );
}

/// Overflow (Vector 4)
#[unsafe(naked)]
pub unsafe extern "C" fn overflow() {
    naked_asm!(
        "mov al, 0x4F", // 'O'
        "mov dx, 0x3F8",
        "out dx, al",
        "mov al, 0x56", // 'V'
        "out dx, al",
        "mov al, 0x46", // 'F'
        "out dx, al",
        "mov al, 0x34", // '4'
        "out dx, al",
        "jmp {common}",
        common = sym exception_common,
        options()
    );
}

/// Bound Range Exceeded (Vector 5)
#[unsafe(naked)]
pub unsafe extern "C" fn bound_range() {
    naked_asm!(
        "mov al, 0x42", // 'B'
        "mov dx, 0x3F8",
        "out dx, al",
        "mov al, 0x4E", // 'N'
        "out dx, al",
        "mov al, 0x44", // 'D'
        "out dx, al",
        "mov al, 0x35", // '5'
        "out dx, al",
        "jmp {common}",
        common = sym exception_common,
        options()
    );
}

/// Invalid Opcode (Vector 6)
#[unsafe(naked)]
pub unsafe extern "C" fn invalid_opcode() {
    naked_asm!(
        "mov dx, 0x3F8",
        "mov al, 0x55", // 'U'
        "out dx, al",
        "mov al, 0x44", // 'D'
        "out dx, al",
        "mov al, 0x36", // '6'
        "out dx, al",
        "mov al, 0x20", // ' '
        "out dx, al",
        "mov al, 0x52", // 'R'
        "out dx, al",
        "mov al, 0x49", // 'I'
        "out dx, al",
        "mov al, 0x50", // 'P'
        "out dx, al",
        "mov al, 0x3D", // '='
        "out dx, al",

        // Print RIP from stack [RSP] as 16 hex digits
        "mov rax, [rsp]",    // Load RIP from interrupt frame
        "mov r8, 15",        // Start from digit 15 (leftmost)
        "3:",
        "mov rbx, rax",
        "mov rcx, r8",
        "shl rcx, 2",        // Multiply by 4 to get shift amount
        "shr rbx, cl",       // Shift right by (digit * 4)
        "and rbx, 0xF",
        "add bl, 0x30",
        "cmp bl, 0x3A",
        "jl 4f",
        "add bl, 7",
        "4:",
        "push rax",
        "mov al, bl",
        "out dx, al",
        "pop rax",
        "dec r8",
        "jns 3b",            // Loop while r8 >= 0

        "jmp {common}",
        common = sym exception_common,
        options()
    );
}

/// Device Not Available (Vector 7)
#[unsafe(naked)]
pub unsafe extern "C" fn device_not_available() {
    naked_asm!(
        "mov al, 0x44", // 'D'
        "mov dx, 0x3F8",
        "out dx, al",
        "mov al, 0x4E", // 'N'
        "out dx, al",
        "mov al, 0x41", // 'A'
        "out dx, al",
        "mov al, 0x37", // '7'
        "out dx, al",
        "jmp {common}",
        common = sym exception_common,
        options()
    );
}

// ============================================================================
// Exception handlers WITH error code
// ============================================================================

/// Double Fault (Vector 8) - CRITICAL, uses IST1
#[unsafe(naked)]
pub unsafe extern "C" fn double_fault() {
    naked_asm!(
        // Print "DF8!" immediately
        "mov al, 0x44", // 'D'
        "mov dx, 0x3F8",
        "out dx, al",
        "mov al, 0x46", // 'F'
        "out dx, al",
        "mov al, 0x38", // '8'
        "out dx, al",
        "mov al, 0x21", // '!'
        "out dx, al",

        // Pop error code (always 0 for double fault)
        "add rsp, 8",

        "jmp {common}",
        common = sym exception_common,
        options()
    );
}

/// Invalid TSS (Vector 10)
#[unsafe(naked)]
pub unsafe extern "C" fn invalid_tss() {
    naked_asm!(
        "mov al, 0x54", // 'T'
        "mov dx, 0x3F8",
        "out dx, al",
        "mov al, 0x53", // 'S'
        "out dx, al",
        "mov al, 0x53", // 'S'
        "out dx, al",
        "mov al, 0x41", // 'A' (10 = A in hex)
        "out dx, al",

        // Pop error code
        "add rsp, 8",

        "jmp {common}",
        common = sym exception_common,
        options()
    );
}

/// Segment Not Present (Vector 11)
#[unsafe(naked)]
pub unsafe extern "C" fn segment_not_present() {
    naked_asm!(
        "mov al, 0x53", // 'S'
        "mov dx, 0x3F8",
        "out dx, al",
        "mov al, 0x4E", // 'N'
        "out dx, al",
        "mov al, 0x50", // 'P'
        "out dx, al",
        "mov al, 0x42", // 'B' (11 = B in hex)
        "out dx, al",

        // Pop error code
        "add rsp, 8",

        "jmp {common}",
        common = sym exception_common,
        options()
    );
}

/// Stack-Segment Fault (Vector 12)
#[unsafe(naked)]
pub unsafe extern "C" fn stack_segment_fault() {
    naked_asm!(
        "mov al, 0x53", // 'S'
        "mov dx, 0x3F8",
        "out dx, al",
        "mov al, 0x53", // 'S'
        "out dx, al",
        "mov al, 0x46", // 'F'
        "out dx, al",
        "mov al, 0x43", // 'C' (12 = C in hex)
        "out dx, al",

        // Pop error code
        "add rsp, 8",

        "jmp {common}",
        common = sym exception_common,
        options()
    );
}

/// General Protection Fault (Vector 13) - Very common!
#[unsafe(naked)]
pub unsafe extern "C" fn general_protection() {
    naked_asm!(
        // Print "GP#D" (GP fault, vector D = 13)
        "mov al, 0x47", // 'G'
        "mov dx, 0x3F8",
        "out dx, al",
        "mov al, 0x50", // 'P'
        "out dx, al",
        "mov al, 0x23", // '#'
        "out dx, al",
        "mov al, 0x44", // 'D' (13 = D in hex)
        "out dx, al",

        // Print error code (it's on stack)
        "mov al, 0x3D", // '='
        "out dx, al",

        // Get error code and print it as hex
        "mov rax, [rsp]",
        "mov rcx, rax",
        "shr rcx, 4",
        "and rcx, 0xF",
        "add cl, 0x30",
        "cmp cl, 0x3A",
        "jl 2f",
        "add cl, 7",
        "2:",
        "mov al, cl",
        "out dx, al",

        "mov rcx, rax",
        "and rcx, 0xF",
        "add cl, 0x30",
        "cmp cl, 0x3A",
        "jl 3f",
        "add cl, 7",
        "3:",
        "mov al, cl",
        "out dx, al",

        // Pop error code
        "add rsp, 8",

        "jmp {common}",
        common = sym exception_common,
        options()
    );
}

/// Page Fault (Vector 14) - Very common!
#[unsafe(naked)]
pub unsafe extern "C" fn page_fault() {
    naked_asm!(
        // Print "PF#E" (Page Fault, vector E = 14)
        "mov al, 0x50", // 'P'
        "mov dx, 0x3F8",
        "out dx, al",
        "mov al, 0x46", // 'F'
        "out dx, al",
        "mov al, 0x23", // '#'
        "out dx, al",
        "mov al, 0x45", // 'E' (14 = E in hex)
        "out dx, al",

        // Print " CR2="
        "mov al, 0x20", // space
        "out dx, al",
        "mov al, 0x43", // 'C'
        "out dx, al",
        "mov al, 0x52", // 'R'
        "out dx, al",
        "mov al, 0x32", // '2'
        "out dx, al",
        "mov al, 0x3D", // '='
        "out dx, al",

        // Read CR2 (faulting address) and print as 16 hex digits
        // Use r8 for counter, rcx for shift amount
        "mov rax, cr2",
        "mov r8, 15",   // Start from digit 15 (leftmost)
        "3:",
        "mov rbx, rax",
        "mov rcx, r8",
        "shl rcx, 2",   // Multiply by 4 to get shift amount
        "shr rbx, cl",  // Shift right by (digit * 4)
        "and rbx, 0xF", // Mask to get single hex digit
        "add bl, 0x30", // Convert to ASCII
        "cmp bl, 0x3A",
        "jl 4f",
        "add bl, 7",    // Adjust for A-F
        "4:",
        "push rax",
        "mov al, bl",
        "out dx, al",
        "pop rax",
        "dec r8",
        "jns 3b",       // Loop while r8 >= 0

        // Print " ERR="
        "mov al, 0x20",
        "out dx, al",
        "mov al, 0x45", // 'E'
        "out dx, al",
        "mov al, 0x52", // 'R'
        "out dx, al",
        "mov al, 0x52", // 'R'
        "out dx, al",
        "mov al, 0x3D", // '='
        "out dx, al",

        // Error code is at [rsp], print as 4 hex digits
        "mov rax, [rsp]",
        "mov r8, 3",    // 4 digits (0-3)
        "5:",
        "mov rbx, rax",
        "mov rcx, r8",
        "shl rcx, 2",
        "shr rbx, cl",
        "and rbx, 0xF",
        "add bl, 0x30",
        "cmp bl, 0x3A",
        "jl 6f",
        "add bl, 7",
        "6:",
        "push rax",
        "mov al, bl",
        "out dx, al",
        "pop rax",
        "dec r8",
        "jns 5b",

        // Print " RIP="
        "mov al, 0x20",
        "out dx, al",
        "mov al, 0x52", // 'R'
        "out dx, al",
        "mov al, 0x49", // 'I'
        "out dx, al",
        "mov al, 0x50", // 'P'
        "out dx, al",
        "mov al, 0x3D", // '='
        "out dx, al",

        // RIP is at [rsp+8], print as 16 hex digits
        "mov rax, [rsp+8]",
        "mov r8, 15",
        "7:",
        "mov rbx, rax",
        "mov rcx, r8",
        "shl rcx, 2",
        "shr rbx, cl",
        "and rbx, 0xF",
        "add bl, 0x30",
        "cmp bl, 0x3A",
        "jl 8f",
        "add bl, 7",
        "8:",
        "push rax",
        "mov al, bl",
        "out dx, al",
        "pop rax",
        "dec r8",
        "jns 7b",

        // Pop error code
        "add rsp, 8",

        "jmp {common}",
        common = sym exception_common,
        options()
    );
}

/// x87 FPU Error (Vector 16)
#[unsafe(naked)]
pub unsafe extern "C" fn x87_fpu() {
    naked_asm!(
        "mov al, 0x46", // 'F'
        "mov dx, 0x3F8",
        "out dx, al",
        "mov al, 0x50", // 'P'
        "out dx, al",
        "mov al, 0x55", // 'U'
        "out dx, al",
        "jmp {common}",
        common = sym exception_common,
        options()
    );
}

/// Alignment Check (Vector 17)
#[unsafe(naked)]
pub unsafe extern "C" fn alignment_check() {
    naked_asm!(
        "mov al, 0x41", // 'A'
        "mov dx, 0x3F8",
        "out dx, al",
        "mov al, 0x43", // 'C'
        "out dx, al",
        "add rsp, 8", // Pop error code
        "jmp {common}",
        common = sym exception_common,
        options()
    );
}

/// Machine Check (Vector 18)
#[unsafe(naked)]
pub unsafe extern "C" fn machine_check() {
    naked_asm!(
        "mov al, 0x4D", // 'M'
        "mov dx, 0x3F8",
        "out dx, al",
        "mov al, 0x43", // 'C'
        "out dx, al",
        "jmp {common}",
        common = sym exception_common,
        options()
    );
}

/// SIMD Floating-Point (Vector 19)
#[unsafe(naked)]
pub unsafe extern "C" fn simd_fpu() {
    naked_asm!(
        "mov al, 0x53", // 'S'
        "mov dx, 0x3F8",
        "out dx, al",
        "mov al, 0x49", // 'I'
        "out dx, al",
        "mov al, 0x4D", // 'M'
        "out dx, al",
        "mov al, 0x44", // 'D'
        "out dx, al",
        "jmp {common}",
        common = sym exception_common,
        options()
    );
}

/// Generic handler for reserved/unused vectors
#[unsafe(naked)]
pub unsafe extern "C" fn reserved() {
    naked_asm!(
        "mov al, 0x52", // 'R'
        "mov dx, 0x3F8",
        "out dx, al",
        "mov al, 0x53", // 'S'
        "out dx, al",
        "mov al, 0x56", // 'V'
        "out dx, al",
        "jmp {common}",
        common = sym exception_common,
        options()
    );
}

/// Get all exception handlers as an array of function pointers
/// Returns (handler_fn, has_error_code, use_ist)
pub fn handlers() -> [(unsafe extern "C" fn(), bool, u8); 32] {
    [
        (divide_error, false, 0),           // 0
        (debug, false, 0),                  // 1
        (nmi, false, 0),                    // 2
        (breakpoint, false, 0),             // 3
        (overflow, false, 0),               // 4
        (bound_range, false, 0),            // 5
        (invalid_opcode, false, 0),         // 6
        (device_not_available, false, 0),   // 7
        (double_fault, true, 1),            // 8 - Uses IST1!
        (reserved, false, 0),               // 9
        (invalid_tss, true, 0),             // 10
        (segment_not_present, true, 0),     // 11
        (stack_segment_fault, true, 0),     // 12
        (general_protection, true, 0),      // 13
        (page_fault, true, 0),              // 14
        (reserved, false, 0),               // 15
        (x87_fpu, false, 0),                // 16
        (alignment_check, true, 0),         // 17
        (machine_check, false, 0),          // 18
        (simd_fpu, false, 0),               // 19
        (reserved, false, 0),               // 20
        (reserved, true, 0),                // 21 - Control Protection
        (reserved, false, 0),               // 22
        (reserved, false, 0),               // 23
        (reserved, false, 0),               // 24
        (reserved, false, 0),               // 25
        (reserved, false, 0),               // 26
        (reserved, false, 0),               // 27
        (reserved, false, 0),               // 28
        (reserved, true, 0),                // 29
        (reserved, true, 0),                // 30
        (reserved, false, 0),               // 31
    ]
}
