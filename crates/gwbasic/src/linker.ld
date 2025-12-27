/* WATOS Application Linker Script */
ENTRY(_start)

SECTIONS
{
    /* Applications load at 2MB (kernel is at 1MB) */
    . = 0x200000;

    /* Code section */
    .text : {
        KEEP(*(.text._start))
        *(.text .text.*)
    }

    /* Read-only data */
    .rodata : {
        *(.rodata .rodata.*)
    }

    /* Initialized data */
    .data : {
        *(.data .data.*)
    }

    /* Uninitialized data */
    .bss : {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    }

    /* Stack at end - grows down */
    . = ALIGN(4096);
    __stack_bottom = .;
    . += 0x10000;  /* 64KB stack */
    __stack_top = .;

    /* Discard debug and other unnecessary sections */
    /DISCARD/ : {
        *(.comment)
        *(.note*)
        *(.eh_frame*)
    }
}
