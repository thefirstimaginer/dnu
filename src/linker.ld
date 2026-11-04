ENTRY(_start)
KERNEL_VMA = 0xffffffff80000000;

PHDRS
{
    text PT_LOAD FLAGS(5);        /* R X - código executável */
    rodata PT_LOAD FLAGS(4);      /* R   - dados somente leitura */
    data PT_LOAD FLAGS(6);        /* R W - dados graváveis */
    bss PT_LOAD FLAGS(6);         /* R W - dados zero-init (.bss) */
}

SECTIONS
{
    . = KERNEL_VMA;

    .text : ALIGN(4096) { *(.text .text.*) } :text
    .rodata : ALIGN(4096) { *(.rodata .rodata.*) } :rodata
    .data : ALIGN(4096) { 
        *(.got .got.*) 
        *(.data .data.*) 
        _edata = .; 
        /* Force the allocator into .data section */
        . = ALIGN(4096);
        *(.data.ALLOCATOR)
    } :data
    .bss : ALIGN(4096) { *(COMMON) *(.bss .bss.*) _ebss = .; } :bss
    .bss_pad : ALIGN(4096) { . = _ebss + 0x1000000; _end = .; } :bss

    /DISCARD/ : { *(.eh_frame .eh_frame_hdr) }
}