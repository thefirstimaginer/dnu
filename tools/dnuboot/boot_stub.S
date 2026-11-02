# boot_stub.S — Stub de boot direto para dnuboot
#
# Este stub é carregado pelo QEMU via `-kernel`. Ele:
#   1. Configura um ambiente mínimo (CLI, stack)
#   2. Localiza o kernel ELF e o initramfs embutidos nas seções do binário
#   3. Constrói um `BootInfo` com valores padrão no endereço BOOT_INFO_ADDR
#   4. Salta para `kernel_entry` com o ponteiro do BootInfo em rdi
#
# O dnuboot embute o kernel ELF na seção .rodata.kernel e o initramfs na
# seção .rodata.initramfs usando objcopy/ld.

# ============================================================================
# Constantes
# ============================================================================

.section .rodata
boot_info_addr:
    .quad 0x200000           # Endereço onde o BootInfo será construído

hhdm_offset_val:
    .quad 0xFFFF800000000000 # HHDM offset padrão

empty_reserved_addr:
    .quad 0x300000           # Endereço do PhysicalRange vazio (1 entrada)

dzimage_path:
    .asciz "dzImage"

# ============================================================================
# Código
# ============================================================================

.section .text
.global _start
.type _start, @function

_start:
    # --- Ambiente mínimo ---
    cli                         # Desabilita interrupções
    cld                         # Direção de string para frente

    # --- Stack temporário ---
    mov $0x7C00, %ax            # Stack logo abaixo do stub (0x7C00 -> 0x7C00-...)
    mov %ax, %ss
    mov $0x7FF0, %rsp           # Topo da stack

    # --- Localizar seções embutidas ---
    # Os símbolos __start_rodata_kernel e __stop_rodata_kernel são gerados pelo
    # linker para a seção .rodata.kernel (kernel ELF).
    # Os símbolos __start_rodata_initramfs e __stop_rodata_initramfs são
    # gerados para a seção .rodata.initramfs (initramfs).
    lea __start_rodata_kernel(%rip), %rax      # rax = início do kernel ELF
    lea __stop_rodata_kernel(%rip), %rbx       # rbx = fim do kernel ELF
    sub %rax, %rbx                             # rbx = tamanho do kernel
    mov %rbx, kernel_size(%rip)

    lea __start_rodata_initramfs(%rip), %rax   # rax = início do initramfs
    lea __stop_rodata_initramfs(%rip), %rbx    # rbx = fim do initramfs
    sub %rax, %rbx                             # rbx = tamanho do initramfs
    mov %rbx, initramfs_size(%rip)

    # Salvar endereços
    mov %rax, initramfs_addr(%rip)             # endereço do initramfs

    # --- Construir o DirectModuleResponse (módulo sintético) ---
    # Estrutura:
    #   modules: [DirectModule; 1]
    #   count: usize
    # DirectModule:
    #   path: [u8; 128]
    #   pad: [u8; 32]
    #   addr: u64
    #   size: u64

    mov $0x400000, %rax                        # Endereço do módulo sintético
    mov %rax, module_addr(%rip)

    # path: copiar "dzImage" (com zeros preenchendo)
    mov $0x400000, %rdi                        # Destino: path do módulo
    lea dzimage_path(%rip), %rsi               # Fonte: string "dzImage"
    mov $7, %rdx                               # Tamanho da string
    rep movsb                                  # Copiar path
    # Preencher o restante de path + pad com zeros (128 + 32 = 160 bytes)
    mov $160 - 7, %rcx
    xor %al, %al
    rep stosb

    # addr: endereço físico do initramfs
    mov initramfs_addr(%rip), %rax
    mov $0x400000 + 160, %rdi                  # offset de addr no DirectModule
    add %rax, %rdi                             # rdi = base do módulo + 160
    mov %rax, (%rdi)

    # size: tamanho do initramfs
    mov initramfs_size(%rip), %rax
    add $8, %rdi                               # offset de size
    mov %rax, (%rdi)

    # count: 1
    mov $1, %rax
    mov $0x400000 + 160 + 16, %rdi             # offset de count
    mov %rax, (%rdi)

    # --- Construir o EMPTY_RESERVED (PhysicalRange vazio) ---
    # PhysicalRange { start: 0, length: 0 }
    mov $0x300000, %rax                        # Endereço do reserved range
    xor %rax, %rax
    mov %rax, (0x300000)                       # start = 0
    mov %rax, (0x300008)                       # length = 0

    # --- Construir o BootInfo ---
    # BootInfo layout (repr(C)):
    #   memory_map: *const MemoryMapResponse   -> 0 (null)
    #   framebuffer: *const FramebufferResponse -> 0 (null)
    #   modules: *const ModuleResponse          -> &DirectModuleResponse
    #   rsdp_address: u64                       -> 0
    #   hhdm_offset: u64                        -> DIRECT_HHDM_OFFSET
    #   tsc_start: u64                          -> 0
    #   tsc_frequency: u64                      -> 0
    #   reserved_ranges: *const PhysicalRange   -> &EMPTY_RESERVED
    #   reserved_count: usize                   -> 1
    #   dzimage: *const u8                      -> endereço do initramfs
    #   dzimage_size: usize                     -> tamanho do initramfs

    mov $0x200000, %rax                        # Endereço do BootInfo

    # memory_map = 0 (offset 0)
    xor %rbx, %rbx
    mov %rbx, (%rax)

    # framebuffer = 0 (offset 8)
    mov %rbx, 8(%rax)

    # modules = &DirectModuleResponse (offset 16)
    mov $0x400000, %rbx
    mov %rbx, 16(%rax)

    # rsdp_address = 0 (offset 24)
    mov %rbx, 24(%rax)
    xor %rbx, %rbx
    mov %rbx, 24(%rax)

    # hhdm_offset = DIRECT_HHDM_OFFSET (offset 32)
    lea hhdm_offset_val(%rip), %rbx
    mov (%rbx), %rbx
    mov %rbx, 32(%rax)

    # tsc_start = 0 (offset 40)
    xor %rbx, %rbx
    mov %rbx, 40(%rax)

    # tsc_frequency = 0 (offset 48)
    mov %rbx, 48(%rax)

    # reserved_ranges = &EMPTY_RESERVED (offset 56)
    mov $0x300000, %rbx
    mov %rbx, 56(%rax)

    # reserved_count = 1 (offset 64)
    mov $1, %rbx
    mov %rbx, 64(%rax)

    # dzimage = endereço do initramfs (offset 72)
    mov initramfs_addr(%rip), %rbx
    mov %rbx, 72(%rax)

    # dzimage_size = tamanho do initramfs (offset 80)
    mov initramfs_size(%rip), %rbx
    mov %rbx, 80(%rax)

    # --- Saltar para kernel_entry ---
    # rdi = ponteiro para o BootInfo (primeiro argumento da ABI System V)
    mov $0x200000, %rdi

    # kernel_entry está no kernel ELF embutido. Precisamos pular para o
    # entry point do kernel. O símbolo kernel_entry é resolvido pelo linker
    # quando o kernel ELF é vinculado com este stub.
    jmp *kernel_entry(%rip)

    # Nunca deve chegar aqui
.halt:
    hlt
    jmp .halt

# ============================================================================
# Variáveis estáticas do stub
# ============================================================================

.section .data
kernel_entry:
    .quad 0                          # Preenchido pelo linker (símbolo do kernel)
kernel_size:
    .quad 0
initramfs_addr:
    .quad 0
initramfs_size:
    .quad 0
module_addr:
    .quad 0

.section .bss
.lcomm stack_space, 4096
