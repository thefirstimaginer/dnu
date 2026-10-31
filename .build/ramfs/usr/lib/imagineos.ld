ENTRY(_start)

PHDRS
{
    text PT_LOAD FLAGS(5);
    rodata PT_LOAD FLAGS(4);
    data PT_LOAD FLAGS(6);
}

SECTIONS
{
    . = 0x400000;
    .text : ALIGN(4096) { *(.text .text.*) } :text
    .rodata : ALIGN(4096) { *(.rodata .rodata.*) } :rodata
    .data : ALIGN(4096) { *(.got .got.*) *(.data .data.*) } :data
    .bss : ALIGN(4096) { *(COMMON) *(.bss .bss.*) } :data

    /DISCARD/ : { *(.eh_frame .eh_frame_hdr) }
}
