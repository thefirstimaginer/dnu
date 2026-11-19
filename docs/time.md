# Tempo e Timer — Kernel DNU

**Versão:** 1.0  
**Status:** Rascunho  
**Aplica-se a:** `src/time/time.rs` e `src/arch/arch_timer.rs`

---

## 1. Introdução

O subsistema de tempo do kernel DNU fornece:

- **TSC (Time Stamp Counter):** relógio de alta resolução baseado no contador de ciclos da CPU.
- **PIT (Programmable Interval Timer):** timer de interrupção para preempção e wakeup com timeout.
- **Timestamps de log:** microssegundos desde o boot, usados nos logs do kernel.

---

## 2. TSC

O TSC é inicializado durante o boot com `time::init_with(tsc_start, tsc_frequency)`, usando valores fornecidos pelo Limine no `BootInfo`.

| Função | Descrição |
|--------|-----------|
| `time::init_with(start, freq)` | Inicializa o TSC com valor inicial e frequência |
| `time::tsc_frequency()` | Retorna a frequência do TSC em Hz |
| `time::format_elapsed(buf)` | Formata o tempo decorrido desde o boot em microssegundos |

O tempo decorrido é usado para timestamp de linhas de log no console:

```
[ 1234.567890 ] mensagem do kernel
```

---

## 3. Timer PIT

O timer PIT (IRQ0) é configurado durante `start_init()` com `time::configure_timer_interrupt()`. Ele fornece:

- Preempção por prioridade para o escalonador de processos.
- Wakeup de `nanosleep` e `futex` com timeout.

### Diagnóstico de Falha

Após configurar o timer, o kernel aguarda alguns ticks e verifica se o contador avançou. Se não, imprime aviso na serial:

```
AVISO: timer PIT nao esta disparando apos configure_timer_interrupt
```

Se o timer estiver funcionando:

```
Timer OK: ticks estao disparando
Timer configurado; preempcao por prioridade ativa
```

---

## 4. Syscalls Relacionadas

| Syscall | Número | Descrição |
|---------|--------|-----------|
| `SYS_NANOSLEEP` | 16 | Suspende execução por um intervalo de tempo |
| `SYS_CLOCK_GETTIME` | 18 | Obtém tempo de um relógio |
| `SYS_FUTEX` | 17 | Sincronização com timeout baseado em tempo |

---

## 5. Exemplos de Commit

```
feat(time): adicionar suporte a SYS_CLOCK_GETTIME para relógio monotônico
fix(time): corrigir wraparound no contador de ticks do PIT
perf(time): reduzir latência de leitura do TSC
```

---

## Referências

- [Especificação de Boot](./bootspec.md) — inicialização do timer durante o boot
- [Arquitetura x86_64](./archspec.md) — timer PIT e interrupções
- [Gerenciamento de Processos](./process.md) — nanosleep e preempção
- [Comunicação entre Processos](./ipc.md) — futex com timeout
