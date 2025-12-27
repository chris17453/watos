//! Unit tests for DOS 16-bit CPU emulator
//!
//! Run with: cargo test --package dos16-core --features std

use super::*;

// ============================================================================
// HELPER MACROS AND FUNCTIONS
// ============================================================================

/// Create an emulator with code loaded at CS:IP
fn emu_with_code(code: &[u8]) -> Emulator {
    let mut emu = Emulator::new();
    emu.cpu.cs = 0;
    emu.cpu.ip = 0x100;
    emu.load_code_at(0, 0x100, code);
    emu
}

/// Run code and return final emulator state
fn run_code(code: &[u8]) -> Emulator {
    let mut emu = emu_with_code(code);
    emu.run(1000);
    emu
}

// ============================================================================
// CPU16 REGISTER TESTS
// ============================================================================

#[test]
fn test_cpu16_new() {
    let cpu = Cpu16::new();
    assert_eq!(cpu.ax, 0);
    assert_eq!(cpu.bx, 0);
    assert_eq!(cpu.cx, 0);
    assert_eq!(cpu.dx, 0);
    assert_eq!(cpu.sp, 0xFFFE);
    assert_eq!(cpu.ip, 0x100);
    assert_eq!(cpu.flags, 0x0002);
}

#[test]
fn test_cpu16_reg16_access() {
    let mut cpu = Cpu16::new();

    // Test all 16-bit registers by index
    cpu.set_reg16(0, 0x1234); assert_eq!(cpu.ax, 0x1234);
    cpu.set_reg16(1, 0x2345); assert_eq!(cpu.cx, 0x2345);
    cpu.set_reg16(2, 0x3456); assert_eq!(cpu.dx, 0x3456);
    cpu.set_reg16(3, 0x4567); assert_eq!(cpu.bx, 0x4567);
    cpu.set_reg16(4, 0x5678); assert_eq!(cpu.sp, 0x5678);
    cpu.set_reg16(5, 0x6789); assert_eq!(cpu.bp, 0x6789);
    cpu.set_reg16(6, 0x789A); assert_eq!(cpu.si, 0x789A);
    cpu.set_reg16(7, 0x89AB); assert_eq!(cpu.di, 0x89AB);

    // Test get
    assert_eq!(cpu.get_reg16(0), 0x1234);
    assert_eq!(cpu.get_reg16(1), 0x2345);
    assert_eq!(cpu.get_reg16(2), 0x3456);
    assert_eq!(cpu.get_reg16(3), 0x4567);
}

#[test]
fn test_cpu16_reg8_access() {
    let mut cpu = Cpu16::new();
    cpu.ax = 0x1234;
    cpu.bx = 0x5678;
    cpu.cx = 0x9ABC;
    cpu.dx = 0xDEF0;

    // Test low bytes (AL, CL, DL, BL)
    assert_eq!(cpu.get_reg8(0), 0x34); // AL
    assert_eq!(cpu.get_reg8(1), 0xBC); // CL
    assert_eq!(cpu.get_reg8(2), 0xF0); // DL
    assert_eq!(cpu.get_reg8(3), 0x78); // BL

    // Test high bytes (AH, CH, DH, BH)
    assert_eq!(cpu.get_reg8(4), 0x12); // AH
    assert_eq!(cpu.get_reg8(5), 0x9A); // CH
    assert_eq!(cpu.get_reg8(6), 0xDE); // DH
    assert_eq!(cpu.get_reg8(7), 0x56); // BH

    // Test setting low byte doesn't affect high
    cpu.set_reg8(0, 0xFF);
    assert_eq!(cpu.ax, 0x12FF);

    // Test setting high byte doesn't affect low
    cpu.set_reg8(4, 0xAB);
    assert_eq!(cpu.ax, 0xABFF);
}

#[test]
fn test_cpu16_segment_access() {
    let mut cpu = Cpu16::new();

    cpu.set_seg(0, 0x1000); assert_eq!(cpu.es, 0x1000);
    cpu.set_seg(1, 0x2000); assert_eq!(cpu.cs, 0x2000);
    cpu.set_seg(2, 0x3000); assert_eq!(cpu.ss, 0x3000);
    cpu.set_seg(3, 0x4000); assert_eq!(cpu.ds, 0x4000);

    assert_eq!(cpu.get_seg(0), 0x1000);
    assert_eq!(cpu.get_seg(1), 0x2000);
    assert_eq!(cpu.get_seg(2), 0x3000);
    assert_eq!(cpu.get_seg(3), 0x4000);
}

// ============================================================================
// FLAG TESTS
// ============================================================================

#[test]
fn test_flags_set_get() {
    let mut cpu = Cpu16::new();

    cpu.set_flag(FLAG_CF, true);
    assert!(cpu.get_flag(FLAG_CF));

    cpu.set_flag(FLAG_CF, false);
    assert!(!cpu.get_flag(FLAG_CF));

    cpu.set_flag(FLAG_ZF, true);
    cpu.set_flag(FLAG_SF, true);
    assert!(cpu.get_flag(FLAG_ZF));
    assert!(cpu.get_flag(FLAG_SF));
}

#[test]
fn test_flags_add8() {
    let mut cpu = Cpu16::new();

    // Zero result
    cpu.update_flags_add8(0, 0, 0);
    assert!(cpu.get_flag(FLAG_ZF));
    assert!(!cpu.get_flag(FLAG_SF));

    // Negative result
    cpu.update_flags_add8(0x7F, 0x01, 0x80);
    assert!(!cpu.get_flag(FLAG_ZF));
    assert!(cpu.get_flag(FLAG_SF));
    assert!(cpu.get_flag(FLAG_OF)); // Overflow: positive + positive = negative

    // Carry
    cpu.update_flags_add8(0xFF, 0x01, 0x100);
    assert!(cpu.get_flag(FLAG_CF));
}

#[test]
fn test_flags_sub8() {
    let mut cpu = Cpu16::new();

    // Zero result
    cpu.update_flags_sub8(5, 5, 0);
    assert!(cpu.get_flag(FLAG_ZF));
    assert!(!cpu.get_flag(FLAG_CF));

    // Borrow
    cpu.update_flags_sub8(5, 10, 0xFFFB);
    assert!(cpu.get_flag(FLAG_CF));
    assert!(cpu.get_flag(FLAG_SF));
}

// ============================================================================
// MEMORY TESTS
// ============================================================================

#[test]
fn test_linear_address() {
    let emu = Emulator::new();

    // 0000:0100 = 0x00100
    assert_eq!(emu.lin(0x0000, 0x0100), 0x00100);

    // 1000:0000 = 0x10000
    assert_eq!(emu.lin(0x1000, 0x0000), 0x10000);

    // 1234:5678 = 0x12340 + 0x5678 = 0x179B8
    assert_eq!(emu.lin(0x1234, 0x5678), 0x179B8);

    // Wraparound at 1MB
    assert_eq!(emu.lin(0xFFFF, 0x0010), 0x00000);
}

#[test]
fn test_memory_read_write() {
    let mut emu = Emulator::new();

    emu.write_u8(0, 0x100, 0x42);
    assert_eq!(emu.read_u8(0, 0x100), 0x42);

    emu.write_u16(0, 0x200, 0x1234);
    assert_eq!(emu.read_u16(0, 0x200), 0x1234);
    assert_eq!(emu.read_u8(0, 0x200), 0x34); // Little endian
    assert_eq!(emu.read_u8(0, 0x201), 0x12);
}

// ============================================================================
// MOV INSTRUCTION TESTS
// ============================================================================

#[test]
fn test_mov_reg16_imm16() {
    // MOV AX, 0x1234 = B8 34 12
    let emu = run_code(&[0xB8, 0x34, 0x12, 0xF4]); // HLT at end
    assert_eq!(emu.cpu.ax, 0x1234);

    // MOV BX, 0x5678 = BB 78 56
    let emu = run_code(&[0xBB, 0x78, 0x56, 0xF4]);
    assert_eq!(emu.cpu.bx, 0x5678);

    // MOV CX, 0x9ABC = B9 BC 9A
    let emu = run_code(&[0xB9, 0xBC, 0x9A, 0xF4]);
    assert_eq!(emu.cpu.cx, 0x9ABC);

    // MOV DX, 0xDEF0 = BA F0 DE
    let emu = run_code(&[0xBA, 0xF0, 0xDE, 0xF4]);
    assert_eq!(emu.cpu.dx, 0xDEF0);
}

#[test]
fn test_mov_reg8_imm8() {
    // MOV AL, 0x12 = B0 12
    let emu = run_code(&[0xB0, 0x12, 0xF4]);
    assert_eq!(emu.cpu.ax & 0xFF, 0x12);

    // MOV AH, 0x34 = B4 34
    let emu = run_code(&[0xB4, 0x34, 0xF4]);
    assert_eq!(emu.cpu.ax >> 8, 0x34);

    // MOV BL, 0x56 = B3 56
    let emu = run_code(&[0xB3, 0x56, 0xF4]);
    assert_eq!(emu.cpu.bx & 0xFF, 0x56);
}

#[test]
fn test_mov_reg_reg() {
    // MOV AX, 0x1234; MOV BX, AX
    // B8 34 12 = MOV AX, 0x1234
    // 89 C3 = MOV BX, AX (r/m16, r16 with ModRM=C3: rm=BX, reg=AX)
    let emu = run_code(&[0xB8, 0x34, 0x12, 0x89, 0xC3, 0xF4]);
    assert_eq!(emu.cpu.ax, 0x1234);
    assert_eq!(emu.cpu.bx, 0x1234);
}

#[test]
fn test_mov_mem() {
    // MOV AX, 0x1234; MOV [0x500], AX; MOV BX, [0x500]
    // A3 00 05 = MOV [0x0500], AX
    // 8B 1E 00 05 = MOV BX, [0x0500]
    let emu = run_code(&[
        0xB8, 0x34, 0x12,       // MOV AX, 0x1234
        0xA3, 0x00, 0x05,       // MOV [0x0500], AX
        0x8B, 0x1E, 0x00, 0x05, // MOV BX, [0x0500]
        0xF4                    // HLT
    ]);
    assert_eq!(emu.cpu.ax, 0x1234);
    assert_eq!(emu.cpu.bx, 0x1234);
    assert_eq!(emu.read_u16(0, 0x500), 0x1234);
}

// ============================================================================
// ADD INSTRUCTION TESTS
// ============================================================================

#[test]
fn test_add_ax_imm16() {
    // MOV AX, 0x1000; ADD AX, 0x0234
    // 05 34 02 = ADD AX, 0x0234
    let emu = run_code(&[
        0xB8, 0x00, 0x10,  // MOV AX, 0x1000
        0x05, 0x34, 0x02,  // ADD AX, 0x0234
        0xF4
    ]);
    assert_eq!(emu.cpu.ax, 0x1234);
}

#[test]
fn test_add_with_carry() {
    // MOV AX, 0xFFFF; ADD AX, 1 -> AX=0, CF=1
    let emu = run_code(&[
        0xB8, 0xFF, 0xFF,  // MOV AX, 0xFFFF
        0x05, 0x01, 0x00,  // ADD AX, 1
        0xF4
    ]);
    assert_eq!(emu.cpu.ax, 0);
    assert!(emu.cpu.get_flag(FLAG_CF));
    assert!(emu.cpu.get_flag(FLAG_ZF));
}

#[test]
fn test_add_reg_reg() {
    // MOV AX, 10; MOV BX, 20; ADD AX, BX
    // 01 D8 = ADD AX, BX (r/m16, r16)
    let emu = run_code(&[
        0xB8, 0x0A, 0x00,  // MOV AX, 10
        0xBB, 0x14, 0x00,  // MOV BX, 20
        0x01, 0xD8,        // ADD AX, BX
        0xF4
    ]);
    assert_eq!(emu.cpu.ax, 30);
}

#[test]
fn test_add_al_imm8() {
    // MOV AL, 0x10; ADD AL, 0x05
    let emu = run_code(&[
        0xB0, 0x10,        // MOV AL, 0x10
        0x04, 0x05,        // ADD AL, 0x05
        0xF4
    ]);
    assert_eq!(emu.cpu.ax & 0xFF, 0x15);
}

// ============================================================================
// SUB INSTRUCTION TESTS
// ============================================================================

#[test]
fn test_sub_ax_imm16() {
    // MOV AX, 0x1234; SUB AX, 0x0234
    let emu = run_code(&[
        0xB8, 0x34, 0x12,  // MOV AX, 0x1234
        0x2D, 0x34, 0x02,  // SUB AX, 0x0234
        0xF4
    ]);
    assert_eq!(emu.cpu.ax, 0x1000);
}

#[test]
fn test_sub_with_borrow() {
    // MOV AX, 5; SUB AX, 10 -> AX=0xFFFB, CF=1
    let emu = run_code(&[
        0xB8, 0x05, 0x00,  // MOV AX, 5
        0x2D, 0x0A, 0x00,  // SUB AX, 10
        0xF4
    ]);
    assert_eq!(emu.cpu.ax, 0xFFFB);
    assert!(emu.cpu.get_flag(FLAG_CF));
    assert!(emu.cpu.get_flag(FLAG_SF));
}

#[test]
fn test_sub_zero_result() {
    // MOV AX, 100; SUB AX, 100 -> AX=0, ZF=1
    let emu = run_code(&[
        0xB8, 0x64, 0x00,  // MOV AX, 100
        0x2D, 0x64, 0x00,  // SUB AX, 100
        0xF4
    ]);
    assert_eq!(emu.cpu.ax, 0);
    assert!(emu.cpu.get_flag(FLAG_ZF));
    assert!(!emu.cpu.get_flag(FLAG_CF));
}

// ============================================================================
// INC/DEC TESTS
// ============================================================================

#[test]
fn test_inc_reg16() {
    // MOV AX, 0xFFFF; INC AX -> AX=0, ZF=1, CF unchanged
    let mut emu = emu_with_code(&[
        0xB8, 0xFF, 0xFF,  // MOV AX, 0xFFFF
        0x40,              // INC AX
        0xF4
    ]);
    emu.cpu.set_flag(FLAG_CF, true); // Pre-set CF
    emu.run(100);
    assert_eq!(emu.cpu.ax, 0);
    assert!(emu.cpu.get_flag(FLAG_ZF));
    assert!(emu.cpu.get_flag(FLAG_CF)); // INC doesn't affect CF
}

#[test]
fn test_dec_reg16() {
    // MOV BX, 1; DEC BX -> BX=0, ZF=1
    let emu = run_code(&[
        0xBB, 0x01, 0x00,  // MOV BX, 1
        0x4B,              // DEC BX
        0xF4
    ]);
    assert_eq!(emu.cpu.bx, 0);
    assert!(emu.cpu.get_flag(FLAG_ZF));
}

// ============================================================================
// LOGIC INSTRUCTION TESTS
// ============================================================================

#[test]
fn test_and_ax_imm16() {
    // MOV AX, 0xFF00; AND AX, 0x0FF0 -> 0x0F00
    let emu = run_code(&[
        0xB8, 0x00, 0xFF,  // MOV AX, 0xFF00
        0x25, 0xF0, 0x0F,  // AND AX, 0x0FF0
        0xF4
    ]);
    assert_eq!(emu.cpu.ax, 0x0F00);
}

#[test]
fn test_or_ax_imm16() {
    // MOV AX, 0x00F0; OR AX, 0x0F00 -> 0x0FF0
    let emu = run_code(&[
        0xB8, 0xF0, 0x00,  // MOV AX, 0x00F0
        0x0D, 0x00, 0x0F,  // OR AX, 0x0F00
        0xF4
    ]);
    assert_eq!(emu.cpu.ax, 0x0FF0);
}

#[test]
fn test_xor_ax_ax() {
    // MOV AX, 0x1234; XOR AX, AX -> 0
    // 31 C0 = XOR AX, AX
    let emu = run_code(&[
        0xB8, 0x34, 0x12,  // MOV AX, 0x1234
        0x31, 0xC0,        // XOR AX, AX
        0xF4
    ]);
    assert_eq!(emu.cpu.ax, 0);
    assert!(emu.cpu.get_flag(FLAG_ZF));
}

// ============================================================================
// CMP INSTRUCTION TESTS
// ============================================================================

#[test]
fn test_cmp_equal() {
    // MOV AX, 100; CMP AX, 100 -> ZF=1, CF=0
    let emu = run_code(&[
        0xB8, 0x64, 0x00,  // MOV AX, 100
        0x3D, 0x64, 0x00,  // CMP AX, 100
        0xF4
    ]);
    assert_eq!(emu.cpu.ax, 100); // CMP doesn't modify dest
    assert!(emu.cpu.get_flag(FLAG_ZF));
    assert!(!emu.cpu.get_flag(FLAG_CF));
}

#[test]
fn test_cmp_less_than() {
    // MOV AX, 50; CMP AX, 100 -> ZF=0, CF=1 (unsigned less than)
    let emu = run_code(&[
        0xB8, 0x32, 0x00,  // MOV AX, 50
        0x3D, 0x64, 0x00,  // CMP AX, 100
        0xF4
    ]);
    assert!(!emu.cpu.get_flag(FLAG_ZF));
    assert!(emu.cpu.get_flag(FLAG_CF));
}

#[test]
fn test_cmp_greater_than() {
    // MOV AX, 100; CMP AX, 50 -> ZF=0, CF=0
    let emu = run_code(&[
        0xB8, 0x64, 0x00,  // MOV AX, 100
        0x3D, 0x32, 0x00,  // CMP AX, 50
        0xF4
    ]);
    assert!(!emu.cpu.get_flag(FLAG_ZF));
    assert!(!emu.cpu.get_flag(FLAG_CF));
}

// ============================================================================
// JUMP INSTRUCTION TESTS
// ============================================================================

#[test]
fn test_jmp_short_forward() {
    // JMP +3; INC AX; INC AX; INC AX; MOV AX, 0x1234
    // The JMP skips 3 INC instructions
    let emu = run_code(&[
        0xEB, 0x03,        // JMP +3 (skip 3 bytes)
        0x40,              // INC AX (skipped)
        0x40,              // INC AX (skipped)
        0x40,              // INC AX (skipped)
        0xB8, 0x34, 0x12,  // MOV AX, 0x1234
        0xF4
    ]);
    assert_eq!(emu.cpu.ax, 0x1234);
}

#[test]
fn test_jmp_short_backward() {
    // Set up a loop: MOV CX, 3; label: DEC CX; JNZ label
    // 0x100: MOV CX, 3
    // 0x103: DEC CX
    // 0x104: JNZ -3 (back to 0x103)
    // 0x106: HLT
    let emu = run_code(&[
        0xB9, 0x03, 0x00,  // MOV CX, 3
        0x49,              // DEC CX
        0x75, 0xFD,        // JNZ -3 (relative to 0x106)
        0xF4               // HLT
    ]);
    assert_eq!(emu.cpu.cx, 0);
}

#[test]
fn test_jmp_near() {
    // JMP near (rel16)
    // EB xx = JMP rel8
    // E9 xx xx = JMP rel16
    let emu = run_code(&[
        0xE9, 0x05, 0x00,  // JMP +5
        0x40,              // INC AX (skipped)
        0x40,              // INC AX (skipped)
        0x40,              // INC AX (skipped)
        0x40,              // INC AX (skipped)
        0x40,              // INC AX (skipped)
        0xB8, 0x42, 0x00,  // MOV AX, 0x42
        0xF4
    ]);
    assert_eq!(emu.cpu.ax, 0x42);
}

#[test]
fn test_jmp_far() {
    // JMP far 0x1234:0x5678
    // EA 78 56 34 12 = JMP 1234:5678
    let mut emu = emu_with_code(&[
        0xEA, 0x78, 0x56, 0x34, 0x12  // JMP 1234:5678
    ]);
    emu.step();
    assert_eq!(emu.cpu.cs, 0x1234);
    assert_eq!(emu.cpu.ip, 0x5678);
}

#[test]
fn test_je_taken() {
    // MOV AX, 100; CMP AX, 100; JE +3; MOV AX, 1; HLT; MOV AX, 2; HLT
    let emu = run_code(&[
        0xB8, 0x64, 0x00,  // MOV AX, 100
        0x3D, 0x64, 0x00,  // CMP AX, 100
        0x74, 0x04,        // JE +4 (skip MOV AX, 1)
        0xB8, 0x01, 0x00,  // MOV AX, 1
        0xF4,              // HLT
        0xB8, 0x02, 0x00,  // MOV AX, 2
        0xF4
    ]);
    assert_eq!(emu.cpu.ax, 2);
}

#[test]
fn test_je_not_taken() {
    // MOV AX, 100; CMP AX, 50; JE +4; MOV AX, 1; HLT; MOV AX, 2; HLT
    let emu = run_code(&[
        0xB8, 0x64, 0x00,  // MOV AX, 100
        0x3D, 0x32, 0x00,  // CMP AX, 50
        0x74, 0x04,        // JE +4 (not taken)
        0xB8, 0x01, 0x00,  // MOV AX, 1
        0xF4,              // HLT
        0xB8, 0x02, 0x00,  // MOV AX, 2
        0xF4
    ]);
    assert_eq!(emu.cpu.ax, 1);
}

#[test]
fn test_jne_taken() {
    // MOV AX, 100; CMP AX, 50; JNE +4
    let emu = run_code(&[
        0xB8, 0x64, 0x00,  // MOV AX, 100
        0x3D, 0x32, 0x00,  // CMP AX, 50
        0x75, 0x04,        // JNE +4
        0xB8, 0x01, 0x00,  // MOV AX, 1
        0xF4,
        0xB8, 0x02, 0x00,  // MOV AX, 2
        0xF4
    ]);
    assert_eq!(emu.cpu.ax, 2);
}

#[test]
fn test_jb_unsigned_less() {
    // MOV AX, 5; CMP AX, 10; JB (CF=1)
    let emu = run_code(&[
        0xB8, 0x05, 0x00,  // MOV AX, 5
        0x3D, 0x0A, 0x00,  // CMP AX, 10
        0x72, 0x04,        // JB +4 (CF=1, taken)
        0xB8, 0x01, 0x00,
        0xF4,
        0xB8, 0x02, 0x00,
        0xF4
    ]);
    assert_eq!(emu.cpu.ax, 2);
}

// ============================================================================
// CALL/RET TESTS
// ============================================================================

#[test]
fn test_call_near() {
    // CALL +3; HLT; MOV AX, 0x42; RET
    // At 0x100: CALL rel16 = E8 03 00
    // At 0x103: HLT = F4
    // At 0x104: MOV AX, 0x42 = B8 42 00
    // At 0x107: RET = C3
    let mut emu = emu_with_code(&[
        0xE8, 0x01, 0x00,  // CALL +1 (to 0x104)
        0xF4,              // HLT (return here)
        0xB8, 0x42, 0x00,  // MOV AX, 0x42
        0xC3               // RET
    ]);
    let initial_sp = emu.cpu.sp;
    emu.run(100);
    assert_eq!(emu.cpu.ax, 0x42);
    assert_eq!(emu.cpu.sp, initial_sp); // SP restored after RET
}

#[test]
fn test_call_far_and_retf() {
    // Set up far call target at 0x1000:0x0000
    let mut emu = Emulator::new();
    emu.cpu.cs = 0;
    emu.cpu.ip = 0x100;

    // Code at 0:0x100
    emu.load_code_at(0, 0x100, &[
        0x9A, 0x00, 0x00, 0x00, 0x10,  // CALL FAR 1000:0000
        0xF4                           // HLT (return here)
    ]);

    // Code at 0x1000:0x0000
    emu.load_code_at(0x1000, 0x0000, &[
        0xB8, 0x34, 0x12,  // MOV AX, 0x1234
        0xCB               // RETF
    ]);

    emu.run(100);
    assert_eq!(emu.cpu.ax, 0x1234);
    assert_eq!(emu.cpu.cs, 0);
    assert_eq!(emu.cpu.ip, 0x106); // After HLT
}

#[test]
fn test_ret_imm16() {
    // Test RET imm16 which pops extra bytes
    let mut emu = emu_with_code(&[
        0x68, 0x00, 0x00,  // PUSH 0 (fake arg)
        0x68, 0x00, 0x00,  // PUSH 0 (fake arg)
        0xE8, 0x01, 0x00,  // CALL +1
        0xF4,              // HLT
        0xC2, 0x04, 0x00   // RET 4 (pop 4 extra bytes)
    ]);
    let initial_sp = emu.cpu.sp;
    emu.run(100);
    assert_eq!(emu.cpu.sp, initial_sp); // All pushed bytes cleaned up
}

// ============================================================================
// PUSH/POP TESTS
// ============================================================================

#[test]
fn test_push_pop_reg16() {
    // PUSH AX; PUSH BX; POP CX; POP DX
    let emu = run_code(&[
        0xB8, 0x34, 0x12,  // MOV AX, 0x1234
        0xBB, 0x78, 0x56,  // MOV BX, 0x5678
        0x50,              // PUSH AX
        0x53,              // PUSH BX
        0x59,              // POP CX
        0x5A,              // POP DX
        0xF4
    ]);
    assert_eq!(emu.cpu.cx, 0x5678); // Last pushed, first popped
    assert_eq!(emu.cpu.dx, 0x1234);
}

#[test]
fn test_push_pop_segment() {
    // MOV AX, 0x1234; MOV DS, AX; PUSH DS; POP ES
    let emu = run_code(&[
        0xB8, 0x34, 0x12,  // MOV AX, 0x1234
        0x8E, 0xD8,        // MOV DS, AX
        0x1E,              // PUSH DS
        0x07,              // POP ES
        0xF4
    ]);
    assert_eq!(emu.cpu.ds, 0x1234);
    assert_eq!(emu.cpu.es, 0x1234);
}

#[test]
fn test_pushf_popf() {
    let mut emu = emu_with_code(&[
        0x9C,              // PUSHF
        0x58,              // POP AX (get flags)
        0xF4
    ]);
    emu.cpu.set_flag(FLAG_CF, true);
    emu.cpu.set_flag(FLAG_ZF, true);
    emu.run(100);
    assert!(emu.cpu.ax & FLAG_CF != 0);
    assert!(emu.cpu.ax & FLAG_ZF != 0);
}

// ============================================================================
// STRING INSTRUCTION TESTS
// ============================================================================

#[test]
fn test_movsb() {
    let mut emu = Emulator::new();
    emu.cpu.cs = 0;
    emu.cpu.ip = 0x100;
    emu.cpu.ds = 0;
    emu.cpu.es = 0;
    emu.cpu.si = 0x500;
    emu.cpu.di = 0x600;

    // Source data
    emu.write_u8(0, 0x500, 0x42);

    // Code
    emu.load_code_at(0, 0x100, &[0xA4, 0xF4]); // MOVSB; HLT
    emu.run(100);

    assert_eq!(emu.read_u8(0, 0x600), 0x42);
    assert_eq!(emu.cpu.si, 0x501);
    assert_eq!(emu.cpu.di, 0x601);
}

#[test]
fn test_rep_movsb() {
    let mut emu = Emulator::new();
    emu.cpu.cs = 0;
    emu.cpu.ip = 0x100;
    emu.cpu.ds = 0;
    emu.cpu.es = 0;
    emu.cpu.si = 0x500;
    emu.cpu.di = 0x600;
    emu.cpu.cx = 5;

    // Source data
    for i in 0..5u8 {
        emu.write_u8(0, 0x500 + i as u16, i + 1);
    }

    // Code
    emu.load_code_at(0, 0x100, &[0xF3, 0xA4, 0xF4]); // REP MOVSB; HLT
    emu.run(100);

    for i in 0..5u8 {
        assert_eq!(emu.read_u8(0, 0x600 + i as u16), i + 1);
    }
    assert_eq!(emu.cpu.cx, 0);
    assert_eq!(emu.cpu.si, 0x505);
    assert_eq!(emu.cpu.di, 0x605);
}

#[test]
fn test_rep_movsw() {
    let mut emu = Emulator::new();
    emu.cpu.cs = 0;
    emu.cpu.ip = 0x100;
    emu.cpu.ds = 0;
    emu.cpu.es = 0;
    emu.cpu.si = 0x500;
    emu.cpu.di = 0x600;
    emu.cpu.cx = 3;

    // Source data (3 words)
    emu.write_u16(0, 0x500, 0x1234);
    emu.write_u16(0, 0x502, 0x5678);
    emu.write_u16(0, 0x504, 0x9ABC);

    // Code
    emu.load_code_at(0, 0x100, &[0xF3, 0xA5, 0xF4]); // REP MOVSW; HLT
    emu.run(100);

    assert_eq!(emu.read_u16(0, 0x600), 0x1234);
    assert_eq!(emu.read_u16(0, 0x602), 0x5678);
    assert_eq!(emu.read_u16(0, 0x604), 0x9ABC);
    assert_eq!(emu.cpu.cx, 0);
}

#[test]
fn test_stosw() {
    let mut emu = Emulator::new();
    emu.cpu.cs = 0;
    emu.cpu.ip = 0x100;
    emu.cpu.es = 0;
    emu.cpu.di = 0x600;
    emu.cpu.ax = 0xABCD;

    emu.load_code_at(0, 0x100, &[0xAB, 0xF4]); // STOSW; HLT
    emu.run(100);

    assert_eq!(emu.read_u16(0, 0x600), 0xABCD);
    assert_eq!(emu.cpu.di, 0x602);
}

#[test]
fn test_rep_stosw() {
    let mut emu = Emulator::new();
    emu.cpu.cs = 0;
    emu.cpu.ip = 0x100;
    emu.cpu.es = 0;
    emu.cpu.di = 0x600;
    emu.cpu.ax = 0;
    emu.cpu.cx = 4;

    // Pre-fill with non-zero
    for i in 0..4 {
        emu.write_u16(0, 0x600 + i * 2, 0xFFFF);
    }

    emu.load_code_at(0, 0x100, &[0xF3, 0xAB, 0xF4]); // REP STOSW; HLT
    emu.run(100);

    for i in 0..4 {
        assert_eq!(emu.read_u16(0, 0x600 + i * 2), 0);
    }
}

// ============================================================================
// COM FILE LOADING TESTS
// ============================================================================

#[test]
fn test_load_com() {
    let mut emu = Emulator::new();
    let com_data = [0xB8, 0x34, 0x12, 0xF4]; // MOV AX, 0x1234; HLT

    emu.load_com(&com_data);

    // Verify code is at offset 0x100
    assert_eq!(emu.cpu.ip, 0x100);
    assert_eq!(emu.cpu.cs, 0);
    assert_eq!(emu.memory[0x100], 0xB8);
    assert_eq!(emu.memory[0x101], 0x34);
    assert_eq!(emu.memory[0x102], 0x12);
    assert_eq!(emu.memory[0x103], 0xF4);

    // Run it
    emu.run(100);
    assert_eq!(emu.cpu.ax, 0x1234);
}

// ============================================================================
// EXE FILE LOADING TESTS
// ============================================================================

/// Build a minimal MZ EXE header
fn build_mz_exe(code: &[u8], init_cs: u16, init_ip: u16, relocs: &[(u16, u16)]) -> Vec<u8> {
    let header_paras = 2; // 32 bytes = 2 paragraphs
    let header_size = header_paras * 16;
    let code_size = code.len();
    let total_size = header_size + code_size;

    let pages = (total_size + 511) / 512;
    let last_page = total_size % 512;

    let mut exe = vec![0u8; total_size];

    // MZ header
    exe[0] = b'M';
    exe[1] = b'Z';
    exe[2..4].copy_from_slice(&(last_page as u16).to_le_bytes());
    exe[4..6].copy_from_slice(&(pages as u16).to_le_bytes());
    exe[6..8].copy_from_slice(&(relocs.len() as u16).to_le_bytes());
    exe[8..10].copy_from_slice(&(header_paras as u16).to_le_bytes());
    exe[10..12].copy_from_slice(&0u16.to_le_bytes()); // min alloc
    exe[12..14].copy_from_slice(&0xFFFFu16.to_le_bytes()); // max alloc
    exe[14..16].copy_from_slice(&0u16.to_le_bytes()); // init SS
    exe[16..18].copy_from_slice(&0xFFFEu16.to_le_bytes()); // init SP
    exe[18..20].copy_from_slice(&0u16.to_le_bytes()); // checksum
    exe[20..22].copy_from_slice(&init_ip.to_le_bytes()); // init IP
    exe[22..24].copy_from_slice(&init_cs.to_le_bytes()); // init CS
    exe[24..26].copy_from_slice(&28u16.to_le_bytes()); // reloc table offset
    exe[26..28].copy_from_slice(&0u16.to_le_bytes()); // overlay

    // Relocation table at offset 28
    for (i, &(off, seg)) in relocs.iter().enumerate() {
        let rel_off = 28 + i * 4;
        if rel_off + 4 <= header_size {
            exe[rel_off..rel_off+2].copy_from_slice(&off.to_le_bytes());
            exe[rel_off+2..rel_off+4].copy_from_slice(&seg.to_le_bytes());
        }
    }

    // Code
    exe[header_size..header_size + code_size].copy_from_slice(code);

    exe
}

#[test]
fn test_load_exe_simple() {
    let code = [0xB8, 0x34, 0x12, 0xF4]; // MOV AX, 0x1234; HLT
    let exe = build_mz_exe(&code, 0, 0, &[]);

    let mut emu = Emulator::new();
    emu.load_exe(&exe).unwrap();

    // EXE loads at segment 0x10
    assert_eq!(emu.cpu.cs, 0x10);
    assert_eq!(emu.cpu.ip, 0);

    emu.run(100);
    assert_eq!(emu.cpu.ax, 0x1234);
}

#[test]
fn test_load_exe_with_relocation() {
    // Code that needs relocation:
    // ADD AX, seg_value where seg_value is a segment reference
    // 05 00 00 = ADD AX, 0x0000 <- this 0x0000 gets relocated
    let code = [0x05, 0x00, 0x00, 0xF4]; // ADD AX, imm16; HLT

    // Relocation at offset 1 (the immediate value), segment 0
    let exe = build_mz_exe(&code, 0, 0, &[(1, 0)]);

    let mut emu = Emulator::new();
    emu.load_exe(&exe).unwrap();

    // The immediate should now be 0x0010 (load segment)
    let addr = emu.lin(0x10, 1);
    let relocated_val = u16::from_le_bytes([emu.memory[addr], emu.memory[addr + 1]]);
    assert_eq!(relocated_val, 0x0010);

    emu.run(100);
    assert_eq!(emu.cpu.ax, 0x0010); // 0 + relocated segment
}

#[test]
fn test_exe_header_validation() {
    let mut emu = Emulator::new();

    // Too small
    assert!(emu.load_exe(&[b'M', b'Z']).is_err());

    // Wrong magic
    let mut bad_exe = build_mz_exe(&[0xF4], 0, 0, &[]);
    bad_exe[0] = b'X';
    assert!(emu.load_exe(&bad_exe).is_err());
}

// ============================================================================
// INTERRUPT TESTS
// ============================================================================

#[test]
fn test_int_returns_interrupt() {
    let mut emu = emu_with_code(&[
        0xB4, 0x09,        // MOV AH, 9
        0xCD, 0x21,        // INT 21h
        0xF4
    ]);

    emu.step(); // MOV AH, 9
    let result = emu.step(); // INT 21h

    assert_eq!(result, StepResult::Interrupt(0x21));
    assert_eq!(emu.cpu.ax >> 8, 9);
}

// ============================================================================
// EDGE CASE TESTS
// ============================================================================

#[test]
fn test_ip_wraparound() {
    // Test that IP correctly wraps around within segment
    let mut emu = Emulator::new();
    emu.cpu.cs = 0;
    emu.cpu.ip = 0xFFFE;

    // Code at 0:0xFFFE
    emu.write_u8(0, 0xFFFE, 0x40); // INC AX
    emu.write_u8(0, 0xFFFF, 0x40); // INC AX
    emu.write_u8(0, 0x0000, 0xF4); // HLT (at wrapped address)

    emu.run(100);
    assert_eq!(emu.cpu.ax, 2);
    assert_eq!(emu.cpu.ip, 0x0001);
}

#[test]
fn test_unknown_opcode() {
    let mut emu = emu_with_code(&[0x0F]); // 0x0F alone is often part of 2-byte opcode
    let result = emu.step();
    match result {
        StepResult::UnknownOpcode(0x0F) => {}
        _ => panic!("Expected UnknownOpcode(0x0F), got {:?}", result),
    }
}

#[test]
fn test_segment_override() {
    // Test ES segment override
    let mut emu = Emulator::new();
    emu.cpu.cs = 0;
    emu.cpu.ip = 0x100;
    emu.cpu.ds = 0x1000;
    emu.cpu.es = 0x2000;

    // Write value at ES:0x500
    emu.write_u16(0x2000, 0x500, 0xABCD);

    // 26 A1 00 05 = ES: MOV AX, [0x500]
    emu.load_code_at(0, 0x100, &[0x26, 0xA1, 0x00, 0x05, 0xF4]);
    emu.run(100);

    assert_eq!(emu.cpu.ax, 0xABCD);
}

// ============================================================================
// REP MOVSW AND DECOMPRESSION TESTS
// ============================================================================

#[test]
fn test_rep_movsw_copy() {
    // Test REP MOVSW copies data correctly
    let mut emu = Emulator::new();
    emu.cpu.cs = 0;
    emu.cpu.ip = 0x100;
    emu.cpu.ds = 0x1000;  // Source segment
    emu.cpu.es = 0x2000;  // Dest segment
    emu.cpu.si = 0x0000;  // Source offset
    emu.cpu.di = 0x0000;  // Dest offset
    emu.cpu.cx = 4;       // Copy 4 words = 8 bytes

    // Write test data at DS:SI
    let test_data = [0x1E, 0x06, 0x1F, 0xF3, 0xA4, 0x1F, 0x5E, 0xE9];
    for (i, &b) in test_data.iter().enumerate() {
        emu.write_u8(0x1000, i as u16, b);
    }

    // Code: CLD; REP MOVSW; HLT
    emu.load_code_at(0, 0x100, &[0xFC, 0xF3, 0xA5, 0xF4]);
    emu.run(100);

    // Verify copy
    for (i, &expected) in test_data.iter().enumerate() {
        let actual = emu.read_u8(0x2000, i as u16);
        assert_eq!(actual, expected, "Mismatch at offset {}: expected 0x{:02X}, got 0x{:02X}", i, expected, actual);
    }

    // Verify SI and DI advanced
    assert_eq!(emu.cpu.si, 8, "SI should advance by 8");
    assert_eq!(emu.cpu.di, 8, "DI should advance by 8");
    assert_eq!(emu.cpu.cx, 0, "CX should be 0 after REP");
}

#[test]
fn test_jmp_rel16_forward() {
    // Test JMP rel16 with positive displacement
    // Code at 0x100: JMP +4 -> should go to 0x107
    // IP after JMP fetch = 0x103, so 0x103 + 4 = 0x107
    let mut emu = emu_with_code(&[
        0xE9, 0x04, 0x00,  // JMP rel16 +4 (to 0x107)
        0xB8, 0x11, 0x00,  // 0x103: MOV AX, 0x11 (skipped)
        0xF4,              // 0x106: HLT (skipped)
        0xB8, 0x22, 0x00,  // 0x107: MOV AX, 0x22 (target)
        0xF4               // 0x10A: HLT
    ]);
    emu.run(100);
    assert_eq!(emu.cpu.ax, 0x22, "JMP rel16 forward failed");
}

#[test]
fn test_jmp_rel16_backward() {
    // Test JMP rel16 with negative displacement
    // Code at 0x100: MOV AX, 1; HLT
    // Code at 0x104: JMP back to 0x100 (rel = 0xFFFA = -6)
    let mut emu = Emulator::new();
    emu.cpu.cs = 0;
    emu.cpu.ip = 0x104;  // Start at JMP
    emu.cpu.ax = 0;

    emu.load_code_at(0, 0x100, &[
        0xB8, 0x01, 0x00,  // 0x100: MOV AX, 1
        0xF4,              // 0x103: HLT
        0xE9, 0xF9, 0xFF,  // 0x104: JMP rel16 (0xFFF9 = -7, target = 0x107 + (-7) = 0x100)
    ]);
    emu.run(100);
    assert_eq!(emu.cpu.ax, 1, "JMP rel16 backward failed");
    assert_eq!(emu.cpu.ip, 0x104, "Should halt at 0x104 (instruction after MOV)");
}

#[test]
fn test_pklite_stub_simulation() {
    // Simulate what PKLITE does:
    // 1. REP MOVSW to copy compressed data to another segment
    // 2. RETF to jump to the copied code
    // 3. Execute the copied code which includes JMP rel16

    let mut emu = Emulator::new();
    emu.cpu.cs = 0x0010;
    emu.cpu.ip = 0x100;
    emu.cpu.ss = 0x3000;
    emu.cpu.sp = 0x200;

    // The "compressed" data that gets copied (simulating PKLITE payload)
    // This will be at DS:0x200 and copied to ES:0x0000
    // After copy, we RETF to ES:0x0000
    let decompressed_code = [
        // 0x0000: PUSH DS; PUSH ES; POP DS; REP MOVSB (but CX=0); POP DS; POP SI; JMP +4
        0x1E,              // 0x00: PUSH DS
        0x06,              // 0x01: PUSH ES
        0x1F,              // 0x02: POP DS
        0xF3, 0xA4,        // 0x03: REP MOVSB (does nothing if CX=0)
        0x1F,              // 0x05: POP DS
        0x5E,              // 0x06: POP SI
        0xE9, 0x04, 0x00,  // 0x07: JMP rel16 +4 -> to offset 0x0E (IP after=0x0A, 0x0A+4=0x0E)
        0xB8, 0xAA, 0x00,  // 0x0A: MOV AX, 0xAA (skipped)
        0xF4,              // 0x0D: HLT (skipped)
        0xB8, 0xBB, 0x00,  // 0x0E: MOV AX, 0xBB (target)
        0xF4               // 0x11: HLT
    ];

    // Write the "compressed" data at 0x0010:0x200
    for (i, &b) in decompressed_code.iter().enumerate() {
        emu.write_u8(0x0010, 0x200 + i as u16, b);
    }

    // Set up segments for copy
    emu.cpu.ds = 0x0010;
    emu.cpu.si = 0x200;
    emu.cpu.es = 0x2000;  // Target segment
    emu.cpu.di = 0x0000;
    emu.cpu.cx = (decompressed_code.len() / 2) as u16;

    // Push return address for RETF: 0x2000:0x0000
    emu.cpu.sp = emu.cpu.sp.wrapping_sub(2);
    emu.write_u16(emu.cpu.ss, emu.cpu.sp, 0x2000);  // CS
    emu.cpu.sp = emu.cpu.sp.wrapping_sub(2);
    emu.write_u16(emu.cpu.ss, emu.cpu.sp, 0x0000);  // IP

    // Code at 0x0010:0x100: CLD; REP MOVSW; RETF
    emu.load_code_at(0x0010, 0x100, &[
        0xFC,              // CLD
        0xF3, 0xA5,        // REP MOVSW
        0xCB               // RETF
    ]);

    emu.run(200);

    // Verify the data was copied correctly
    for (i, &expected) in decompressed_code.iter().enumerate() {
        let actual = emu.read_u8(0x2000, i as u16);
        assert_eq!(actual, expected,
            "Copied data mismatch at offset {}: expected 0x{:02X}, got 0x{:02X}",
            i, expected, actual);
    }

    // After RETF, we execute at 0x2000:0x0000
    // The code should execute and reach the JMP, which should go to 0x000F
    // Then execute MOV AX, 0xBB; HLT
    assert_eq!(emu.cpu.ax, 0xBB,
        "After JMP rel16, AX should be 0xBB but got 0x{:04X}", emu.cpu.ax);
}

#[test]
fn test_setup_exe_bytes() {
    // Test that we correctly understand the SETUP.EXE byte layout
    // Source at file offset 0x1C4 (DS:SI = 0010:0144 when loaded)
    // First 16 bytes should be: 1E 06 1F F3 A4 1F 5E E9 75 FF D1 ED 4A 75 04 AD

    let expected_bytes: [u8; 16] = [
        0x1E, 0x06, 0x1F, 0xF3, 0xA4, 0x1F, 0x5E, 0xE9,
        0x75, 0xFF, 0xD1, 0xED, 0x4A, 0x75, 0x04, 0xAD
    ];

    // Verify JMP displacement calculation
    // JMP at offset 7: E9 75 FF
    // After fetching 3 bytes, IP = 7 + 3 = 10 = 0x000A
    // Displacement = 0xFF75 = -139 signed
    // Target IP = 0x000A + 0xFF75 = 0xFF7F (with 16-bit wraparound)
    let displacement = u16::from_le_bytes([expected_bytes[8], expected_bytes[9]]);
    let displacement_signed = displacement as i16;
    assert_eq!(displacement, 0xFF75, "JMP displacement should be 0xFF75");
    assert_eq!(displacement_signed, -139, "JMP displacement signed should be -139");

    let ip_after_jmp = 0x000A_u16;
    let target_ip = ip_after_jmp.wrapping_add(displacement);
    assert_eq!(target_ip, 0xFF7F, "JMP target IP should be 0xFF7F");

    // This is the problem: 0xFF7F is beyond the copied code (only 0x246 bytes)
    // The JMP is jumping backwards to an address that wraps around
    // In a real DOS environment, this would access memory at ES:FF7F
    // which might contain code from the original loaded EXE or PKLITE stub
}

#[test]
fn test_rep_movsw_with_setup_exe_pattern() {
    // Simulate the PKLITE copy: REP MOVSW copying 0x123 words from source to dest
    let mut emu = Emulator::new();
    emu.cpu.cs = 0x0010;
    emu.cpu.ip = 0x100;
    emu.cpu.ds = 0x0010;  // Source segment
    emu.cpu.es = 0x2619;  // Dest segment (like PKLITE calculates)
    emu.cpu.si = 0x0144;  // Source offset
    emu.cpu.di = 0x0000;  // Dest offset
    emu.cpu.cx = 0x0123;  // Count (582 bytes)
    emu.cpu.ss = 0x263E;
    emu.cpu.sp = 0x0200;

    // Write the first 16 bytes of source data (from SETUP.EXE offset 0x1C4)
    let source_data: [u8; 16] = [
        0x1E, 0x06, 0x1F, 0xF3, 0xA4, 0x1F, 0x5E, 0xE9,
        0x75, 0xFF, 0xD1, 0xED, 0x4A, 0x75, 0x04, 0xAD
    ];

    // Write source data at DS:SI (0x0010:0x0144)
    for (i, &b) in source_data.iter().enumerate() {
        emu.write_u8(0x0010, 0x0144 + i as u16, b);
    }
    // Fill remaining with pattern bytes for testing
    for i in 16..0x246 {
        emu.write_u8(0x0010, 0x0144 + i as u16, (i & 0xFF) as u8);
    }

    // Code: CLD; REP MOVSW; HLT
    emu.load_code_at(0x0010, 0x100, &[0xFC, 0xF3, 0xA5, 0xF4]);
    emu.run(1000);

    // Verify first 16 bytes were copied correctly
    for (i, &expected) in source_data.iter().enumerate() {
        let actual = emu.read_u8(0x2619, i as u16);
        assert_eq!(actual, expected,
            "Byte at offset {} mismatch: expected 0x{:02X}, got 0x{:02X}",
            i, expected, actual);
    }

    // Verify SI and DI advanced correctly
    assert_eq!(emu.cpu.si, 0x0144 + 0x246, "SI should advance by 0x246 bytes");
    assert_eq!(emu.cpu.di, 0x0246, "DI should advance by 0x246 bytes");
    assert_eq!(emu.cpu.cx, 0, "CX should be 0 after REP");
}

#[test]
fn test_memory_is_zeroed() {
    // Verify that emulator memory starts completely zeroed
    let emu = Emulator::new();

    // Check first 256 bytes
    for i in 0..256 {
        assert_eq!(emu.memory[i], 0, "Memory at offset {} should be 0", i);
    }

    // Check some random higher addresses
    assert_eq!(emu.memory[0x1000], 0);
    assert_eq!(emu.memory[0x10000], 0);
    assert_eq!(emu.memory[0x50000], 0);
    assert_eq!(emu.memory[emu.memory.len() - 1], 0);
}

#[test]
fn test_load_com_zeroes_rest() {
    // Load a COM file and verify memory outside the loaded area is still zero
    let mut emu = Emulator::new();

    let com_data = [0xB8, 0x00, 0x4C, 0xCD, 0x21]; // MOV AX,4C00; INT 21
    emu.load_com(&com_data);

    // Verify data is loaded at 0x100
    assert_eq!(emu.memory[0x100], 0xB8);
    assert_eq!(emu.memory[0x101], 0x00);
    assert_eq!(emu.memory[0x104], 0x21);

    // Verify memory before 0x100 is still 0
    for i in 0..0x100 {
        assert_eq!(emu.memory[i], 0, "Memory at offset {} should be 0", i);
    }

    // Verify memory after loaded data is still 0
    for i in 0x105..0x200 {
        assert_eq!(emu.memory[i], 0, "Memory at offset {} should be 0", i);
    }
}
