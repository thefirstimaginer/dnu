//! Saídas do kernel: framebuffer, serial 16550 e decodificador UTF-8.
//!
//! O log circular (`dmesg`) é declarado em `main.rs` com `#[path]`, porque é
//! compartilhado pelo console e pela syscall `SYS_DMESG`.
//!
//! `main` reexporta [`serial`] e reúne os helpers de texto do console.
pub mod serial;
pub mod utf8;

#[path = "framebuffer.rs"]
pub mod framebuffer;
