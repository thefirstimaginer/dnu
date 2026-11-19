# ABI e Syscalls do Kernel DNU

**Versão:** 1.0  
**Status:** Rascunho  
**Aplica-se a:** Kernel DNU e userspace do ImagineOS

---

## 1. Introdução

A **ABI** (Application Binary Interface) do kernel DNU define o contrato entre o kernel e programas de userspace. Inclui números de syscall, convenções de chamada, códigos de erro e structs compartilhadas. O contrato é mantido no crate `dnu-abi` (`abi/`), usado tanto pelo kernel quanto por ferramentas de host e libc de userspace, garantindo que não haja divergência entre os dois lados.

---

## 2. Convenção de Chamada de Syscall (x86_64)

O kernel DNU usa a instrução `syscall` (não `int 0x80`). Os registradores seguem a convenção System V AMD64 adaptada:

| Registrador | Uso |
|-------------|-----|
| `RAX` | Número da syscall |
| `RDI` | Argumento 1 |
| `RSI` | Argumento 2 |
| `RDX` | Argumento 3 |
| `R10` | Argumento 4 (substitui RCX na convenção syscall) |
| `R8` | Argumento 5 |
| `R9` | Argumento 6 |
| `RCX` | Corrompido por `syscall` (kernel salva RIP) |
| `R11` | Corrompido por `syscall` (kernel salva RFLAGS) |

**Retorno:** `RAX` contém o valor/ponteiro de retorno; `RDX` contém o código de erro (0 = sucesso).

O handler de syscall é configurado via MSRs (`IA32_LSTAR`, `IA32_STAR`, `IA32_FMASK`) durante o boot, em `idt::configure_syscall_msrs()`.

---

## 3. Números de Syscall

Os números de syscall são definidos em `abi/src/syscall.rs` e reexportados pelo kernel via `crate::abi::syscall::SYS_*`.

| Syscall | Número | Descrição |
|---------|--------|-----------|
| `SYS_EXIT` | 0 | Encerra o processo atual |
| `SYS_READ` | 1 | Lê de um descritor de arquivo |
| `SYS_WRITE` | 2 | Escreve em um descritor de arquivo |
| `SYS_OPEN` | 3 | Abre um arquivo |
| `SYS_CLOSE` | 4 | Fecha um descritor de arquivo |
| `SYS_STAT` | 5 | Obtém metadados de arquivo |
| `SYS_LSEEK` | 6 | Move o cursor de leitura/escrita |
| `SYS_MMAP` | 7 | Mapeia memória |
| `SYS_MUNMAP` | 8 | Desmapeia memória |
| `SYS_SPAWN` | 9 | Cria um novo processo |
| `SYS_WAIT` | 10 | Aguarda término de processo filho |
| `SYS_EXEC` | 11 | Carrega e executa um programa DEF |
| `SYS_FORK` | 12 | Cria um processo filho (cópia do atual) |
| `SYS_KILL` | 13 | Envia sinal a um processo |
| `SYS_GETPID` | 14 | Retorna o PID do processo atual |
| `SYS_GETPPID` | 15 | Retorna o PID do processo pai |
| `SYS_NANOSLEEP` | 16 | Suspende execução por um intervalo de tempo |
| `SYS_FUTEX` | 17 | Operação de futex (sincronização entre threads/processos) |
| `SYS_CLOCK_GETTIME` | 18 | Obtém tempo de um relógio |
| `SYS_FB_INFO` | 19 | Obtém informações do framebuffer |
| `SYS_FB_MAP` | 20 | Mapeia o framebuffer no userspace |
| `SYS_FB_WRITE` | 21 | Escreve no framebuffer |
| `SYS_KEYBOARD_READ` | 22 | Lê evento de teclado |
| `SYS_MOUSE_READ` | 23 | Lê evento de mouse |
| `SYS_DMESG` | 24 | Lê o buffer de log do kernel |
| `SYS_SHUTDOWN` | 25 | Desliga ou reinicia o sistema |
| `SYS_CHDIR` | 26 | Muda o diretório de trabalho corrente |
| `SYS_MKDIR` | 27 | Cria um diretório |
| `SYS_RMDIR` | 28 | Remove um diretório |
| `SYS_UNLINK` | 29 | Remove um arquivo |
| `SYS_RENAME` | 30 | Renomeia um arquivo ou diretório |
| `SYS_DUP` | 31 | Duplica um descritor de arquivo |
| `SYS_PIPE` | 32 | Cria um pipe |
| `SYS_IOCTL` | 33 | Operação de controle de dispositivo |

> **Nota:** Esta numeração pode evoluir. O contrato canônico vive em `abi/src/syscall.rs`. Alterações na numeração são *breaking changes* e devem ser documentadas no rodapé do commit (veja `commiting.md`).

---

## 4. Códigos de Erro

Os códigos de erro são retornados em `RDX` (ou como valor negativo em `RAX`, dependendo da convenção da libc wrapper).

| Código | Constante | Descrição |
|--------|-----------|-----------|
| 0 | `ERR_NONE` | Sucesso |
| 1 | `ERR_NOT_DEF` | Arquivo não é um DEF válido |
| 2 | `ERR_ARCH_MISMATCH` | Arquitetura incompatível |
| 3 | `ERR_ABI_VERSION` | Versão da ABI incompatível |
| 4 | `ERR_CRC32` | CRC32 do cabeçalho falhou |
| 5 | `ERR_INVALID_SEGMENT` | Segmento inválido |
| 6 | `ERR_SIGNATURE_INVALID` | Assinatura criptográfica inválida |
| 7 | `ERR_SIGNATURE_ALGO` | Algoritmo de assinatura não suportado |
| 8 | `ERR_SECURITY_PARSE` | Falha ao parsear metadados de segurança |
| 9 | `ERR_CAPABILITY_DENIED` | Capacidade requerida não concedida |
| 10 | `ERR_RESOURCE_LIMIT` | Limite de recurso excedido |
| 11 | `ERR_MEMORY` | Memória insuficiente |
| 12 | `ERR_ENTRY_POINT` | Entry point fora de segmento executável |
| 13 | `ERR_NOT_FOUND` | Arquivo ou recurso não encontrado |
| 14 | `ERR_PERMISSION_DENIED` | Permissão negada |
| 15 | `ERR_INVALID_ARG` | Argumento inválido |
| 16 | `ERR_ALREADY_EXISTS` | Recurso já existe |
| 17 | `ERR_BUSY` | Recurso ocupado |
| 18 | `ERR_TIMEDOUT` | Operação excedeu o tempo limite |
| 19 | `ERR_INTERRUPTED` | Operação interrompida por sinal |
| 20 | `ERR_NOT_IMPLEMENTED` | Funcionalidade não implementada |
| 21 | `ERR_SYMBOL_UNDEFINED` | Símbolo não encontrado em DLL |
| 22 | `ERR_NO_DEV` | Dispositivo não disponível |

---

## 5. Structs Compartilhadas (ABI)

O crate `dnu-abi` (`abi/src/`) define structs `#[repr(C)]` compartilhadas entre kernel e userspace:

| Arquivo | Conteúdo |
|---------|----------|
| `abi/src/def.rs` | Structs do formato DEF (cabeçalho, segmentos, assinatura) |
| `abi/src/syscall.rs` | Números de syscall e códigos de erro |
| `abi/src/types.rs` | Tipos primitivos e `ABI_VERSION` |

Exemplo de struct compartilhada (`abi/src/types.rs`):

```rust
pub const ABI_VERSION: u64 = 1;

#[repr(C)]
pub struct Stat {
    pub dev: u64,
    pub ino: u64,
    pub mode: u32,
    pub nlink: u32,
    pub uid: u32,
    pub gid: u32,
    pub rdev: u64,
    pub size: u64,
    pub atime: u64,
    pub mtime: u64,
    pub ctime: u64,
    pub blksize: u64,
    pub blocks: u64,
}
```

---

## 6. Versionamento da ABI

A ABI segue versionamento semântico:

- **Major:** mudanças breaking (remoção de syscalls, mudança de numeração, mudança de structs).
- **Minor:** adição de novas syscalls ou campos (retrocompatível).
- **Patch:** correções de bugs sem mudar a interface.

O kernel define `MIN_ABI_VERSION` e `MAX_ABI_VERSION` suportados. Binários fora do range retornam `ERR_ABI_VERSION`.

---

## Referências

- [Formato DEF](./defspec.md) — formato de executáveis userspace
- [DNU Linkable Library](./dllspec.md) — bibliotecas compartilhadas
- [Convenção de Commits](./commiting.md) — regras para commits que alteram a ABI
