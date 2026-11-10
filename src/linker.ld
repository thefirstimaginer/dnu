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

    .text : ALIGN(4096) { 
        *(.text .text.*) 
        *(.text.*) 
    } :text
    .rodata : ALIGN(4096) { *(.rodata .rodata.*) } :rodata
    .data : ALIGN(4096) { 
        *(.data.early .data.early.*) 
        *(.got .got.*) 
        *(.data .data.*) 
        _edata = .; 
        /* Ensure the entire .data segment is mapped by forcing p_filesz = p_memsz */
        .fill 0x200000, 1, 0
    } :data
    .bss : ALIGN(4096) { 
        *(COMMON) *(.bss .bss.*) 
        _ebss = .; 
        /* Ensure the heap (HEAP_SPACE in .bss) is mapped by forcing p_filesz = p_memsz */
        .fill 0x400000, 1, 0
    } :bss

    /DISCARD/ : { *(.eh_frame .eh_frame_hdr) }
}
