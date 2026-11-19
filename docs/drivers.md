# Especificação de Drivers — Kernel DNU

**Versão:** 1.0  
**Status:** Rascunho  
**Aplica-se a:** `src/drivers/` (drivers de bloco, entrada, vídeo, platform)

---

## 1. Introdução

O kernel DNU inclui drivers de dispositivos compilados diretamente no kernel. Cada categoria de hardware tem um driver genérico que implementa o mínimo necessário para o dispositivo funcionar, sem travar o kernel: em caso de falha, retorna erro e continua operando.

Os drivers estão organizados em `src/drivers/` e coordenados pelo hub `drivers_hub.rs`.

---

## 2. Drivers Disponíveis

| Driver | Arquivo | Descrição |
|--------|---------|-----------|
| **ATA Block** | `src/drivers/ata.rs` | Driver de disco ATA (primary master). Lê setores do disco para sistemas de arquivos (DFS) e initramfs. |
| **Block** | `src/drivers/block.rs` | Camada genérica de bloco que abstrai o acesso a dispositivos de armazenamento. |
| **Teclado** | `src/drivers/keyboard.rs` | Driver de teclado PS/2. Lê eventos de tecla para o console do kernel. |
| **Mouse** | `src/drivers/mouse.rs` | Driver de mouse PS/2. Lê eventos de movimento e cliques. |
| **ACPI** | `src/drivers/acpi.rs` | Driver ACPI: lê RSDP, tabelas ACPI e fornece caminho de desligamento do sistema. |

---

## 3. Driver de Bloco (ATA)

Localizado em `src/drivers/ata.rs`.

O driver ATA acessa o disco primário-master via portas I/O padrão (0x1F0–0x1F7 para comandos, 0x3F6 para status de controle). É usado para:

- Detectar o disco e ler o número de setores disponíveis.
- Ler a tabela de partições GPT.
- Montar o sistema de arquivos DFS.

### Fluxo de Inicialização

Durante o boot, `probe_storage()` (em `src/main.rs`) chama `drivers::block::init()`:

1. Se houver um disco ATA primário-master disponível: inicializa o driver, lê o número de setores, marca o subsistema `Storage` como pronto.
2. Se não houver disco: continua sem storage, marca `Storage` como pronto, e o sistema usa apenas RAMFS.

---

## 4. Driver de Teclado

Localizado em `src/drivers/keyboard.rs`.

O driver de teclado lê eventos de tecla do controlador PS/2 e os disponibiliza para o console do kernel via syscalls `SYS_KEYBOARD_READ`.

---

## 5. Driver de Mouse

Localizado em `src/drivers/mouse.rs`.

O driver de mouse lê eventos de movimento e cliques do controlador PS/2 e os disponibiliza via syscall `SYS_MOUSE_READ`.

---

## 6. Driver ACPI

Localizado em `src/drivers/acpi.rs`.

O driver ACPI localiza a RSDP (Root System Description Pointer), lê as tabelas ACPI e fornece o caminho de desligamento do sistema (via syscall `SYS_SHUTDOWN`). O subsistema `Acpi` é marcado como pronto durante o boot.

---

## 7. Hub de Drivers

Localizado em `src/drivers/drivers_hub.rs`.

O hub coordena a inicialização de todos os drivers e fornece pontos de entrada para o restante do kernel. Durante o boot, os drivers são inicializados em ordem apropriada (armazenamento antes de sistemas de arquivos, por exemplo).

---

## 8. Adicionando um Novo Driver

Para adicionar um novo driver ao kernel DNU:

1. Crie o arquivo do driver em `src/drivers/`.
2. Implemente as operações básicas (init, read, write, etc.) seguindo o padrão dos drivers existentes.
3. Registre o driver no hub (`drivers_hub.rs`) se houver coordenação necessária.
4. Adicione testes em `tests/` se aplicável.
5. Atualize este documento (`drivers.md`) com o novo driver.
6. Use o escopo `drivers` em commits (veja `commiting.md`).

Exemplo de commit:
```
feat(drivers): adicionar driver de bloco NVMe
```

---

## Referências

- [Especificação de Boot](./bootspec.md) — inicialização de drivers durante o boot
- [Sistemas de Arquivos](./fsspec.md) — DFS e uso do disco ATA
- [ABI e Syscalls](./abispec.md) — syscalls de dispositivo (`SYS_KEYBOARD_READ`, `SYS_MOUSE_READ`, etc.)
- [Convenção de Commits](./commiting.md) — escopo `drivers`
