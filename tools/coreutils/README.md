# WATOS Core Utilities

This directory contains basic DOS utilities written in x86 assembly for the WATOS operating system.

## Building

Prerequisites:
- NASM (Netwide Assembler)

To build all utilities:
```bash
cd tools/coreutils
make
```

This will create .COM files in `rootfs/BIN/`.

To clean:
```bash
make clean
```

## Utilities

### ECHO.COM
Display text to console.

**Usage:** `ECHO <text>`

**Example:**
```
ECHO Hello, World!
```

### COPY.COM
Copy files from source to destination.

**Usage:** `COPY <source> <dest>`

**Example:**
```
COPY file1.txt file2.txt
```

### DEL.COM
Delete a file.

**Usage:** `DEL <filename>`

**Example:**
```
DEL oldfile.txt
```

### REN.COM
Rename a file.

**Usage:** `REN <oldname> <newname>`

**Example:**
```
REN old.txt new.txt
```

### MORE.COM
Display file contents with paging (23 lines at a time).

**Usage:** `MORE <filename>`

**Example:**
```
MORE longfile.txt
```

Press any key to continue to the next page.

### CAT.COM
Concatenate and display files.

**Usage:** `CAT <file1> [file2] [file3] ...`

**Examples:**
```
CAT file.txt
CAT file1.txt file2.txt file3.txt
```

## Implementation Notes

All utilities are written as DOS .COM files using the flat binary format:
- Load address: 0x100 (DOS COM format)
- Maximum size: ~64KB minus PSP (Program Segment Prefix)
- Use DOS INT 21h for system calls

### DOS INT 21h Functions Used

- **AH=02h** - Write character to stdout
- **AH=09h** - Write string to stdout ($ terminated)
- **AH=3Ch** - Create file
- **AH=3Dh** - Open file
- **AH=3Eh** - Close file
- **AH=3Fh** - Read from file
- **AH=40h** - Write to file
- **AH=41h** - Delete file
- **AH=56h** - Rename file
- **AH=4Ch** - Exit program

### DOS INT 16h Functions Used

- **AH=00h** - Read keyboard (blocking)

## Testing

After building, test the utilities in WATOS:

1. Build WATOS: `./scripts/build.sh`
2. Run WATOS: `./scripts/boot_test.sh --interactive`
3. In WATOS shell:
   ```
   ECHO Hello from WATOS!
   COPY HELLO.COM TEST.COM
   CAT HELLO.COM
   MORE HELLO.COM
   REN TEST.COM RENAMED.COM
   DEL RENAMED.COM
   ```

## Adding New Utilities

1. Create a new `.asm` file in this directory
2. Use the DOS .COM format (`org 0x100`)
3. Implement using DOS INT 21h functions
4. The Makefile will automatically build it

## Future Utilities

Planned utilities to add:
- MKDIR - Create directories
- RMDIR - Remove directories
- CD - Change directory
- DATE - Display/set date
- TIME - Display/set time
- SORT - Sort text files
- FIND - Search for text in files
- COMP - Compare files
