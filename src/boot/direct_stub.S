# direct_stub.S — Stub de boot direto para dnuboot
#
# Colocado no início da seção .text (endereço 0x100000) para que o QEMU,
# ao carregar o binário bruto com -kernel e pular para 0x100000, execute
# primeiro este stub e depois salte para _direct_start.

.section .boot.stub, "ax", @progbits
.global boot_stub
.type boot_stub, @function

boot_stub:
    cli                         # Desabilita interrupções
    jmp _direct_start           # Salta para o entry point do boot direto

.size boot_stub, . - boot_stub
