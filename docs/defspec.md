# Formato DEF (Dreamcore "DNU" Executable Format)

O **DEF** (DNU Executable Format) é o formato nativo de executáveis do Kernel DNU. Projetado para simplicidade, segurança e parsing zero-copy em Rust, elimina a complexidade histórica do ELF mantendo apenas a visão de execução (segmentos carregáveis).

> **Nota importante**: Apenas programas DEF são suportados por distros que usam o DNU como base. O kernel continua sendo compilado em ELF para compatibilidade com o bootloader Limine. O loader ELF foi removido do kernel: `SYS_EXEC`/`SYS_EXECVE`/`SYS_SPAWN` rejeitam qualquer imagem que não comece com o magic `DNUF` (0x44 0x4E 0x55 0x46).

---

## Visão Geral da Estrutura

```
┌─────────────────────────────────────────────┐
│           Cabeçalho DEF (80 bytes)          │  ← Zero-copy mappable
├─────────────────────────────────────────────┤
│         Tabela de Segmentos (N × 48 B)      │
├─────────────────────────────────────────────┤
│              Payload dos Segmentos          │  ← Dados brutos (código/dados)
└─────────────────────────────────────────────┘
```

---

## Cabeçalho DEF (80 bytes)

O cabeçalho é desenhado para ser diretamente mapeável em memória via `#[repr(C, packed)]` em Rust, permitindo cast direto de `&[u8]` para a struct do cabeçalho (zero-copy parsing).

| Offset | Tamanho | Campo | Descrição |
|---:|---:|---|---|
| 0 | 4 bytes | `magic` | Identificador: `0x44 0x4E 0x55 0x46` (`"DNUF"`) |
| 4 | 1 byte | `arch` | Arquitetura alvo (`DefArch`) |
| 5 | 1 byte | `type` | Tipo de artefato (`DefType`) |
| 6 | 2 bytes | `format_version` | Versão do formato DEF (atual: 1) |
| 8 | 4 bytes | `abi_version` | Versão da ABI do ImagineOS (compatibilidade) |
| 12 | 4 bytes | `flags` | Flags globais (`DefFlags`) |
| 16 | 4 bytes | `segment_count` | Número de entradas na tabela de segmentos |
| 20 | 4 bytes | `security_offset` | Offset da seção de metadados de segurança (0 = ausente) |
| 24 | 4 bytes | `security_size` | Tamanho da seção de segurança em bytes |
| 28 | 4 bytes | `signature_offset` | Offset do bloco de assinatura criptográfica (0 = não assinado) |
| 32 | 4 bytes | `signature_size` | Tamanho da assinatura em bytes |
| 36 | 8 bytes | `entry_point` | Endereço virtual do ponto de entrada (`_start`) |
| 44 | 8 bytes | `stack_size` | Tamanho inicial da pilha solicitado (bytes, alinhado a página) |
| 52 | 8 bytes | `image_base` | Endereço-base preferido da imagem (ASLR: kernel pode ignorar) |
| 60 | 8 bytes | `image_size` | Tamanho total da imagem na memória virtual (inclui BSS) |
| 68 | 4 bytes | `heap_size` | Tamanho inicial do heap solicitado (bytes, 0 = padrão) |
| 72 | 4 bytes | `reserved` | Reservado para expansão futura (deve ser 0) |
| 76 | 4 bytes | `crc32` | CRC32 do cabeçalho (bytes 0–75) para detecção de corrupção |

**Tamanho total do cabeçalho: 80 bytes** (múltiplo de 16 para alinhamento de cache line).

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
| 6 | `DEBUG_INFO` | Contém informações de depuração (DWARF em seção separada) |
| 7 | `NO_STD` | Não usa `dnu-std` (bare metal / runtime customizado) |
| 8–31 | — | Reservados (devem ser 0) |

---

## Tabela de Segmentos

Cada entrada tem **48 bytes**. A tabela começa no offset **80** (imediatamente após o cabeçalho).

| Offset na entrada | Tamanho | Campo | Descrição |
|---:|---:|---|---|
| 0 | 8 bytes | `file_offset` | Offset absoluto do segmento no arquivo DEF (desde o byte 0) |
| 8 | 8 bytes | `file_size` | Tamanho dos dados do segmento no arquivo (pode ser 0 para BSS) |
| 16 | 8 bytes | `memory_size` | Tamanho reservado na memória virtual (≥ `file_size`) |
| 24 | 8 bytes | `vaddr` | Endereço virtual do segmento (relativo a `image_base` se PIE) |
| 32 | 4 bytes | `permissions` | Permissões (`DefPermissions`, bitmask) |
| 36 | 4 bytes | `alignment` | Alinhamento requerido em memória (potência de 2, 0 = padrão 4K) |
| 40 | 8 bytes | `reserved` | Reservado |

**Tamanho total por entrada: 48 bytes**

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

## Seção de Metadados de Segurança (Opcional)

Localizada em `security_offset` / `security_size`. Permite ao kernel aplicar sandboxing **antes** da primeira instrução do programa.

Formato: sequência de entradas TLV (Type-Length-Value).

```
┌──────────────┬──────────────┬─────────────────────┐
│  Type (1B)   │ Length (2B)  │  Value (Length B)   │
└──────────────┴──────────────┴─────────────────────┘
```

### Tipos de Metadados de Segurança

| Type | Nome | Value | Descrição |
|---:|---|---|---|
| 0x01 | `SYSCALL_MASK` | Bitmap de 256 bits (32 bytes) | Quais syscalls o programa pode chamar (1 = permitido) |
| 0x02 | `CAPABILITIES` | Lista de u32 (capability IDs) | Capacidades concedidas ao processo (arquivos, sockets, devices) |
| 0x03 | `FS_ALLOWLIST` | Lista de strings (caminhos) | Caminhos do FS que o programa pode acessar (prefix match) |
| 0x04 | `NET_ALLOWLIST` | Lista de (IP, porta, protocolo) | Endereços de rede permitidos para conexão/bind |
| 0x05 | `DEVICE_ALLOWLIST` | Lista de device IDs | Dispositivos de hardware acessíveis |
| 0x06 | `RESOURCE_LIMITS` | Struct `ResourceLimits` | Limites: max memória, max threads, max FDs, CPU quota |
| 0x07 | `SECCOMP_POLICY` | Bytes de política seccomp-bpf | Filtro BPF estilo Linux seccomp (para compatibilidade) |
| 0xFF | `CUSTOM` | Dados opacos | Extensões específicas de aplicação |

### Estrutura `ResourceLimits` (para type 0x06)

```rust
#[repr(C, packed)]
struct ResourceLimits {
    max_memory_bytes: u64,      // 0 = ilimitado
    max_threads: u32,           // 0 = ilimitado
    max_fds: u32,               // 0 = ilimitado
    cpu_time_ms: u64,           // 0 = ilimitado
    max_children: u32,          // 0 = ilimitado
    reserved: [u32; 3],         // Padding para 32 bytes
}
```

---

## Assinatura Criptográfica (Opcional)

Localizada em `signature_offset` / `signature_size`. Verificada pelo kernel **antes** de qualquer mapeamento de memória.

### Formato do Bloco de Assinatura

```
┌──────────────┬──────────────┬──────────────┬─────────────────────┐
│  Algoritmo   │  Key ID      │  Reserved    │  Signature Bytes    │
│   (1 byte)   │  (4 bytes)   │  (3 bytes)   │  (variable)         │
└──────────────┴──────────────┴──────────────┴─────────────────────┘
```

### Algoritmos Suportados

| Valor | Algoritmo | Tamanho da Assinatura |
|---:|---|---:|
| 0x00 | Nenhuma / Inválida | — |
| 0x01 | Ed25519 | 64 bytes |
| 0x02 | ECDSA P-256 (SHA-256) | 64 bytes (r||s) |
| 0x03 | RSA-PSS (SHA-256, 2048-bit) | 256 bytes |
| 0x04 | RSA-PSS (SHA-512, 4096-bit) | 512 bytes |

### Verificação

1. Kernel carrega chaves públicas confiáveis na inicialização (do firmware/bootloader ou armazenamento seguro).
2. Computa hash do arquivo **excluindo** o bloco de assinatura (bytes 0 até `signature_offset - 1` + bytes após `signature_offset + signature_size` até EOF).
3. Verifica assinatura com a chave pública correspondente ao `Key ID`.
4. Se inválida: **rejeita execução imediatamente** (retorna `ERR_INVALID_SIGNATURE` para o chamador de `SYS_EXEC`).

---

## Payload dos Segmentos

Os dados brutos dos segmentos começam imediatamente após a tabela de segmentos (offset `80 + segment_count × 48`). Cada segmento é colocado em seu `file_offset` (offset absoluto desde o início do arquivo).

**Regras:**
- `file_offset` **é absoluto** (relativo ao byte 0 do arquivo DEF).
- Segmentos podem ter gaps (padding) entre si no arquivo.
- `file_size == 0` indica segmento BSS (apenas reservado em memória, zero-filled pelo kernel).
- `memory_size > file_size` → diferença é BSS (zero-filled).
- Segmentos **devem** estar ordenados por `vaddr` crescente.
- Sobreposição de `vaddr` entre segmentos é **erro de validação**.

---

## Carregamento pelo Kernel (Fluxo Resumido)

```
SYS_EXEC(path)
  │
  ├─► Abre arquivo, lê primeiros 80 bytes (cabeçalho)
  │
  ├─► Valida magic == "DNUF"
  │
  ├─► Valida arch == arquitetura atual
  │
  ├─► Valida abi_version compatível com kernel
  │
  ├─► Valida CRC32 do cabeçalho
  │
  ├─► Se security_offset > 0: parseia metadados, prepara sandbox
  │
  ├─► Se signature_offset > 0: verifica assinatura criptográfica
  │       └─► Falha → retorna ERR_INVALID_SIGNATURE
  │
  ├─► Aloca novo address space (páginas de página, page tables)
  │
  ├─► Para cada segmento:
  │       ├─► Mapeia páginas virtuais em vaddr..vaddr+memory_size
  │       ├─► Define permissões (R/W/X/USER/COW/GUARD/HUGEPAGE)
  │       ├─► Se file_size > 0: copia dados do arquivo (file_offset, file_size)
  │       └─► Se memory_size > file_size: zera região BSS restante
  │
  ├─► Configura stack em vaddr_stack (topo = stack_base + stack_size)
  │       └─► Se flag REQUIRES_STACK_GUARD: adiciona guard page no topo
  │
  ├─► Configura heap se heap_size > 0
  │
  ├─► Define registradores de entrada (x86_64 System V AMD64):
  │       RDI = argc, RSI = argv_ptr, RDX = envp_ptr, R10 = auxv_ptr
  │
  ├─► Salva estado de retorno (sysret) para entry_point
  │
  └─► Retorna para user mode (Ring 3) em entry_point
```

---

## Convenção de Chamada de Syscall (ABI x86_64)

O DEF não define a ABI de syscall — ela é definida pela ABI do ImagineOS. Resumo para contexto:

| Registrador | Uso |
|---|---|
| `RAX` | Número da syscall |
| `RDI`, `RSI`, `RDX`, `R10`, `R8`, `R9` | Argumentos 1–6 |
| `RCX`, `R11` | Corrompidos por `syscall` (kernel salva rip/rflags) |
| **Retorno** | `RAX` = valor/ponteiro, `RDX` = código de erro (0 = sucesso) |

**Numeração de syscalls**: Definida em `imagine-common/abi/src/syscalls.rs` (compartilhada kernel/userspace via crate `imagine-abi`).

---

## Empacotamento: `dnu-pack` (ELF → DEF)

Como o toolchain Rust padrão produz ELF, o `dnu-pack` (em `dnu/tools/dnu-pack/`) converte:

```
cargo build --release          # Gera target/release/app (ELF)
dnu-pack app app.def           # Converte ELF → DEF
# Ou integrado no build.rs:
dnu-pack target/release/app /bin/app
```

### O que `dnu-pack` faz:

1. Parseia ELF via crate `object`
2. Extrai segments `PT_LOAD` (código, dados, rodata, BSS)
3. Constrói cabeçalho DEF com:
   - `entry_point` = `e_entry` do ELF
   - `stack_size` = padrão (1 MiB) ou nota custom `.note.stack_size`
   - `heap_size` = padrão (4 MiB) ou nota custom
   - `image_base` = 0x400000 (padrão PIE) ou endereço fixo
   - `abi_version` = versão atual do kernel
   - `format_version` = 1
   - `flags` = ASLR_COMPATIBLE | PIE | CAPABILITY_MODE | REQUIRES_STACK_GUARD
4. Gera seção de segurança a partir de `DEF_MANIFEST` (TOML/JSON embutido no build)
5. Opcionalmente assina com `dnu-sign` (chave Ed25519)
6. Escreve arquivo DEF final

### Manifesto de Segurança (exemplo TOML para `dnu-pack`)

```toml
# def-manifest.toml
[security]
syscalls = ["open", "read", "write", "close", "mmap", "exit", "spawn"]
capabilities = ["fs:/home/user", "net:tcp:outbound"]
fs_allowlist = ["/home/user", "/tmp"]
net_allowlist = [{proto = "tcp", direction = "outbound"}]
resource_limits = { max_memory = "64MiB", max_threads = 32, max_fds = 256 }

[build]
stack_size = "1MiB"
heap_size = "4MiB"
pie = true
aslr = true
stack_guard = true
```

---

## Constantes e Tipos (Referência de Implementação)

Todos os valores abaixo são definidos em **`imagine-common/abi/src/def.rs`** (crate compartilhada kernel ↔ userspace).

```rust
// Magic number
pub const DEF_MAGIC: [u8; 4] = *b"DNUF";
pub const DEF_MAGIC_U32: u32 = u32::from_le_bytes(*b"DNUF"); // 0x46554E44

// Versões
pub const DEF_FORMAT_VERSION: u16 = 1;
pub const DEF_ABI_VERSION: u32 = 0x00010000; // major.minor.patch.build encoded

// Tamanhos fixos
pub const DEF_HEADER_SIZE: usize = 80;
pub const DEF_SEGMENT_ENTRY_SIZE: usize = 48;

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

// Tipos de metadados de segurança
pub mod security_type {
    pub const SYSCALL_MASK: u8 = 0x01;
    pub const CAPABILITIES: u8 = 0x02;
    pub const FS_ALLOWLIST: u8 = 0x03;
    pub const NET_ALLOWLIST: u8 = 0x04;
    pub const DEVICE_ALLOWLIST: u8 = 0x05;
    pub const RESOURCE_LIMITS: u8 = 0x06;
    pub const SECCOMP_POLICY: u8 = 0x07;
    pub const CUSTOM: u8 = 0xFF;
}

// Algoritmos de assinatura
#[repr(u8)]
pub enum SignatureAlgo {
    None = 0x00,
    Ed25519 = 0x01,
    EcdsaP256 = 0x02,
    RsaPss2048 = 0x03,
    RsaPss4096 = 0x04,
}
```

### Structs `#[repr(C, packed)]` para Zero-Copy

```rust
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct DefHeader {
    pub magic: [u8; 4],
    pub arch: u8,
    pub ty: u8,
    pub format_version: u16,
    pub abi_version: u32,
    pub flags: u32,
    pub segment_count: u32,
    pub security_offset: u32,
    pub security_size: u32,
    pub signature_offset: u32,
    pub signature_size: u32,
    pub entry_point: u64,
    pub stack_size: u64,
    pub image_base: u64,
    pub image_size: u64,
    pub heap_size: u64,
    pub reserved: u32,
    pub crc32: u32,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct DefSegment {
    pub file_offset: u64,
    pub file_size: u64,
    pub memory_size: u64,
    pub vaddr: u64,
    pub permissions: u32,
    pub alignment: u32,
    pub reserved: u64,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct DefSignatureBlock {
    pub algorithm: u8,
    pub key_id: u32,
    pub reserved: [u8; 3],
    // signature bytes follow immediately
}
```

---

## Códigos de Erro de Carregamento (Retornados por `SYS_EXEC`)

| Código | Constante | Descrição |
|---:|---|---|
| 1 | `ERR_NOT_DEF` | Magic inválido (não é DEF) |
| 2 | `ERR_ARCH_MISMATCH` | Arquitetura incompatível |
| 3 | `ERR_ABI_VERSION` | Versão da ABI incompatível |
| 4 | `ERR_CRC32` | CRC32 do cabeçalho falhou |
| 5 | `ERR_INVALID_SEGMENT` | Segmento inválido (overlap, alinhamento, permissões) |
| 6 | `ERR_SIGNATURE_INVALID` | Assinatura criptográfica inválida |
| 7 | `ERR_SIGNATURE_ALGO` | Algoritmo de assinatura não suportado |
| 8 | `ERR_SECURITY_PARSE` | Falha ao parsear metadados de segurança |
| 9 | `ERR_CAPABILITY_DENIED` | Capacidade requerida não concedida |
| 10 | `ERR_RESOURCE_LIMIT` | Limite de recurso excedido |
| 11 | `ERR_MEMORY` | Memória insuficiente para carregar |
| 12 | `ERR_ENTRY_POINT` | Entry point fora de segmento executável |

---

## Exemplo de Layout de Arquivo DEF (Visual)

```
Offset  Tamanho  Conteúdo
──────  ──────  ──────────────────────────────────────────
0x0000     80B  Cabeçalho DEF
0x0050   N×48B  Tabela de Segmentos (ex: 5 segmentos = 240B)
0x0140       -  Início do payload (alinhado conforme segmentos)
0x0140  0x1000  Segmento 0: .text (READ|EXECUTE|USER) @ vaddr 0x400000
0x1140  0x0800  Segmento 1: .rodata (READ|USER) @ vaddr 0x402000
0x1940  0x0400  Segmento 2: .data (READ|WRITE|USER|COW) @ vaddr 0x403000
0x1D40     0B   Segmento 3: .bss (READ|WRITE|USER|COW) @ vaddr 0x404000 (file_size=0)
0x1D40  0x1000  Segmento 4: stack (READ|WRITE|USER|GUARD) @ vaddr 0x7FFFD000
         ...    (gap)
    ...      ...  Seção de segurança (se security_offset > 0)
    ...      ...  Bloco de assinatura (se signature_offset > 0)
EOF
```

---

## Compatibilidade e Versionamento

- **Format version** (`format_version`): Mudanças breaking no formato do arquivo (cabeçalho, segmentos). Kernel suporta múltiplas versões via dispatch.
- **ABI version** (`abi_version`): Mudanças na interface kernel↔userspace (syscalls, structs, convenções). Codificado como `major<<24 | minor<<16 | patch<<8 | build`.
  - Kernel define `MIN_ABI_VERSION` e `MAX_ABI_VERSION` suportados.
  - Binários fora do range → `ERR_ABI_VERSION`.
- **Política**: ABI estável dentro da mesma major version. Minor adiciona syscalls/structs (retrocompatível). Patch corrige bugs.

---

## Diferenças Chave vs ELF

| Aspecto | ELF | DEF |
|---|---|---|
| Visões | Section Headers (link) + Program Headers (exec) | Apenas segmentos de execução |
| Parsing | Complexo, múltiplas passagens | Zero-copy, single-pass |
| Metadados segurança | Externo (separate files, IMA, etc.) | Embutido no binário |
| Assinatura | Externa (detached, GPG, sbctl) | Embutida, verificada no load |
| Stack/Heap | Convenção / linker script | Explícito no cabeçalho |
| Capabilities | Runtime (libcap, seccomp) | Declarativo, enforced no load |
| Extensão | `.elf`, sem extensão, `.so` | Sem extensão (binários), `.xapp` (pacotes Liminal futuros) |

---

## Referências de Implementação

| Componente | Localização |
|---|---|
| Definições compartilhadas (header, segment, constants) | `imagine-common/abi/src/def.rs` |
| Loader do kernel (validação, mapeamento, exec) | `dnu/src/abi/abi_exec.rs`, `dnu/src/exec_loader.rs` |
| Empacotador `dnu-pack` (ELF → DEF) | `dnu/tools/dnu-pack/` |
| Assinador `dnu-sign` | `dnu/tools/dnu-sign/` |
| Testes de parsing zero-copy | `dnu/tests/def_parsing.rs` |
| Manifestos de segurança de exemplo | `imagine-common/def-manifests/` |

---

## Changelog da Especificação

| Versão | Data | Autor | Alterações |
|---:|---|---|---|
| 1.0 | 2026-11-04 | — | Especificação inicial completa com segurança, assinaturas, capacidades, zero-copy |

---

*Documento mantido pela equipe do ImagineOS. Para discussões técnicas, consulte o repositório principal.*