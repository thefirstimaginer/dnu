# Subsistemas de Boot — Kernel DNU

**Versão:** 1.0  
**Status:** Rascunho  
**Aplica-se a:** `src/subsystems/subsystems.rs`

---

## 1. Introdução

Durante o boot, o kernel DNU inicializa uma sequência de subsistemas e marca cada um como pronto. O estado é armazenado em um bitmap atômico (`READY`) colocado na seção `.data.early` para evitar page faults durante o boot inicial.

O relatório final é impresso em bloco no fim do boot, com status `[ ok ]` ou `[ -- ]` para cada subsistema, permitindo ao operador ver de relance o que subiu e o que falhou.

---

## 2. Lista de Subsistemas

Os subsistemas são inicializados na seguinte ordem:

| # | Subsistema | Label | Obrigatório | Descrição |
|---|------------|-------|-------------|-----------|
| 0 | DescriptorTables | `gdt/idt` | Sim | GDT/TSS e IDT: mínimo para tratar exceções e interrupções |
| 1 | Memory | `memory` | Sim | Alocador de frames físicos e heap do kernel |
| 2 | Console | `console` | Sim | Framebuffer, fonte PSF e cursor |
| 3 | Timer | `timer` | Sim | PIT (IRQ0) e preempção por prioridade |
| 4 | Acpi | `acpi` | Sim | ACPI: RSDP, tabelas e caminho de desligamento |
| 5 | Storage | `storage` | Sim | Disco ATA primário e leitura da GPT |
| 6 | Ramfs | `ramfs` | Sim | RAMFS montado a partir do initramfs |
| 7 | Dfs | `dfs` | Não | Raiz persistente DFS (opcional: o sistema sobe sem ele) |
| 8 | Configuration | `config` | Sim | Configuração global (`global.conf`) e layout de teclado |
| 9 | InitProcess | `init` | Não | Primeiro processo (PID 1) carregado e pronto |
| 10 | Allocator | `allocator` | Sim | Alocador de heap global com suporte a alloc/dealloc completo |

> **Obrigatório:** Todo subsistema é obrigatório, exceto DFS (opcional), InitProcess (marcado pronto depois que `/sbin/init` é carregado) e Timer (inicializado dentro de `start_init`).

---

## 3. API dos Subsistemas

| Função | Descrição |
|--------|-----------|
| `mark_ready(subsystem)` | Marca um subsistema como inicializado com sucesso |
| `mark_unavailable(subsystem)` | Marca um subsistema como indisponível (falhou ou não existe) |
| `is_ready(subsystem)` | Retorna `true` se o subsistema está pronto |
| `all_required_ready()` | Retorna `true` quando todos os subsistemas obrigatórios subiram |
| `report()` | Escreve uma linha por subsistema e um resumo final no console |

---

## 4. Exemplo de Relatório

```
boot: subsystem status
  [ ok ] gdt/idt
  [ ok ] memory
  [ ok ] console
  [ ok ] timer
  [ ok ] acpi
  [ ok ] storage
  [ ok ] ramfs
  [ -- ] dfs (optional)
  [ ok ] config
  [ ok ] init
  [ ok ] allocator
boot: all required subsystems ready
```

---

## 5. Inicialização no Boot

Durante `kernel_entry` (em `src/main.rs`), os subsistemas são marcados à medida que são inicializados:

1. `mark_ready(Subsystem::Memory)` — depois que o `FrameAllocator` é inicializado.
2. `mark_ready(Subsystem::Allocator)` — depois que o heap global é inicializado.
3. `mark_ready(Subsystem::Console)` — depois que o console de framebuffer está pronto.
4. `mark_ready(Subsystem::Acpi)` — depois que o driver ACPI é inicializado.
5. `mark_ready(Subsystem::Storage)` — depois que o disco ATA é detectado.
6. `mark_ready(Subsystem::Ramfs)` — depois que o RAMFS é montado.
7. `mark_ready(Subsystem::Dfs)` ou `mark_unavailable(Subsystem::Dfs)` — dependendo do resultado do mount.
8. `mark_ready(Subsystem::Configuration)` — depois que a configuração global é carregada.
9. `mark_ready(Subsystem::InitProcess)` — depois que `/sbin/init` (PID 1) é carregado.
10. `mark_ready(Subsystem::Timer)` — depois que o timer PIT é configurado.

O boot prossegue para userspace apenas quando `all_required_ready()` retorna `true`.

---

## Referências

- [Especificação de Boot](./bootspec.md) — fluxo de boot e ordem de inicialização
- [Convenção de Commits](./commiting.md) — escopo `kernel` para alterações nos subsistemas
