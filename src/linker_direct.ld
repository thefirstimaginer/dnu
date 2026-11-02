# Linker script para boot direto (dnuboot).
#
# Linka o kernel em 0x100000 (identity-mapped, paging inicialmente desligado).
# O entry point é _direct_start. Inclui um segmento PT_NOTE com a seção
# .note.pvh exigida pelo QEMU para aceitar kernels x86_64 via -kernel.

ENTRY(_direct_start)
KERNEL_VMA = 0x100000;

PHDRS
{
    text PT_LOAD ;
    data PT_LOAD ;
    note PT_NOTE ;
}

SECTIONS
{
    . = KERNEL_VMA;

    .text : ALIGN(4096) { *(.text .text.*) } :text
    .rodata : ALIGN(4096) { *(.rodata .rodata.*) } :text
    .data : ALIGN(4096) { *(.got .got.*) *(.data .data.*) } :data
    .bss : ALIGN(4096) { *(COMMON) *(.bss .bss.*) } :data

    _initramfs_start = .;
    _initramfs_end = .;

    /DISCARD/ : { *(.eh_frame .eh_frame_hdr) }

    .note.pvh : { *(.note.pvh) } :note
}
