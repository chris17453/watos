//! WATOS EXE Tester - CLI tool for debugging WATOS ELF64 executables

use std::fs;
use std::env;

#[repr(C)]
struct Elf64Header {
    magic: [u8; 4], class: u8, endian: u8, version: u8, os_abi: u8,
    abi_version: u8, _padding: [u8; 7], etype: u16, machine: u16,
    version2: u32, entry: u64, phoff: u64, shoff: u64, flags: u32,
    ehsize: u16, phentsize: u16, phnum: u16, shentsize: u16,
    shnum: u16, shstrndx: u16,
}

#[repr(C)]
struct Elf64Phdr {
    ptype: u32, flags: u32, offset: u64, vaddr: u64, paddr: u64,
    filesz: u64, memsz: u64, align: u64,
}

const PT_LOAD: u32 = 1;
const PF_X: u32 = 1;
const PF_W: u32 = 2;
const PF_R: u32 = 4;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <exe-file> [--dump-entry] [--check-syscalls]", args[0]);
        std::process::exit(1);
    }
    
    let file_path = &args[1];
    let dump_entry = args.contains(&"--dump-entry".to_string());
    let check_syscalls = args.contains(&"--check-syscalls".to_string());

    println!("\n═══════════════════════════════════════");
    println!("     WATOS EXE Tester v0.1");
    println!("═══════════════════════════════════════\n");

    let data = fs::read(file_path).unwrap_or_else(|e| {
        eprintln!("❌ Error reading file: {}", e);
        std::process::exit(1);
    });

    println!("�� File: {}", file_path);
    println!("📊 Size: {} bytes\n", data.len());

    if data.len() < std::mem::size_of::<Elf64Header>() {
        eprintln!("❌ File too small");
        std::process::exit(1);
    }

    let header = unsafe { &*(data.as_ptr() as *const Elf64Header) };

    if &header.magic != b"\x7FELF" {
        eprintln!("❌ Invalid ELF magic");
        std::process::exit(1);
    }
    println!("✓ Valid ELF64");

    if header.class != 2 || header.machine != 0x3E {
        eprintln!("❌ Not 64-bit x86-64");
        std::process::exit(1);
    }
    println!("✓ x86-64 architecture");

    println!("🎯 Entry: 0x{:016x}", header.entry);
    println!("📑 Segments: {}\n", header.phnum);

    if dump_entry {
        println!("═══ Entry Point Dump ═══");
        let phdr_offset = header.phoff as usize;
        let phdr_size = std::mem::size_of::<Elf64Phdr>();
        
        for i in 0..header.phnum as usize {
            let offset = phdr_offset + i * phdr_size;
            if offset + phdr_size > data.len() { continue; }
            let phdr = unsafe { &*(data[offset..].as_ptr() as *const Elf64Phdr) };
            
            if phdr.ptype == PT_LOAD {
                let seg_start = phdr.vaddr;
                let seg_end = phdr.vaddr + phdr.memsz;
                
                if header.entry >= seg_start && header.entry < seg_end {
                    let entry_offset = (header.entry - phdr.vaddr + phdr.offset) as usize;
                    if entry_offset < data.len() {
                        let dump_size = std::cmp::min(64, data.len() - entry_offset);
                        for j in 0..dump_size {
                            if j % 16 == 0 { print!("\n  {:04x}: ", j); }
                            print!("{:02x} ", data[entry_offset + j]);
                        }
                        println!("\n");
                    }
                    break;
                }
            }
        }
    }

    if check_syscalls {
        println!("═══ Syscall Check ═══");
        let mut found = false;
        let phdr_offset = header.phoff as usize;
        let phdr_size = std::mem::size_of::<Elf64Phdr>();
        
        for i in 0..header.phnum as usize {
            let offset = phdr_offset + i * phdr_size;
            if offset + phdr_size > data.len() { continue; }
            let phdr = unsafe { &*(data[offset..].as_ptr() as *const Elf64Phdr) };
            
            if phdr.ptype == PT_LOAD && (phdr.flags & PF_X != 0) {
                let start = phdr.offset as usize;
                let end = std::cmp::min(start + phdr.filesz as usize, data.len());
                for j in start..end-1 {
                    if data[j] == 0xCD && data[j+1] == 0x80 {
                        println!("✓ INT 0x80 at offset 0x{:x}", j);
                        found = true;
                    }
                }
            }
        }
        if !found { println!("⚠️  No syscalls found"); }
    }

    println!("\n═══════════════════════════════════════");
    println!("✅ Validation complete");
    println!("═══════════════════════════════════════\n");
}
