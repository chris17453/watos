; MORE.COM - Display file contents with paging
; Usage: MORE <filename>
; DOS INT 21h functions:
;   AH=3Dh - Open file
;   AH=3Eh - Close file
;   AH=3Fh - Read from file
;   AH=02h - Write character
;   AH=09h - Write string
;   AH=4Ch - Exit

    org 0x100

LINES_PER_PAGE equ 23   ; Show 23 lines before pausing

start:
    ; Parse command line
    mov si, 0x81
    
skip_spaces:
    lodsb
    cmp al, ' '
    je skip_spaces
    cmp al, 0x0D
    je usage_error
    dec si
    
    ; Copy filename
    mov di, filename
copy_name:
    lodsb
    cmp al, ' '
    je found_end
    cmp al, 0x0D
    je found_end
    stosb
    jmp copy_name
    
found_end:
    mov byte [di], 0
    
    ; Open file
    mov ax, 0x3D00      ; Open for reading
    mov dx, filename
    int 0x21
    jc open_error
    mov [file_handle], ax
    
    ; Initialize line counter
    mov word [line_count], 0
    
read_loop:
    ; Read one character
    mov ah, 0x3F
    mov bx, [file_handle]
    mov cx, 1
    mov dx, char_buffer
    int 0x21
    jc read_error
    
    ; Check if EOF
    cmp ax, 0
    je done
    
    ; Get the character
    mov al, [char_buffer]
    
    ; Check for newline
    cmp al, 0x0A
    jne not_newline
    
    ; Increment line counter
    inc word [line_count]
    
    ; Check if we've shown enough lines
    mov ax, [line_count]
    cmp ax, LINES_PER_PAGE
    jb not_newline
    
    ; Pause and wait for key
    mov ah, 0x09
    mov dx, msg_more
    int 0x21
    
    ; Wait for any key (INT 16h, AH=00h - read keyboard)
    mov ah, 0x00
    int 0x16
    
    ; Clear the "-- More --" message
    mov ah, 0x09
    mov dx, msg_clear
    int 0x21
    
    ; Reset line counter
    mov word [line_count], 0
    
not_newline:
    ; Display the character
    mov dl, al
    mov ah, 0x02
    int 0x21
    
    jmp read_loop

done:
    ; Close file
    mov ah, 0x3E
    mov bx, [file_handle]
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

read_error:
    mov ah, 0x3E
    mov bx, [file_handle]
    int 0x21
    
    mov ah, 0x09
    mov dx, msg_read_err
    int 0x21
    mov ax, 0x4C01
    int 0x21

; Data section
msg_usage       db 'Usage: MORE <filename>',0x0D,0x0A,'$'
msg_open_err    db 'Error: Cannot open file',0x0D,0x0A,'$'
msg_read_err    db 'Error: Read failed',0x0D,0x0A,'$'
msg_more        db 0x0D,'-- More -- (press any key)',0x0D,0x0A,'$'
msg_clear       db 0x0D,'                           ',0x0D,'$'

file_handle     dw 0
line_count      dw 0
char_buffer     db 0
filename        times 64 db 0
