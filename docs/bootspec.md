# Especificação de Boot — Kernel DNU

**Versão:** 1.0  
**Status:** Rascunho  
**Aplica-se a:** Kernel DNU (Dreamcore is Not Unix)

---

## 1. Introdução

Este documento descreve o processo de boot do kernel DNU, desde a entrada do bootloader até o lançamento do primeiro processo de userspace (`/sbin/init` como PID 1).

O kernel suporta dois modos de boot:

| Modo | Descrição | Linker Script | Entry Point |
|------|-----------|---------------|-------------|
| **Limine** (padrão) | Carregado pelo bootloader Limine, endereço alto | `src/linker.ld` | `_start_rust` / `kernel_entry` |
| **Direto** (dnuboot) | Boot direto sem Limine, endereço baixo identity-mapped | `src/linker_direct.ld` | `_direct_start` |

---

## 2. Artefato: dzImage

O único artefato publicado pelo build é o **dzImage** (DNU Zipped Image), localizado em `.build/dzImage`. Ele contém:

- O kernel ELF linkado com `src/linker.ld` (endereço alto, exigido pelo Limine).
- Opcionalmente, um initramfs (tar USTAR) quando `RAMFS_DIR` é informado.

### Geração

```bash
make dzImage                                # kernel apenas
make dzImage RAMFS_DIR=../imagine-os/ramfs  # kernel + initramfs
```

O dzImage é empacotado pela ferramenta de host `dzimage` (`tools/dzimage_host.rs`).

> **Nota:** O kernel deve ser recompilado no modo Limine antes de empacotar no dzImage, para evitar o panic "Lower half PHDRs are not allowed" no boot da ISO.

---

## 3. Fluxo de Boot (Modo Limine)

```
1. Limine carrega o kernel ELF e passa um ponteiro para BootInfo.
2. _start_rust() → chama boot::limine_start().
3. kernel_entry(boot_info: *const BootInfo) -> !:
   a. Desativa interrupções (cli).
   b. Salva RSP (diagnóstico na serial 0x3f8).
   c. Inicializa FPU (fpu::init).
   d. Inicializa UART serial (uart::init).
   e. Valida BootInfo (não nulo; senão → kernel panic).
   f. Inicializa TSC (time::init_with).
   g. Mascara PIC legado (boot::mask_legacy_pic).
   h. Inicializa GDT (gdt::init).
   i. Inicializa IDT (idt::init).
   j. Inicializa gerenciador de memória (memory::init_with_allocator).
   k. Marca subsistemas prontos (veja subsystems.md).
   l. Monta sistemas de arquivos (RAMFS, DFS) — mount_root_filesystems.
   m. Carrega configuração global (global.conf) e fonte PSF.
   n. Configura MSRs de syscall (idt::configure_syscall_msrs) — feito por último.
   o. Inicia /sbin/init como PID 1 (process::init).
   p. Configura timer PIT e habilita interrupções (sti).
   q. Inicia o escalonador de processos (process::start).
```

### Detalhes Críticos

- **FPU antes de tudo:** `fpu::init()` é chamado no início de `kernel_entry` para garantir que o estado de ponto flutuante esteja correto antes de qualquer operação.
- **Serial primeiro:** A UART em `0x3f8` é usada para diagnóstico precoce, antes mesmo do console de framebuffer estar disponível.
- **MSRs por último:** Os MSRs de syscall são configurados só depois que toda a infraestrutura do kernel (memória, allocator, IDT) está pronta, para evitar reset durante inicialização no QEMU.
- **Page tables fixadas:** `paging::fix_kernel_page_table_entries()` corrige entradas de page table do kernel após o boot inicial.

---

## 4. Fluxo de Boot (Modo Direto / dnuboot)

```
1. O binário boot.bin (stub + kernel) é carregado pelo dnuboot ou firmware.
2. O stub salta para _direct_start.
3. _direct_start → boot::direct_start():
   a. Mesmo fluxo de kernel_entry, mas sem dependência do Limine.
   b. Endereço baixo (identity-mapped via linker_direct.ld).
   c. Não há BootInfo do Limine; parâmetros de boot vêm de outra fonte.
```

---

## 5. Subsistemas de Boot

Durante o boot, cada etapa marca o seu subsistema como pronto. A ordem e quais são obrigatórios estão documentados em `subsystems.md`.

Subsistemas obrigatórios para continuar o boot:
- DescriptorTables (GDT/IDT)
- Memory (alocador de frames)
- Console (framebuffer/serial)
- Timer (PIT)
- Acpi
- Storage (ATA)
- Ramfs
- Dfs (opcional)
- Configuration
- Allocator (heap global)

O boot prossegue para userspace apenas quando todos os obrigatórios estão prontos (`all_required_ready()`).

---

## 6. Arquivos Relacionados

| Arquivo | Função |
|---------|--------|
| `src/main.rs` | Ponto de entrada do kernel (`kernel_entry`, `_start_rust`, `halt`) |
| `src/boot.rs` | Inicialização de boot (Limine, direto) |
| `src/boot/dzimage.rs` | Parsing do formato dzImage / initramfs |
| `src/boot/boot_info.rs` | Struct BootInfo passada pelo Limine |
| `src/linker.ld` | Linker script para modo Limine (endereço alto) |
| `src/linker_direct.ld` | Linker script para modo direto (endereço baixo) |
| `src/arch/gdt.rs` | GDT |
| `src/arch/idt.rs` | IDT e MSRs de syscall |
| `src/arch/fpu.rs` | Inicialização de FPU |
| `src/mm/allocator.rs` | Alocador de frames físicos |
| `src/mm/memory.rs` | Gerenciador de memória |
| `src/subsystems/subsystems.rs` | Relatório de subsistemas de boot |
| `tools/dzimage_host.rs` | Empacotador dzImage |
| `tools/dnuboot.rs` | Boot direto |
| `tools/dnu-pack.rs` | Empacotador de binários |

---

## Referências

- [Convenção de Commits](./commiting.md)
- [Subsistemas de Boot](./subsystems.md)
- [Arquitetura x86_64](./archspec.md)
- [Gerenciamento de Memória](./mmspec.md)
