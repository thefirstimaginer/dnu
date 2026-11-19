# Formato DEF (Dreamcore "DNU" Executable Format)

O **DEF** (DNU Executable Format) é o formato nativo de executáveis do Kernel DNU. Projetado para simplicidade, segurança e parsing zero-copy em Rust, elimina a complexidade histórica do ELF mantendo apenas a visão de execução (segmentos carregáveis).

> **Nota importante**: Apenas programas DEF são suportados por distros que usam o DNU como base. O kernel continua sendo compilado em ELF para compatibilidade com o bootloader Limine. O loader ELF foi removido do kernel: qualquer imagem que não comece com o magic `DNUF` (0x44 0x4E 0x55 0x46) é rejeitada.

O DNU não é um sistema Unix-like. Não há `fork()`, `execve()` nem separação entre criar processo e carregar imagem. Em vez disso, o kernel expõe uma **única função de execução**: o caller passa todos os parâmetros necessários (caminho da imagem, argumentos, ambiente, descritores de I/O, etc.) e o kernel carrega, inicializa e inicia o programa em uma só chamada.

---

## Visão Geral da Estrutura

```
┌─────────────────────────────────────────────┐
│           Cabeçalho DEF (96 bytes)          │  ← Zero-copy mappable
├─────────────────────────────────────────────┤
│         Tabela de Segmentos (N × 40 B)      │
├─────────────────────────────────────────────┤
│              Payload dos Segmentos          │  ← Dados brutos (código/dados)
└─────────────────────────────────────────────┘
```

---

## Cabeçalho DEF (96 bytes)

O cabeçalho é desenhado para ser diretamente mapeável em memória via `#[repr(C)]` em Rust, permitindo cast direto de `&[u8]` para a struct do cabeçalho (zero-copy parsing).

| Offset | Tamanho | Campo | Descrição |
|---:|---:|---|---|
| 0 | 4 bytes | `magic` | Identificador: `0x44 0x4E 0x55 0x46` (`"DNUF"`) |
| 4 | 1 byte | `arch` | Arquitetura alvo (`DefArch`) |
| 5 | 1 byte | `type` | Tipo de artefato (`DefType`) |
| 6 | 2 bytes | `format_version` | Versão do formato DEF (atual: 1) |
| 8 | 4 bytes | `abi_version` | Versão da ABI do kernel DNU (compatibilidade) |
| 12 | 4 bytes | `flags` | Flags globais (`DefFlags`) |
| 16 | 4 bytes | `segment_count` | Número de entradas na tabela de segmentos |
| 20 | 4 bytes | `reserved` | Reservado para expansão futura (deve ser 0) |
| 24 | 8 bytes | `entry_point` | Endereço virtual do ponto de entrada (`_start`) |
| 32 | 8 bytes | `stack_size` | Tamanho inicial da pilha solicitado (bytes, alinhado a página) |
| 40 | 8 bytes | `image_base` | Endereço-base preferido da imagem (ASLR: kernel pode ignorar) |
| 48 | 8 bytes | `image_size` | Tamanho total da imagem na memória virtual (inclui BSS) |
| 56 | 32 bytes | `capabilities` | Bitmap de capacidades concedidas (32 bytes = 256 bits) |
| 88 | 64 bytes | `signature` | Assinatura Ed25519 (zerado = não assinado) |

**Tamanho total do cabeçalho: 96 bytes** (múltiplo de 16 para alinhamento de cache line).

Não há campo `crc32`. A integridade e autenticidade do binário são garantidas exclusivamente pela **assinatura criptográfica Ed25519** (ou outro algoritmo moderno suportado), verificada pelo kernel antes de qualquer mapeamento de memória.

### Valores de `DefArch` (1 byte)

| Valor | Arquitetura |
|---:|---|
| 0x00 | Desconhecido/Inválido |
| 0x01 | x86_64 (AMD64) |
| 0x02 | AArch64 (ARM64) |
| 0x03 | RISC-V 64-bit |
| 0xFF | Nativo do host (build-time) |

### Valores de `DefType` (1 byte)

| Valor | Tipo | Descrição |
|---:|---|---|
| 0x00 | `APP` | Aplicação de usuário padrão |
| 0x01 | `SERVICE` | Servidor híbrido (VFS, rede, display, drivers) |
| 0x02 | `INIT` | Processo init (PID 1) |
| 0x03 | `KERNEL_MODULE` | Módulo de kernel carregável (futuro) |
| 0x04 | `BOOTLOADER` | Bootloader de segunda etapa |
| 0xFF | `RAW` | Imagem bruta sem metadados especiais |

### Flags (`DefFlags`, 4 bytes, bitmask)

| Bit | Nome | Descrição |
|---:|---|---|
| 0 | `REQUIRES_STACK_GUARD` | Solicita guard page automática no topo da pilha |
| 1 | `REQUIRES_HEAP_GUARD` | Solicita guard page na borda do heap |
| 2 | `ASLR_COMPATIBLE` | Imagem suporta randomização de endereço-base |
| 3 | `PIE` | Position-Independent Executable (relocável) |
| 4 | `STATIC_LINK` | Totalmente estático (sem dependências externas) |
| 5 | `CAPABILITY_MODE` | Executa em modo capacidades (restrito) |
| 6 | `DEBUG_INFO` | Contém informações de depuração |
| 7 | `NO_STD` | Não usa `dnu-std` (bare metal / runtime customizado) |
| 8–31 | — | Reservados (devem ser 0) |

---

## Tabela de Segmentos

Cada entrada tem **40 bytes**. A tabela começa no offset **96** (imediatamente após o cabeçalho).

| Offset na entrada | Tamanho | Campo | Descrição |
|---:|---:|---|---|
| 0 | 8 bytes | `file_offset` | Offset absoluto do segmento no arquivo DEF (desde o byte 0) |
| 8 | 8 bytes | `file_size` | Tamanho dos dados do segmento no arquivo (pode ser 0 para BSS) |
| 16 | 8 bytes | `memory_size` | Tamanho reservado na memória virtual (≥ `file_size`) |
| 24 | 8 bytes | `vaddr` | Endereço virtual do segmento (relativo a `image_base` se PIE) |
| 32 | 4 bytes | `permissions` | Permissões (`DefPermissions`, bitmask) |
| 36 | 4 bytes | `reserved` | Reservado (deve ser 0) |

**Tamanho total por entrada: 40 bytes**

### Permissões de Segmento (`DefPermissions`, bitmask)

| Bit | Nome | Valor | Descrição |
|---:|---|---:|---|
| 0 | `READ` | 0x1 | Leitura permitida |
| 1 | `WRITE` | 0x2 | Escrita permitida |
| 2 | `EXECUTE` | 0x4 | Execução permitida |
| 3 | `USER` | 0x8 | Acessível em Ring 3 (user mode) |
| 4 | `COW` | 0x10 | Copy-on-Write (dados privados por processo) |
| 5 | `GUARD` | 0x20 | Página de guarda (acesso gera exceção) |
| 6 | `NOCACHE` | 0x40 | Desabilita cache (MMIO, framebuffer) |
| 7 | `HUGEPAGE` | 0x80 | Prefere huge pages (2 MiB / 1 GiB) |

**Combinações típicas:**
- Código (`.text`): `READ | EXECUTE | USER` = 0x0D
- Dados ro (`.rodata`): `READ | USER` = 0x09
- Dados rw (`.data` + `.bss`): `READ | WRITE | USER | COW` = 0x1B
- Stack: `READ | WRITE | USER | GUARD` = 0x23
- Heap: `READ | WRITE | USER | COW` = 0x1B

---

## Assinatura Criptográfica

Localizada no campo `signature` do cabeçalho (offset 88, 64 bytes). Verificada pelo kernel **antes** de qualquer mapeamento de memória.

### Formato da Assinatura

Os 64 bytes do campo `signature` contêm uma assinatura Ed25519 do arquivo DEF (exceto os próprios bytes da assinatura). Quando todos os bytes são zero, o binário é considerado não assinado e o kernel pode rejeitá-lo ou carregá-lo sem verificação, dependendo da política de boot.

### Algoritmo Suportado

| Valor | Algoritmo | Tamanho |
|---:|---|---:|
| 0x01 | Ed25519 | 64 bytes |

Outros algoritmos modernos podem ser adicionados no futuro, mas CRC32 **não** é usado para verificação de integridade.

### Verificação

1. Kernel carrega chaves públicas confiáveis na inicialização (do firmware/bootloader ou armazenamento seguro).
2. Computa hash do arquivo **excluindo** o campo de assinatura (bytes 0–87 + bytes 152 até EOF).
3. Verifica assinatura Ed25519 com a chave pública correspondente.
4. Se inválida: **rejeita execução imediatamente** (retorna erro de assinatura inválida).

---

## Payload dos Segmentos

Os dados brutos dos segmentos começam imediatamente após a tabela de segmentos (offset `96 + segment_count × 40`). Cada segmento é colocado em seu `file_offset` (offset absoluto desde o início do arquivo).

**Regras:**
- `file_offset` **é absoluto** (relativo ao byte 0 do arquivo DEF).
- Segmentos podem ter gaps (padding) entre si no arquivo.
- `file_size == 0` indica segmento BSS (apenas reservado em memória, zero-filled pelo kernel).
- `memory_size > file_size` → diferença é BSS (zero-filled).
- Segmentos **devem** estar ordenados por `vaddr` crescente.
- Sobreposição de `vaddr` entre segmentos é **erro de validação**.

---

## Execução no Kernel

O DNU não segue o modelo Unix de `fork()` + `execve()`. Existe **uma única função de execução** que recebe todos os parâmetros necessários para carregar e iniciar o programa:

```
SYS_EXEC_IMAGE(path, argv, argc, envp, envc, stdio_fds, flags)
  │
  ├─► Abre arquivo, lê cabeçalho DEF (96 bytes)
  │
  ├─► Valida magic == "DNUF"
  │
  ├─► Valida arch == arquitetura atual
  │
  ├─► Valida abi_version compatível com kernel
  │
  ├─► Valida assinatura Ed25519 (se presente)
  │       └─► Falha → retorna ERR_INVALID_SIGNATURE
  │
  ├─► Aloca novo address space (page tables)
  │
  ├─► Para cada segmento:
  │       ├─► Mapeia páginas virtuais em vaddr..vaddr+memory_size
  │       ├─► Define permissões (R/W/X/USER/COW/GUARD/NOCACHE/HUGEPAGE)
  │       ├─► Se file_size > 0: copia dados do arquivo
  │       └─► Se memory_size > file_size: zera região BSS restante
  │
  ├─► Configura stack (com guard page se solicitado)
  │
  ├─► Passa parâmetros de execução (argv, envp, stdio) para o novo processo
  │
  ├─► Define registradores de entrada e ponto de entrada (entry_point)
  │
  └─► Inicia execução em user mode (Ring 3) em entry_point
```

Os parâmetros são passados diretamente na chamada única, ao contrário do modelo Unix onde `fork()` duplica o processo e `execve()` substitui a imagem posteriormente.

---

## Convenção de Chamada de Syscall (ABI x86_64)

O DEF não define a ABI de syscall — ela é definida pela ABI do kernel DNU. Resumo para contexto:

| Registrador | Uso |
|---|---|
| `RAX` | Número da syscall |
| `RDI`, `RSI`, `RDX`, `R10`, `R8`, `R9` | Argumentos 1–6 |
| `RCX`, `R11` | Corrompidos por `syscall` (kernel salva rip/rflags) |
| **Retorno** | `RAX` = valor/ponteiro, `RDX` = código de erro (0 = sucesso) |

**Numeração de syscalls**: Definida em `abi/src/syscall.rs` (crate `dnu-abi`, compartilhada kernel ↔ userspace).

---

## Empacotamento: `dnu-pack` (ELF → DEF)

Como o toolchain Rust padrão produz ELF, o `dnu-pack` (em `tools/dnu-pack.rs`) converte:

```
cargo build --release          # Gera target/release/app (ELF)
dnu-pack app /bin/app          # Converte ELF → DEF
```

### O que `dnu-pack` faz:

1. Parseia ELF via crate `object`
2. Extrai segments `PT_LOAD` (código, dados, rodata, BSS)
3. Constrói cabeçalho DEF com:
   - `entry_point` = `e_entry` do ELF
   - `stack_size` = padrão (1 MiB) ou nota custom
   - `image_base` = 0x400000 (padrão PIE) ou endereço fixo
   - `abi_version` = versão atual do kernel
   - `format_version` = 1
   - `flags` = ASLR_COMPATIBLE | PIE | CAPABILITY_MODE | REQUIRES_STACK_GUARD
4. Opcionalmente assina com Ed25519 (`dnu-sign`)
5. Escreve arquivo DEF final

Não há CRC32 no cabeçalho; a integridade é fornecida pela assinatura Ed25519 opcional.

---

## Constantes e Tipos (Referência de Implementação)

Todos os valores abaixo são definidos em `abi/src/def.rs` (crate `dnu-abi`, compartilhada kernel ↔ userspace).

```rust
// Magic number
pub const DEF_MAGIC: [u8; 4] = *b"DNUF";

// Versões
pub const DEF_FORMAT_VERSION: u16 = 1;

// Tamanhos fixos
pub const DEF_HEADER_SIZE: usize = 96;
pub const DEF_SEGMENT_ENTRY_SIZE: usize = 40;

// Arquiteturas
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefArch {
    Unknown = 0x00,
    X86_64 = 0x01,
    AArch64 = 0x02,
    RiscV64 = 0x03,
    Native = 0xFF,
}

// Tipos de artefato
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefType {
    App = 0x00,
    Service = 0x01,
    Init = 0x02,
    KernelModule = 0x03,
    Bootloader = 0x04,
    Raw = 0xFF,
}

// Flags (bitmask)
pub mod def_flags {
    pub const REQUIRES_STACK_GUARD: u32 = 1 << 0;
    pub const REQUIRES_HEAP_GUARD: u32 = 1 << 1;
    pub const ASLR_COMPATIBLE: u32 = 1 << 2;
    pub const PIE: u32 = 1 << 3;
    pub const STATIC_LINK: u32 = 1 << 4;
    pub const CAPABILITY_MODE: u32 = 1 << 5;
    pub const DEBUG_INFO: u32 = 1 << 6;
    pub const NO_STD: u32 = 1 << 7;
}

// Permissões de segmento (bitmask)
pub mod def_perm {
    pub const READ: u32 = 1 << 0;
    pub const WRITE: u32 = 1 << 1;
    pub const EXECUTE: u32 = 1 << 2;
    pub const USER: u32 = 1 << 3;
    pub const COW: u32 = 1 << 4;
    pub const GUARD: u32 = 1 << 5;
    pub const NOCACHE: u32 = 1 << 6;
    pub const HUGEPAGE: u32 = 1 << 7;
}

// Algoritmos de assinatura
#[repr(u8)]
pub enum SignatureAlgo {
    None = 0x00,
    Ed25519 = 0x01,
}
```

### Structs `#[repr(C)]` para Zero-Copy

```rust
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefHeader {
    pub magic: [u8; 4],
    pub arch: u8,
    pub ty: u8,
    pub format_version: u16,
    pub abi_version: u32,
    pub flags: u32,
    pub segment_count: u32,
    pub reserved: u32,
    pub entry_point: u64,
    pub stack_size: u64,
    pub image_base: u64,
    pub image_size: u64,
    pub capabilities: [u8; 32],
    pub signature: [u8; 64],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefSegment {
    pub file_offset: u64,
    pub file_size: u64,
    pub memory_size: u64,
    pub vaddr: u64,
    pub permissions: u32,
    pub reserved: u32,
}
```

---

## Códigos de Erro de Carregamento

| Código | Constante | Descrição |
|--------|-----------|-----------|
| 1 | `ERR_NOT_DEF` | Magic inválido (não é DEF) |
| 2 | `ERR_ARCH_MISMATCH` | Arquitetura incompatível |
| 3 | `ERR_ABI_VERSION` | Versão da ABI incompatível |
| 5 | `ERR_INVALID_SEGMENT` | Segmento inválido (overlap, alinhamento, permissões) |
| 6 | `ERR_SIGNATURE_INVALID` | Assinatura criptográfica inválida |
| 7 | `ERR_SIGNATURE_ALGO` | Algoritmo de assinatura não suportado |
| 11 | `ERR_MEMORY` | Memória insuficiente para carregar |
| 12 | `ERR_ENTRY_POINT` | Entry point fora de segmento executável |

Não há código de erro para CRC32 (não utilizado).

---

## Exemplo de Layout de Arquivo DEF (Visual)

```
Offset  Tamanho  Conteúdo
──────  ──────  ──────────────────────────────────────────
0x0000     96B  Cabeçalho DEF
0x0060   N×40B  Tabela de Segmentos (ex: 5 segmentos = 200B)
0x0128       -  Início do payload (alinhado conforme segmentos)
0x0128  0x1000  Segmento 0: .text (READ|EXECUTE|USER) @ vaddr 0x400000
0x1128  0x0800  Segmento 1: .rodata (READ|USER) @ vaddr 0x402000
0x1928  0x0400  Segmento 2: .data (READ|WRITE|USER|COW) @ vaddr 0x403000
0x1D28     0B   Segmento 3: .bss (READ|WRITE|USER|COW) @ vaddr 0x404000 (file_size=0)
0x1D28  0x1000  Segmento 4: stack (READ|WRITE|USER|GUARD) @ vaddr 0x7FFFD000
         ...    (gap)
EOF
```

---

## Compatibilidade e Versionamento

- **Format version** (`format_version`): Mudanças breaking no formato do arquivo (cabeçalho, segmentos). Kernel suporta múltiplas versões via dispatch.
- **ABI version** (`abi_version`): Mudanças na interface kernel↔userspace (syscalls, structs, convenções).
- **Política**: ABI estável dentro da mesma major version. Minor adiciona funcionalidades (retrocompatível). Patch corrige bugs.

---

## Diferenças Chave vs ELF

| Aspecto | ELF | DEF |
|---|---|---|
| Visões | Section Headers + Program Headers | Apenas segmentos de execução |
| Parsing | Complexo, múltiplas passagens | Zero-copy, single-pass |
| Assinatura | Externa (GPG, sbctl) | Embutida (Ed25519 no cabeçalho) |
| Stack/Heap | Convenção / linker script | Explícito no cabeçalho |
| Modelo de execução | fork() + execve() (Unix) | Única função SYS_EXEC_IMAGE |
| Integridade | CRC/sumário externo | Assinatura Ed25519 embutida |
| Extensão | `.elf`, sem extensão, `.so` | Sem extensão (binários em `/bin`, `/sbin`) |

---

## Referências de Implementação

| Componente | Localização |
|---|---|
| Definições compartilhadas (header, segment, constants) | `abi/src/def.rs` |
| Loader do kernel (validação, mapeamento, exec) | `src/abi/abi_process.rs`, `src/abi/abi_dispatch.rs` |
| Empacotador `dnu-pack` (ELF → DEF) | `tools/dnu-pack.rs` |
| Assinador `dnu-sign` | `tools/` (futuro) |
| Testes de parsing zero-copy | `tests/` |

---

## Changelog da Especificação

| Versão | Data | Alterações |
|--------|------|------------|
| 1.1 | 2026-11-19 | Ajustado para escopo DNU: sem fork()/execve(), única função de execução SYS_EXEC_IMAGE, sem CRC32, assinatura Ed25519 embutida, cabeçalho 96 bytes, segmentos 40 bytes |
| 1.0 | 2026-11-04 | Especificação inicial (base unix-like, com CRC32) |

---

*Documento mantido pela equipe do DNU. Para discussões técnicas, consulte o repositório principal.*
