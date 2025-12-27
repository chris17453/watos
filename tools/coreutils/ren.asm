; REN.COM - Rename files
; Usage: REN <oldname> <newname>
; DOS INT 21h functions:
;   AH=56h - Rename file (DS:DX=old name, ES:DI=new name)
;   AH=09h - Write string
;   AH=4Ch - Exit

    org 0x100

start:
    ; Parse command line
    mov si, 0x81        ; Command tail
    
    ; Skip leading spaces
skip_spaces1:
    lodsb
    cmp al, ' '
    je skip_spaces1
    cmp al, 0x0D
    je usage_error
    dec si
    
    ; Copy old filename
    mov di, oldname
copy_old:
    lodsb
    cmp al, ' '
    je found_space
    cmp al, 0x0D
    je usage_error
    stosb
    jmp copy_old
    
found_space:
    mov byte [di], 0    ; Null-terminate
    
    ; Skip spaces between filenames
skip_spaces2:
    lodsb
    cmp al, ' '
    je skip_spaces2
    cmp al, 0x0D
    je usage_error
    dec si
    
    ; Copy new filename
    mov di, newname
copy_new:
    lodsb
    cmp al, ' '
    je found_end
    cmp al, 0x0D
    je found_end
    stosb
    jmp copy_new
    
found_end:
    mov byte [di], 0    ; Null-terminate
    
    ; Rename the file
    push ds
    pop es              ; ES = DS for new name
    mov ah, 0x56        ; Rename file
    mov dx, oldname     ; DS:DX = old name
    mov di, newname     ; ES:DI = new name
    int 0x21
    jc rename_error
    
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

rename_error:
    mov ah, 0x09
    mov dx, msg_error
    int 0x21
    mov ax, 0x4C01
    int 0x21

; Data section
msg_usage       db 'Usage: REN <oldname> <newname>',0x0D,0x0A,'$'
msg_success     db 'File renamed successfully',0x0D,0x0A,'$'
msg_error       db 'Error: Cannot rename file',0x0D,0x0A,'$'

oldname         times 64 db 0
newname         times 64 db 0
