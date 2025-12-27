; CAT.COM - Concatenate and display files
; Usage: CAT <file1> [file2] [file3] ...
; DOS INT 21h functions:
;   AH=3Dh - Open file
;   AH=3Eh - Close file
;   AH=3Fh - Read from file
;   AH=02h - Write character
;   AH=09h - Write string
;   AH=4Ch - Exit

    org 0x100

start:
    ; Parse command line
    mov si, 0x81
    
next_file:
    ; Skip leading spaces
skip_spaces:
    lodsb
    cmp al, ' '
    je skip_spaces
    cmp al, 0x0D
    je exit_success     ; No more files
    dec si
    
    ; Copy filename
    mov di, filename
copy_name:
    lodsb
    cmp al, ' '
    je found_end
    cmp al, 0x0D
    je last_file
    stosb
    jmp copy_name
    
found_end:
    mov byte [di], 0
    push si             ; Save position for next file
    jmp process_file
    
last_file:
    mov byte [di], 0
    push si
    
process_file:
    ; Open file
    mov ax, 0x3D00
    mov dx, filename
    int 0x21
    jc open_error
    mov [file_handle], ax
    
read_loop:
    ; Read a block
    mov ah, 0x3F
    mov bx, [file_handle]
    mov cx, 512
    mov dx, buffer
    int 0x21
    jc read_error
    
    ; Check if EOF
    cmp ax, 0
    je close_file
    
    ; Save number of bytes read
    mov [bytes_read], ax
    
    ; Display the buffer
    mov cx, [bytes_read]
    mov si, buffer
display_loop:
    lodsb
    mov dl, al
    mov ah, 0x02
    int 0x21
    loop display_loop
    
    jmp read_loop

close_file:
    ; Close file
    mov ah, 0x3E
    mov bx, [file_handle]
    int 0x21
    
    ; Restore position and check for more files
    pop si
    mov al, [si-1]      ; Check what we stopped at
    cmp al, 0x0D
    je exit_success     ; Was last file
    jmp next_file       ; More files to process

exit_success:
    mov ax, 0x4C00
    int 0x21

open_error:
    ; Print error with filename
    mov ah, 0x09
    mov dx, msg_error
    int 0x21
    
    mov ah, 0x09
    mov dx, filename
    ; Find end of filename and add $ terminator temporarily
    mov di, filename
find_end:
    cmp byte [di], 0
    je end_found
    inc di
    jmp find_end
end_found:
    mov byte [di], '$'
    int 0x21
    mov byte [di], 0    ; Restore null terminator
    
    ; Print newline
    mov dl, 0x0D
    mov ah, 0x02
    int 0x21
    mov dl, 0x0A
    int 0x21
    
    ; Continue with next file if any
    pop si
    mov al, [si-1]
    cmp al, 0x0D
    je exit_error
    jmp next_file

read_error:
    mov ah, 0x3E
    mov bx, [file_handle]
    int 0x21
    
    mov ah, 0x09
    mov dx, msg_read_err
    int 0x21
    jmp exit_error

exit_error:
    mov ax, 0x4C01
    int 0x21

; Data section
msg_error       db 'Error: Cannot open file: $'
msg_read_err    db 'Error: Read failed',0x0D,0x0A,'$'

file_handle     dw 0
bytes_read      dw 0
filename        times 64 db 0
buffer          times 512 db 0
