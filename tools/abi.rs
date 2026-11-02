//! Módulo de compatibilidade para o dnu-pack.
//!
//! Re-exporta o módulo `imagineos_abi` do `imagine-base/abi/` para uso pelas
//! ferramentas de host do DNU.

//! Re-exporta o crate imagineos_abi como um submódulo público.
//! O arquivo real está em imagine-base/abi/src/lib.rs.
#[path = "../../imagine-base/abi/src/lib.rs"]
pub mod imagineos_abi;

pub use imagineos_abi::*;

