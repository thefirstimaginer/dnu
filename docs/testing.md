# Testes — Kernel DNU

**Versão:** 1.0  
**Status:** Rascunho  
**Aplica-se a:** `tests/` e `make test`

---

## 1. Visão Geral

Os testes do kernel DNU são executados no host (não no bare metal/QEMU). Cada arquivo em `tests/` é um harness compilado com `rustc --test` no host. Eles validam lógica pura do kernel que pode ser executada fora do ambiente de boot (parsing de estruturas, cálculos, layout de TLS, etc.).

O alvo `make test` compila e executa todos os testes.

---

## 2. Testes Disponíveis

| Teste | Arquivo | Descrição |
|-------|---------|-----------|
| `dfs` | `tests/dfs.rs` | Teste do sistema de arquivos Dreamcore (DFS) |
| `dzimage` | `tests/dzimage.rs` | Teste de parsing e empacotamento do dzImage |
| `installer` | `tests/installer.rs` | Teste do instalador do sistema |
| `ipc_threads` | `tests/ipc_threads.rs` | Teste de comunicação entre processos e threads |
| `keyboard` | `tests/keyboard.rs` | Teste do driver de teclado |
| `shutdown` | `tests/shutdown.rs` | Teste do caminho de desligamento (ACPI) |
| `storage` | `tests/storage.rs` | Teste de armazenamento ATA / bloco |
| `time_math` | `tests/time_math.rs` | Teste de cálculos de tempo (TSC, ticks) |
| `tls_layout` | `tests/tls_layout.rs` | Teste de layout de Thread Local Storage |
| `trap` | `tests/trap.rs` | Teste de traps e exceções de CPU |
| `vfs` | `tests/vfs.rs` | Teste da camada Virtual File System |

---

## 3. Executando Testes

```bash
# Todos os testes
make test

# Apenas checagem de tipos (sem executar)
make check
```

O alvo `check` também roda `clippy` com lints rígidos:
```bash
make clippy
```

---

## 4. Testes em QEMU (Boot)

Os testes de `tests/` não cobrem o boot completo em QEMU. Para validar alterações que afetam o boot, o procedimento recomendado é:

```bash
make dzImage                  # gera o dzImage
# Inicie a ISO/imagem no QEMU usando o orquestrador do ImagineOS
# Verifique a saída serial para confirmar que todos os subsistemas subiram
```

Veja `bootspec.md` para o fluxo de boot e `subsystems.md` para a lista de subsistemas que devem estar prontos.

---

## 5. Escrevendo Novos Testes

1. Crie um novo arquivo em `tests/` com a função `#[test]` ou uma suíte de testes.
2. O Makefile detecta automaticamente arquivos em `tests/*.rs` via `KERNEL_TESTS`.
3. Use o escopo `test` em commits (veja `commiting.md`).

Exemplo de commit:
```
test: adicionar teste de parse de initramfs no dzimage
```

---

## Referências

- [Sistema de Build](./build.md) — alvos `test`, `check`, `clippy`
- [Especificação de Boot](./bootspec.md) — fluxo de boot para testes em QEMU
- [Subsistemas de Boot](./subsystems.md) — estado dos subsistemas no boot
