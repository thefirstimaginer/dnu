# Especificação de Arquitetura — x86_64 (Kernel DNU)

**Versão:** 1.0  
**Status:** Rascunho  
**Aplica-se a:** Código específico de arquitetura em `src/arch/`

---

## 1. Introdução

Este documento descreve os componentes específicos da arquitetura x86_64 do kernel DNU: GDT, IDT, FPU e timer. O kernel é compilado para o alvo `x86_64-unknown-none` (bare metal, sem sistema operacional host) e usa a toolchain nightly do Rust (devido ao crate `limine` e recursos `#![feature]`).

---

## 2. GDT — Global Descriptor Table

Localizado em `src/arch/gdt.rs`.

A GDT define os descritores de segmento usados pelo modo protegido e modo longo (64-bit). No modo longo, a maioria dos descritores é pouco usada, mas ainda é necessária uma GDT mínima para definir segmentos de código e dados do kernel.

### Componentes

| Componente | Descrição |
|------------|-----------|
| `gdt` | Estrutura da GDT com descritores de código e dados |
| `tss` | Task State Segment (usado para o stack de interrupção de kernel) |

O GDT é carregado via instrução `lgdt` durante a inicialização do kernel (`kernel_entry` em `main.rs`).

---

## 3. IDT — Interrupt Descriptor Table

Localizado em `src/arch/idt.rs`.

A IDT mapeia vetores de interrupção e exceções para handlers no kernel. Inclui:

- **Exceções de CPU:** page fault, general protection fault, double fault, invalid opcode, etc.
- **IRQs:** interrupções de hardware (teclado, timer PIT, disco ATA, etc.)
- **Syscalls:** configuradas via MSRs (`IA32_LSTAR`, `IA32_STAR`, `IA32_FMASK`) para a instrução `syscall`.

### MSRs de Syscall

Configurados por último no boot, após toda a infraestrutura do kernel estar pronta, para evitar reset durante inicialização de memória/allocator no QEMU:

```rust
idt::configure_syscall_msrs();
```

Os MSRs configurados são:

| MSR | Descrição |
|-----|-----------|
| `IA32_LSTAR` | Endereço do handler de syscall no kernel |
| `IA32_STAR` | Segmentos de usuário e kernel para syscall/sysret |
| `IA32_FMASK` | Flags a limpar ao entrar em modo kernel |

---

## 4. FPU — Floating Point Unit

Localizado em `src/arch/fpu.rs`.

A FPU é inicializada no início do boot (`kernel_entry`) para garantir que o estado de ponto flutuante esteja correto antes de qualquer operação que possa usá-la. A inicialização configura os registradores de controle da FPU e SSE/AVX.

---

## 5. Timer — PIT

Localizado em `src/arch/arch_timer.rs` e `src/time/time.rs`.

O timer PIT (Programmable Interval Timer, IRQ0) é configurado durante `start_init()` para fornecer preempção por prioridade e wakeup de `nanosleep`/`futex` com timeout.

### Operações do Timer

| Função | Descrição |
|--------|-----------|
| `time::configure_timer_interrupt()` | Configura o PIT e habilita interrupções |
| `time::timer_ticks()` | Retorna o contador atual de ticks do timer |
| `time::tsc_frequency()` | Retorna a frequência do TSC em Hz |

Diagnóstico de falha: após `configure_timer_interrupt()`, o kernel aguarda alguns ticks e verifica se o contador avançou. Se não, imprime um aviso na serial: `"AVISO: timer PIT nao esta disparando apos configure_timer_interrupt"`.

---

## 6. Alinhamento e Endereçamento

- O kernel é compilado para o alvo `x86_64-unknown-none`.
- O modo Limine exige o kernel em endereço alto (linker script `src/linker.ld`).
- O modo direto usa endereço baixo identity-mapped (linker script `src/linker_direct.ld`).
- O HHDM (Higher Half Direct Map) do Limine mapeia memória física para endereços virtuais altos; o offset é obtido via `boot_info.hhdm_offset`.

---

## Referências

- [Especificação de Boot](./bootspec.md) — fluxo de boot e entry points
- [Gerenciamento de Memória](./mmspec.md) — paginação, alocador de frames, HHDM
- [Convenção de Commits](./commiting.md) — regras de commit
