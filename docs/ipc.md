# Comunicação entre Processos (IPC) — Kernel DNU

**Versão:** 1.0  
**Status:** Rascunho  
**Aplica-se a:** `src/ipc/ipc.rs`

---

## 1. Introdução

O kernel DNU fornece mecanismos de comunicação entre processos (IPC) para permitir que programas de userspace troquem dados e se sincronizem. Os mecanismos incluem pipes, futexes, sinais e memória compartilhada (via `mmap`).

---

## 2. Mecanismos de IPC

| Mecanismo | Syscall | Descrição |
|-----------|---------|-----------|
| **Pipes** | `SYS_PIPE` (32) | Cria um pipe unidirecional para comunicação entre processos |
| **Futex** | `SYS_FUTEX` (17) | Sincronização de threads/processos via palavra de memória compartilhada |
| **Sinais** | `SYS_KILL` (13) | Envia sinais a processos |
| **Memória compartilhada** | `SYS_MMAP` (7) | Mapeia páginas compartilhadas entre processos |

---

## 3. Pipes

Localizado em `src/ipc/ipc.rs`.

Pipes são criados com `SYS_PIPE` e retornam dois descritores de arquivo: um para leitura e um para escrita. Dados escritos em um lado podem ser lidos no outro. Pipes são úteis para comunicação entre processos relacionados (pai/filho) ou processos não relacionados via shell (operador `|`).

---

## 4. Futexes

Localizado em `src/ipc/ipc.rs`.

O futex (fast userspace mutex) permite que threads e processos se sincronizem bloqueando e acordando com base no valor de uma palavra de memória compartilhada. É usado por primitivos de sincronização de userspace (mutexes, condvars, semáforos).

---

## 5. Sinais

Localizado em `src/abi/abi_process.rs`.

Sinais são enviados com `SYS_KILL` e entregues ao processo de destino. O kernel gerencia a fila de sinais pendentes e a entrega quando o processo está em um ponto seguro de execução.

---

## 6. Memória Compartilhada

Localizado em `src/mm/memory.rs` e `src/abi/abi_thread.rs`.

Processos podem compartilhar páginas de memória via `SYS_MMAP` com o flag `COW` (Copy-on-Write) ou mapeando a mesma região física. Isso permite comunicação de alta performance entre processos.

---

## 7. Exemplos de Uso

```bash
# Comunicação via pipe (shell do userspace)
cat /etc/config | wc -l

# Sincronização via futex (biblioteca de userspace)
mutex_lock(&mutex);
```

Exemplo de commit:
```
feat(ipc): adicionar suporte a pipes nomeados (FIFO)
fix(ipc): corrigir race condition no wakeup de futex
```

---

## Referências

- [ABI e Syscalls](./abispec.md) — syscalls `SYS_PIPE`, `SYS_FUTEX`, `SYS_KILL`, `SYS_MMAP`
- [Gerenciamento de Memória](./mmspec.md) — `mmap` e páginas compartilhadas
- [Gerenciamento de Processos](./process.md) — sinais e ciclo de vida de processos
