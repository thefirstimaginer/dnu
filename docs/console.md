# Console e Display — Kernel DNU

**Versão:** 1.0  
**Status:** Rascunho  
**Aplica-se a:** `src/console/` (framebuffer, serial, utf8, surface)

---

## 1. Introdução

O subsistema de console do kernel DNU fornece saída de texto para o operador do sistema via duas saídas principais:

- **Serial UART** (porta `0x3f8`): usada para diagnóstico precoce durante o boot e logs do kernel.
- **Framebuffer**: usado para exibição gráfica de texto via fonte PSF embarcada.

A inicialização do console é marcada pelo subsistema `Console` durante o boot.

---

## 2. Componentes

| Componente | Arquivo | Descrição |
|------------|---------|-----------|
| **Serial UART** | `src/console/serial.rs` | Escrita na porta serial `0x3f8` para diagnóstico |
| **Framebuffer** | `src/console/surface.rs` | Exibição de texto no framebuffer via fonte PSF |
| **UTF-8** | `src/console/utf8.rs` | Decodificação e escrita de UTF-8 no console |
| **Input** | `src/console/display_input.rs` | Entrada de teclado para o console |
| **Hub** | `src/console/console_hub.rs` | Coordenação dos componentes de console |

---

## 3. Serial UART

A UART em `0x3f8` é a primeira saída disponível durante o boot. É usada para:

- Diagnóstico precoce antes do framebuffer estar pronto.
- Logs do kernel (dmesg).
- Depuração de falhas de boot.

Funções principais:

| Função | Descrição |
|--------|-----------|
| `uart::init()` | Inicializa a porta serial |
| `uart::write(bytes)` | Escreve bytes na serial |
| `serial_write(bytes)` | Interface do kernel para escrita na serial |

---

## 4. Framebuffer

O framebuffer é inicializado depois da serial e do console estar pronto. Usa a fonte PSF embarcada em `src/assets/zap-vga16.psf` para renderizar caracteres.

### Inicialização

Durante o boot, se o Limine fornecer um framebuffer válido:

```rust
if !boot_info.framebuffer.is_null() {
    let fb_response = unsafe { &*boot_info.framebuffer };
    let mut fb_iter = unsafe { fb_response.framebuffers() };
    if let Some(fb) = fb_iter.next() {
        framebuffer::init(Some(fb), boot_info.hhdm_offset);
    }
}
```

### Funções Principais

| Função | Descrição |
|--------|-----------|
| `framebuffer::init(fb, hhdm_offset)` | Inicializa o framebuffer com o buffer do Limine |
| `framebuffer::write_utf8(text)` | Escreve texto UTF-8 no framebuffer |
| `framebuffer::write_char(c)` | Escreve um caractere |
| `framebuffer::write_ansi(bytes)` | Escreve bytes ANSI (sequências de escape) |
| `framebuffer::set_hhdm_offset(offset)` | Define o offset HHDM para endereçamento do framebuffer |
| `framebuffer::load_font(font)` | Carrega uma fonte PSF da RAMFS |

---

## 5. Console Unificado

O kernel fornece `console_write(text)` e `console_write_bytes(bytes)` que escrevem simultaneamente no kernel log, serial e framebuffer:

| Função | Saídas |
|--------|--------|
| `console_write(text)` | kernel_log + framebuffer (com timestamp por linha) |
| `console_write_bytes(bytes)` | serial UART + framebuffer (sem formatação) |

Cada linha no `console_write` recebe um timestamp automático baseado no tempo decorrido desde o boot.

---

## 6. Syscalls Relacionadas

| Syscall | Número | Descrição |
|---------|--------|-----------|
| `SYS_FB_INFO` | 19 | Obtém informações do framebuffer |
| `SYS_FB_MAP` | 20 | Mapeia o framebuffer no userspace |
| `SYS_FB_WRITE` | 21 | Escreve no framebuffer |
| `SYS_KEYBOARD_READ` | 22 | Lê evento de teclado |
| `SYS_MOUSE_READ` | 23 | Lê evento de mouse |

> **Importante:** A GUI fica no userspace. O kernel expõe apenas syscalls de framebuffer; a interface gráfica (ws_server, wm, explorer) roda fora do kernel.

---

## 7. Fonte PSF

A fonte padrão é `zap-vga16.psf`, embarcada em `src/assets/zap-vga16.psf`. Fontes alternativas podem ser carregadas da RAMFS via `ramfs::find_font()`.

---

## Referências

- [Especificação de Boot](./bootspec.md) — inicialização do console durante o boot
- [ABI e Syscalls](./abispec.md) — syscalls de framebuffer e entrada
- [Sistemas de Arquivos](./fsspec.md) — RAMFS e fontes PSF
- [Convenção de Commits](./commiting.md) — regra 2: GUI fica no userspace
