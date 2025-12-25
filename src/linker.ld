/* WATOS Kernel Linker Script */
ENTRY(_start)

SECTIONS
{
    /* Load at 1MB */
    . = 0x100000;

    /* Entry point first */
    .text.boot : {
        KEEP(*(.text._start))
        KEEP(*(.text.kernel_main))
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

    /* Discard debug and other unnecessary sections */
    /DISCARD/ : {
        *(.comment)
        *(.note*)
        *(.eh_frame*)
    }
}
