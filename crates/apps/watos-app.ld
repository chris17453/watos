/* WATOS Application Linker Script
 * All apps load at 0x400000 - proper virtual memory
 */

ENTRY(_start)

SECTIONS
{
    . = 0x400000;  /* All processes use same virtual address */

    .text.boot : {
        *(.text.boot)
    }

    .text : {
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
    }

    /DISCARD/ : {
        *(.eh_frame)
        *(.comment)
    }
}
