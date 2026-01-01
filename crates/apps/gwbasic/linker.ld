/* Linker script for WATOS user programs */
/* Load at 0x1000000 (16MB) - outside kernel's identity-mapped region (0-8MB) */

ENTRY(_start)

SECTIONS
{
    . = 0x1000000;

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
        *(COMMON)
    }

    /DISCARD/ : {
        *(.eh_frame)
        *(.note*)
        *(.comment*)
    }
}
