/* Linker script for WATOS native applications */

ENTRY(_start)

SECTIONS
{
    /* Start at 1MB (standard for ELF executables) */
    . = 0x100000;

    .text : {
        *(.text._start)
        *(.text .text.*)
    }

    .rodata : {
        *(.rodata .rodata.*)
    }

    .data : {
        *(.data .data.*)
    }

    .bss : {
        *(.bss .bss.*)
        *(COMMON)
    }

    /DISCARD/ : {
        *(.eh_frame)
        *(.note.GNU-stack)
    }
}
