# Especificação de Sistemas de Arquivos — Kernel DNU

**Versão:** 1.0  
**Status:** Rascunho  
**Aplica-se a:** `src/fs/` (VFS, RAMFS, DFS, GPT)

---

## 1. Introdução

O kernel DNU suporta múltiplos sistemas de arquivos, organizados por meio de uma camada VFS (Virtual File System) que permite montar diferentes backends sob um único namespace unificado.

Os sistemas de arquivos suportados são:

| Sistema | Tipo | Descrição |
|---------|------|-----------|
| **RAMFS** | Volátil | Sistema de arquivos em RAM, populado a partir do initramfs do dzImage |
| **DFS** | Persistente | Sistema de arquivos de disco (Dreamcore File System), montado a partir de uma partição GPT |
| **VFS** | Camada abstrata | Camada de virtualização que permite montar RAMFS, DFS e outros sob caminhos unificados |

---

## 2. VFS — Virtual File System

Localizado em `src/fs/vfs.rs`.

A VFS fornece uma interface unificada para operações de arquivo, independentemente do sistema de arquivos subjacente:

| Operação | Descrição |
|----------|-----------|
| `open` | Abre um arquivo por caminho |
| `read` | Lê dados de um descritor de arquivo |
| `write` | Escreve dados em um descritor de arquivo |
| `close` | Fecha um descritor de arquivo |
| `lseek` | Move o cursor de leitura/escrita |
| `fstat` | Obtém metadados de um arquivo |
| `mkdir` | Cria um diretório |
| `rmdir` | Remove um diretório |
| `unlink` | Remove um arquivo |
| `rename` | Renomeia um arquivo ou diretório |

Descritores de arquivo são referenciados por inteiros (`fd`) dentro do contexto de um processo.

---

## 3. RAMFS — Sistema de Arquivos em RAM

Localizado em `src/fs/ramfs.rs`.

O RAMFS é um sistema de arquivos volátil populado a partir do initramfs embarcado no dzImage. É o primeiro sistema de arquivos montado durante o boot e serve como raiz temporária.

### Fluxo de Montagem

1. Durante o boot, `embedded_initramfs()` lê a imagem initramfs do `BootInfo.dzimage`.
2. `ramfs::mount(ramfs_image)` popula o RAMFS com os arquivos do initramfs.
3. O kernel tenta montar o DFS como raiz persistente; se falhar, continua com RAMFS como raiz.
4. O subsistema `Ramfs` é marcado como pronto no boot.

### Funções Principais

| Função | Descrição |
|--------|-----------|
| `ramfs::mount(image)` | Monta uma imagem initramfs no RAMFS |
| `ramfs::read(path)` | Lê o conteúdo de um arquivo no RAMFS |
| `ramfs::find_font()` | Localiza um arquivo de fonte PSF no RAMFS |
| `ramfs::set_block_disk_present(present)` | Indica se um disco bloco está disponível |

---

## 4. DFS — Dreamcore File System

Localizado em `src/fs/dfs.rs`.

O DFS é o sistema de arquivos persistente do DNU, montado a partir de uma partição GPT no disco ATA primário. É opcional: o sistema pode bootar sem ele, usando apenas RAMFS.

### Fluxo de Montagem

1. Durante o boot, `probe_storage()` inicializa o driver ATA e detecta o disco.
2. `dfs::mount_primary()` tenta montar a partição primária GPT como DFS.
3. Se o mount falhar ou não houver partição, o sistema continua com RAMFS como raiz.
4. O subsistema `Dfs` é marcado como pronto ou indisponível no boot.

### Funções Principais

| Função | Descrição |
|--------|-----------|
| `dfs::mount_primary()` | Monta a partição DFS primária |
| `dfs::read(path)` | Lê um arquivo do DFS |
| `dfs::write(path, data)` | Escreve um arquivo no DFS |

---

## 5. GPT — GUID Partition Table

Localizado em `src/fs/gpt.rs`.

O GPT é usado para identificar partições no disco. O driver ATA lê a tabela de partições GPT para localizar a partição DFS.

---

## 6. Locais no Sistema de Arquivos

Durante o boot, diretórios padrão são criados no RAMFS:

| Caminho | Uso |
|---------|-----|
| `/etc` | Configuração do sistema |
| `/home` | Diretórios de usuário |
| `/tmp` | Arquivos temporários |
| `/usr` | Programas e bibliotecas de sistema |
| `/sbin/init` | Primeiro processo (PID 1) |

Quando `RAMFS_DIR` é informado no build, o conteúdo dessa árvore é copiado para o initramfs do dzImage.

---

## 7. Syscalls Relacionadas

As seguintes syscalls (definidas em `abispec.md`) operam sobre o VFS:

| Syscall | Número | Descrição |
|---------|--------|-----------|
| `SYS_OPEN` | 3 | Abre um arquivo |
| `SYS_READ` | 1 | Lê de um descritor de arquivo |
| `SYS_WRITE` | 2 | Escreve em um descritor de arquivo |
| `SYS_CLOSE` | 4 | Fecha um descritor de arquivo |
| `SYS_STAT` | 5 | Obtém metadados de arquivo |
| `SYS_LSEEK` | 6 | Move o cursor de leitura/escrita |
| `SYS_MKDIR` | 27 | Cria um diretório |
| `SYS_RMDIR` | 28 | Remove um diretório |
| `SYS_UNLINK` | 29 | Remove um arquivo |
| `SYS_RENAME` | 30 | Renomeia arquivo/diretório |
| `SYS_CHDIR` | 26 | Muda o diretório de trabalho |

---

## Referências

- [Especificação de Boot](./bootspec.md) — montagem de sistemas de arquivos durante o boot
- [ABI e Syscalls](./abispec.md) — syscalls de sistema de arquivos
- [Drivers](./drivers.md) — driver ATA e disco
