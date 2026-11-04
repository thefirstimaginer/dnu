//! Módulo de compatibilidade para o dnu-pack (host).
//!
//! Re-exporta apenas os módulos necessários para empacotar ELF → DEF:
//! `def` (estruturas do formato), `syscall` (números de syscall) e
//! `ABI_VERSION`. Não importa módulos do kernel (process, framebuffer, etc.).

#[path = "../../imagine-base/abi/src/def.rs"]
pub mod def;

#[path = "../../imagine-base/abi/src/syscall.rs"]
pub mod syscall;

// Para que `crate::abi::imagineos_abi::{def, ABI_VERSION}` funcione
pub mod imagineos_abi {
    pub use super::def as def;
    pub const ABI_VERSION: u64 = 1;
}

// Para que `crate::abi::ABI_VERSION` funcione (referenciado em def.rs)
pub const ABI_VERSION: u64 = 1;

pub use def::*;
pub use syscall::*;
pub use imagineos_abi::*;
