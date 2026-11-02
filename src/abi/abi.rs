//! Implementação do contrato de syscalls no lado do kernel.
//!
//! O ponto de entrada é `dispatch::dreamcore_syscall_dispatch` (chamado pelo
//! stub `int 0x80` em `arch/x86_64/idt.rs`), que decodifica os registradores do
//! `trap::TrapFrame` e roteia para um dos módulos:
//!
//! * `fs` — arquivos, diretórios, descritores (`open/read/write/lseek/fstat`);
//! * `process` — processos, sinais, pipes e `exec*`/`spawn`/`wait`;
//! * `thread` — threads, `futex`, `nanosleep`, relógios e `mmap`/`munmap`;
//! * `display` — framebuffer, superfícies, entrada e console;
//! * `system` — identidade, tempo, desligamento, instalação e `dmesg`;
//! * `access` — checagem de permissão de caminhos;
//! * `input` — leitura de teclado/serial para o console do kernel;
//! * `user` — cópia validada de/para o espaço de usuário.
//!
//! Os números das syscalls e os tamanhos limites vêm de `imagineos_abi`, o
//! mesmo crate que a libc e o runtime de usuário usam, para que kernel e
//! userspace não possam divergir.

// `src/` é uma árvore plana: as folhas da ABI são declaradas com `#[path]`
// relativo a este módulo-hub.
#[path = "abi_access.rs"]
pub mod access;
#[path = "abi_dispatch.rs"]
pub mod dispatch;
#[path = "abi_display.rs"]
pub mod display;
#[path = "abi_fs.rs"]
pub mod fs;
#[path = "abi_console_input.rs"]
pub mod input;
#[path = "abi_process.rs"]
pub mod process;
#[path = "abi_system.rs"]
pub mod system;
#[path = "abi_thread.rs"]
pub mod thread;
#[path = "abi_trap.rs"]
pub mod trap;
#[path = "abi_user.rs"]
pub mod user;

/// Toca os estados globais de I/O da camada ABI (teclado e decodificador da
/// serial) para que o linker não os descarte e para que o custo de inicialização
/// (nenhum) fique explícito na entrada do kernel.
#[inline(always)]
pub fn anchor_runtime_state() {
    let _ = display::KEYBOARD.get();
    let _ = input::SERIAL_UTF8.get();
}
