/* WATOS Application Linker Script
 * Base address at 16MB to avoid kernel conflict at 1MB
 */

ENTRY(_start)

SECTIONS
{
    . = 0x1000000;  /* 16MB - safe from kernel at 0x100000 */

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
