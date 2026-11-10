//! Contrato da ABI do Dreamcore Kernel.
//! Apenas constantes e estruturas compartilhadas entre kernel e userspace.
//! Sem lógica de implementação.

#![no_std]

pub mod syscall;
pub mod types;

pub use syscall::*;
pub use types::*;
