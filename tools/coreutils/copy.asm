; COPY.COM - Copy files
; Usage: COPY <source> <dest>
; DOS INT 21h functions:
;   AH=3Dh - Open file (AL=mode, DS:DX=filename)
;   AH=3Eh - Close file (BX=handle)
;   AH=3Fh - Read from file (BX=handle, CX=bytes, DS:DX=buffer)
;   AH=40h - Write to file (BX=handle, CX=bytes, DS:DX=buffer)
;   AH=3Ch - Create file (CX=attributes, DS:DX=filename)
;   AH=09h - Write string (DS:DX=string$)
;   AH=4Ch - Exit

    org 0x100

start:
    ; Parse command line to get source and dest filenames
    mov si, 0x81        ; Command tail
    
    ; Skip leading spaces
skip_spaces1:
    lodsb
    cmp al, ' '
    je skip_spaces1
    cmp al, 0x0D
    je usage_error
    dec si
    
    ; Copy source filename to buffer
    mov di, source_file
copy_source:
    lodsb
    cmp al, ' '
    je found_space1
    cmp al, 0x0D
    je usage_error
    stosb
    jmp copy_source
    
found_space1:
    mov byte [di], 0    ; Null-terminate source
    
    ; Skip spaces between filenames
skip_spaces2:
    lodsb
    cmp al, ' '
    je skip_spaces2
    cmp al, 0x0D
    je usage_error
    dec si
    
    ; Copy dest filename to buffer
    mov di, dest_file
copy_dest:
    lodsb
    cmp al, ' '
    je found_space2
    cmp al, 0x0D
    je found_space2
    stosb
    jmp copy_dest
    
found_space2:
    mov byte [di], 0    ; Null-terminate dest
    
    ; Open source file for reading
    mov ax, 0x3D00      ; AH=3Dh (open), AL=00 (read mode)
    mov dx, source_file
    int 0x21
    jc open_error
    mov [source_handle], ax
    
    ; Create destination file
    mov ah, 0x3C        ; Create file
    mov cx, 0           ; Normal attributes
    mov dx, dest_file
    int 0x21
    jc create_error
    mov [dest_handle], ax
    
    ; Copy loop
copy_loop:
    ; Read from source
    mov ah, 0x3F        ; Read from file
    mov bx, [source_handle]
    mov cx, 512         ; Read 512 bytes
    mov dx, buffer
    int 0x21
    jc read_error
    
    ; Check if EOF (AX=0)
    cmp ax, 0
    je copy_done
    
    ; Write to dest
    mov [bytes_read], ax
    mov ah, 0x40        ; Write to file
    mov bx, [dest_handle]
    mov cx, [bytes_read]
    mov dx, buffer
    int 0x21
    jc write_error
    
    jmp copy_loop
    
copy_done:
    ; Close files
    mov ah, 0x3E        ; Close file
    mov bx, [source_handle]
    int 0x21
    
    mov ah, 0x3E
    mov bx, [dest_handle]
    int 0x21
    
    ; Print success message
    mov ah, 0x09
    mov dx, msg_success
    int 0x21
    
    ; Exit
    mov ax, 0x4C00
    int 0x21

usage_error:
    mov ah, 0x09
    mov dx, msg_usage
    int 0x21
    mov ax, 0x4C01
    int 0x21

open_error:
    mov ah, 0x09
    mov dx, msg_open_err
    int 0x21
    mov ax, 0x4C01
    int 0x21

create_error:
    ; Close source file first
    mov ah, 0x3E
    mov bx, [source_handle]
    int 0x21
    
    mov ah, 0x09
    mov dx, msg_create_err
    int 0x21
    mov ax, 0x4C01
    int 0x21

read_error:
    ; Close both files
    mov ah, 0x3E
    mov bx, [source_handle]
    int 0x21
    mov ah, 0x3E
    mov bx, [dest_handle]
    int 0x21
    
    mov ah, 0x09
    mov dx, msg_read_err
    int 0x21
    mov ax, 0x4C01
    int 0x21

write_error:
    ; Close both files
    mov ah, 0x3E
    mov bx, [source_handle]
    int 0x21
    mov ah, 0x3E
    mov bx, [dest_handle]
    int 0x21
    
    mov ah, 0x09
    mov dx, msg_write_err
    int 0x21
    mov ax, 0x4C01
    int 0x21

; Data section
msg_usage       db 'Usage: COPY <source> <dest>',0x0D,0x0A,'$'
msg_success     db 'File copied successfully',0x0D,0x0A,'$'
msg_open_err    db 'Error: Cannot open source file',0x0D,0x0A,'$'
msg_create_err  db 'Error: Cannot create destination file',0x0D,0x0A,'$'
msg_read_err    db 'Error: Read failed',0x0D,0x0A,'$'
msg_write_err   db 'Error: Write failed',0x0D,0x0A,'$'

source_handle   dw 0
dest_handle     dw 0
bytes_read      dw 0
source_file     times 64 db 0
dest_file       times 64 db 0
buffer          times 512 db 0
