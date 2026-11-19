# Gerenciamento de Processos — Kernel DNU

**Versão:** 1.0  
**Status:** Rascunho  
**Aplica-se a:** `src/process/` (carga de DEF, escalonador, syscalls de processo)

---

## 1. Introdução

O kernel DNU gerencia processos de userspace: carga de executáveis DEF, criação de processos (spawn/fork), escalonamento por prioridade, sinais e espera por término de filhos.

O primeiro processo é `/sbin/init` (PID 1), iniciado durante o boot após todos os subsistemas obrigatórios estarem prontos.

---

## 2. Formato de Executável

Programas de userspace usam o formato **DEF** (veja `defspec.md`). O kernel rejeita qualquer imagem que não comece com o magic `DNUF`. O loader ELF foi removido: `SYS_EXEC`/`SYS_EXECVE`/`SYS_SPAWN` aceitam apenas DEF.

---

## 3. Ciclo de Vida de um Processo

```
SYS_SPAWN / SYS_EXEC
    │
    ├─► Carrega arquivo DEF (valida magic, arch, ABI, CRC32)
    ├─► Aloca address space e page tables
    ├─► Mapeia segmentos (código, dados, rodata, BSS, stack)
    ├─► Configura registradores de entrada (x86_64 System V)
    ├─► Cria entrada na tabela de processos (PID, prioridade, estado)
    ├─► Insere na fila de escalonamento
    └─► Retorna PID ao processo pai (ou ao chamador)
```

Durante a execução, o processo pode:

- Chamar `SYS_EXIT` para terminar.
- Chamar `SYS_WAIT` para aguardar término de um filho.
- Chamar `SYS_KILL` para enviar sinais a outro processo.
- Chamar `SYS_NANOSLEEP` para suspender execução temporariamente.

---

## 4. Escalonador

O kernel usa **preempção por prioridade**, habilitada pelo timer PIT. Cada processo tem uma prioridade associada; o escalonador escolhe o processo de maior prioridade pronto para executar.

A preempção só é ativada depois que `time::configure_timer_interrupt()` é chamado (dentro de `start_init`), após toda a infraestrutura do kernel estar pronta.

---

## 5. Syscalls de Processo

| Syscall | Número | Descrição |
|---------|--------|-----------|
| `SYS_SPAWN` | 9 | Cria um novo processo |
| `SYS_EXEC` | 11 | Carrega e executa um programa DEF |
| `SYS_FORK` | 12 | Cria um processo filho (cópia do atual) |
| `SYS_WAIT` | 10 | Aguarda término de processo filho |
| `SYS_EXIT` | 0 | Encerra o processo atual |
| `SYS_KILL` | 13 | Envia sinal a um processo |
| `SYS_GETPID` | 14 | Retorna o PID do processo atual |
| `SYS_GETPPID` | 15 | Retorna o PID do processo pai |
| `SYS_NANOSLEEP` | 16 | Suspende execução por um intervalo de tempo |

---

## 6. Inicialização de Userspace

Durante o boot, `start_init()` (em `src/main.rs`):

1. Lê `/sbin/init` da RAMFS.
2. Chama `process::init()` para carregá-lo como PID 1.
3. Marca `Subsystem::InitProcess` como pronto.
4. Configura o timer PIT e habilita interrupções (`sti`).
5. Chama `process::start()` para iniciar o escalonador.

Se `/sbin/init` não for encontrado, o kernel dá halt.

---

## 7. Exemplos de Commit

```
feat(process): adicionar suporte a SYS_FORK para criar processos filhos
fix(process): corrigir vazamento de descritores ao sair de um processo
perf(process): reduzir sobrecarga do escalonador por prioridade
```

---

## Referências

- [Formato DEF](./defspec.md) — formato de executáveis userspace
- [ABI e Syscalls](./abispec.md) — syscalls de processo
- [Especificação de Boot](./bootspec.md) — inicialização de `/sbin/init` durante o boot
- [Comunicação entre Processos](./ipc.md) — sinais e sincronização
