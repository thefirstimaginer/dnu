# Sistema de Build — Kernel DNU

**Versão:** 1.0  
**Status:** Rascunho  
**Aplica-se a:** `Makefile`, `tools/`, `Cargo.toml`

---

## 1. Visão Geral

O build do kernel DNU é feito com **Make** + **Cargo (nightly)**. O único artefato publicado é o **dzImage** (kernel ELF + opcionalmente initramfs), localizado em `.build/dzImage`.

O kernel só compila em **nightly** (usa `#![feature]` do crate `limine`).

---

## 2. Alvos do Makefile

| Alvo | Descrição |
|------|-----------|
| `dzImage` (padrão) | O único artefato final: kernel empacotado em dzImage |
| `kernel` | ELF do kernel, sem empacotar |
| `kernel-direct` | ELF do kernel para boot direto (feature `direct-boot`) |
| `boot.bin` | Binário de boot direto (kernel + initramfs embutidos) |
| `tools` | Ferramentas de host (dzimage, dnudebug, dnutest, dnuboot, dnu_pack) |
| `test` | Suíte de testes do kernel no host |
| `check` | Checagem de tipos do kernel no alvo bare-metal |
| `clippy` | Clippy com lints rígidos no kernel e ferramentas de host |
| `clean` | Remove `.build/` e `target/` |
| `help` | Exibe ajuda resumida |

---

## 3. Comandos Comuns

```bash
# Compilar o kernel (modo Limine, padrão)
make kernel

# Gerar o dzImage (kernel apenas)
make dzImage

# Gerar o dzImage com initramfs de uma árvore externa
make dzImage RAMFS_DIR=../imagine-os/ramfs

# Boot direto (sem Limine)
make kernel-direct
make boot.bin

# Compilar ferramentas de host
make tools

# Rodar testes no host
make test

# Checagem de tipos + clippy
make check

# Limpar artefatos
make clean
```

---

## 4. Ferramentas de Host (`tools/`)

| Ferramenta | Arquivo | Descrição |
|----------|---------|-----------|
| `dzimage` | `tools/dzimage_host.rs` | Empacota o kernel ELF em dzImage (com ou sem initramfs) |
| `dnudebug` | `tools/dnudebug.rs` | Ferramenta de depuração do dzImage |
| `dnutest` | `tools/dnutest.rs` | Harness de testes no host |
| `dnuboot` | `tools/dnuboot.rs` | Boot direto do kernel sem Limine |
| `dnu_pack` | `tools/dnu-pack.rs` | Converte ELF → DEF (empacotador de programs userspace) |
| `dnulink` | `tools/dnulink.rs` | Linker de auxílio |

---

## 5. Variáveis de Ambiente

| Variável | Descrição |
|----------|-----------|
| `RAMFS_DIR` | Árvore que vira a raiz volátil (initramfs). Quando não informada, o dzImage contém apenas o kernel. |

---

## 6. Toolchain

O kernel requer a toolchain **nightly** do Rust, especificada em `rust-toolchain.toml`. O crate `limine` usa recursos `#![feature]` que só estão disponíveis no nightly.

```bash
rustup toolchain install nightly
rustup default nightly
rustup component add clippy
```

---

## 7. Perfis de Compilação

| Perfil | `panic` | `lto` | `codegen-units` | Uso |
|--------|---------|-------|-----------------|-----|
| `dev` | `abort` | — | — | Desenvolvimento |
| `release` | `abort` | `true` | `1` | Produção (dzImage publicado) |

O kernel usa `panic = "abort"` em ambos os perfis.

---

## 8. Testes no Host

Os testes em `tests/` são harnesses compilados com `rustc --test` no host:

| Teste | Arquivo | Descrição |
|-------|---------|-----------|
| `dfs` | `tests/dfs.rs` | Teste do sistema de arquivos DFS |
| `dzimage` | `tests/dzimage.rs` | Teste de parsing do dzImage |
| `installer` | `tests/installer.rs` | Teste do instalador |
| `ipc_threads` | `tests/ipc_threads.rs` | Teste de IPC e threads |
| `keyboard` | `tests/keyboard.rs` | Teste do driver de teclado |
| `shutdown` | `tests/shutdown.rs` | Teste de desligamento |
| `storage` | `tests/storage.rs` | Teste de armazenamento ATA |
| `time_math` | `tests/time_math.rs` | Teste de cálculos de tempo |
| `tls_layout` | `tests/tls_layout.rs` | Teste de layout TLS |
| `trap` | `tests/trap.rs` | Teste de traps/exceções |
| `vfs` | `tests/vfs.rs` | Teste do VFS |

Execute com: `make test`

---

## 9. QEMU

Para testar o boot em QEMU:

```bash
make dzImage          # gera o dzImage
# Depois, use o orquestrador de boot do ImagineOS para iniciar a ISO/imagem no QEMU
```

O commit que afeta o boot deve ser testado em QEMU antes de ser submetido (veja `commiting.md`, regra 3).

---

## Referências

- [Convenção de Commits](./commiting.md) — regras de commit para alterações de build
- [Especificação de Boot](./bootspec.md) — artefato dzImage e fluxo de boot
- [DNU Linkable Library](./dllspec.md) — ferramenta `dnu_pack` para conversão ELF → DEF
