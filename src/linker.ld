ENTRY(_start)
KERNEL_VMA = 0xffffffff80000000;
KERNEL_LMA = 0x100000;

SECTIONS
{
    . = KERNEL_VMA + KERNEL_LMA;

    .requests : AT(ADDR(.requests) - KERNEL_VMA) ALIGN(8) {
        KEEP(*(.requests_start_marker))
        KEEP(*(.requests))
        KEEP(*(.requests_end_marker))
    }

    .text : AT(ADDR(.text) - KERNEL_VMA) ALIGN(4096) { *(.text .text.*) }
    .rodata : AT(ADDR(.rodata) - KERNEL_VMA) ALIGN(4096) { *(.rodata .rodata.*) }
    .data : AT(ADDR(.data) - KERNEL_VMA) ALIGN(4096) { *(.got .got.*) *(.data .data.*) }
    .bss : AT(ADDR(.bss) - KERNEL_VMA) ALIGN(4096) { *(COMMON) *(.bss .bss.*) }

    /DISCARD/ : { *(.eh_frame .eh_frame_hdr) }
}