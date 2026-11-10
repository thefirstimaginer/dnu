//! Módulo de compatibilidade para o dnu-pack (host).
//!
//! Re-exporta apenas os módulos necessários para empacotar ELF → DEF:
//! `def` (estruturas do formato), `syscall` (números de syscall) e
//! `ABI_VERSION`. Não importa módulos do kernel (process, framebuffer, etc.).
//! Todos os símbolos vêm de `dnu-abi` via `src/abi/`.

#[path = "../../src/abi/def.rs"]
pub mod def;

#[path = "../../src/abi/syscall.rs"]
pub mod syscall;

// Para que `crate::abi::ABI_VERSION` funcione (referenciado em def.rs)
pub const ABI_VERSION: u64 = 1;

pub use def::*;
pub use syscall::*;
