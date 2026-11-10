//! Reexporta os números de syscall do contrato `dnu-abi`.
//! Este módulo mantém o caminho `crate::abi::syscall::SYS_*` funcionando
//! para o kernel, mas a definição canônica vive em `dnu-abi`.

pub use dnu_abi::syscall::*;
