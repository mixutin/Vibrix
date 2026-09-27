ENTRY(vibrix_kernel_entry)

SECTIONS
{
    . = 0xFFFFFFFF80000000;

    .text : ALIGN(4096) {
        *(.text .text.*)
    }

    .rodata : ALIGN(4096) {
        *(.rodata .rodata.*)
    }

    /* GOT is writable and must not share a 4 KiB page with RO .rodata.
       The initial PT_LOAD mapper intentionally rejects conflicting page flags. */
    .got : ALIGN(4096) {
        *(.got .got.*)
    }

    .data : ALIGN(4096) {
        *(.data .data.*)
    }

    .bss : ALIGN(4096) {
        *(COMMON)
        *(.bss .bss.*)
    }

    /DISCARD/ : {
        *(.eh_frame*)
        *(.comment*)
    }
}
