; ECHO.COM - Display text to console
; Usage: ECHO <text>
; DOS INT 21h functions used:
;   AH=09h - Write string to stdout (DX=string address, terminated with '$')
;   AH=4Ch - Exit program (AL=return code)

    org 0x100           ; COM file format starts at 0x100

start:
    ; Get PSP command tail at offset 0x80
    ; PSP is at CS:0, command tail is at CS:0x80
    mov si, 0x81        ; SI points to command tail (skip length byte at 0x80)
    
    ; Skip leading spaces
skip_spaces:
    lodsb               ; Load byte from DS:SI into AL, increment SI
    cmp al, ' '
    je skip_spaces
    cmp al, 0x0D        ; Check for carriage return (end of command line)
    je print_newline    ; If empty command, just print newline
    
    ; Found first non-space character, back up one
    dec si
    
    ; Print each character until CR
print_loop:
    lodsb               ; Load byte from DS:SI into AL
    cmp al, 0x0D        ; Check for carriage return
    je print_newline
    
    ; Print character using INT 21h/02h
    mov dl, al
    mov ah, 0x02        ; Function 02h - Write character to stdout
    int 0x21
    jmp print_loop

print_newline:
    ; Print CR+LF
    mov dl, 0x0D
    mov ah, 0x02
    int 0x21
    mov dl, 0x0A
    mov ah, 0x02
    int 0x21

exit:
    ; Exit to DOS
    mov ax, 0x4C00      ; AH=4Ch (terminate), AL=00 (return code)
    int 0x21
