; DEL.COM - Delete files
; Usage: DEL <filename>
; DOS INT 21h functions:
;   AH=41h - Delete file (DS:DX=filename)
;   AH=09h - Write string
;   AH=4Ch - Exit

    org 0x100

start:
    ; Parse command line to get filename
    mov si, 0x81        ; Command tail
    
    ; Skip leading spaces
skip_spaces:
    lodsb
    cmp al, ' '
    je skip_spaces
    cmp al, 0x0D
    je usage_error
    dec si
    
    ; Copy filename to buffer
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
    mov byte [di], 0    ; Null-terminate filename
    
    ; Delete the file
    mov ah, 0x41        ; Delete file
    mov dx, filename
    int 0x21
    jc delete_error
    
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

delete_error:
    mov ah, 0x09
    mov dx, msg_error
    int 0x21
    mov ax, 0x4C01
    int 0x21

; Data section
msg_usage       db 'Usage: DEL <filename>',0x0D,0x0A,'$'
msg_success     db 'File deleted successfully',0x0D,0x0A,'$'
msg_error       db 'Error: Cannot delete file',0x0D,0x0A,'$'

filename        times 64 db 0
