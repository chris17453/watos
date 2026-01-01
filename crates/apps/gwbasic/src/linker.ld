/* WATOS Application Linker Script */
ENTRY(_start)

SECTIONS
{
    /* Applications load at 16MB - outside kernel's identity-mapped region (0-8MB) */
    . = 0x1000000;

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
