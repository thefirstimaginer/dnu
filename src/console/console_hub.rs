//! Saídas do kernel: framebuffer, serial 16550 e decodificador UTF-8.
//!
//! O log circular (`dmesg`) é declarado em `main.rs`, porque é compartilhado
//! pelo console e pela syscall `SYS_DMESG`.
//!
//! `main` reexporta [`serial`] e reúne os helpers de texto do console.

#[path = "serial.rs"]
pub mod serial;
#[path = "utf8.rs"]
pub mod utf8;
#[path = "framebuffer.rs"]
pub mod framebuffer;
