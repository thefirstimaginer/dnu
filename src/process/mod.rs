//! Gerenciamento de processos: escalonador, carregador de imagens DEF,
//! descritores, tabelas de cópia usuário/kernel e espaços de endereço.
//!
//! Estes módulos viveram em `src/exec/` e foram reunidos aqui em `src/process/`.

#[path = "scheduler.rs"]
pub mod scheduler;
#[path = "loader.rs"]
pub mod loader;
#[path = "def.rs"]
pub mod def;
#[path = "descriptors.rs"]
pub mod descriptors;
#[path = "space.rs"]
pub mod space;

// Reexporta a API pública do escalonador (antes em `crate::exec::process`) no
// nível `crate::process`, mantendo os caminhos curtos usados pelo resto do
// kernel (`crate::process::current_pid`, `crate::process::Process`, etc.).
pub use scheduler::*;
