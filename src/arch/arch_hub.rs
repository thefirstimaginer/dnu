//! Código dependente de arquitetura. Hoje só x86_64.
//!
//! O timer PIT é o único pedaço que vive neste módulo; o restante do x86_64
//! (GDT, IDT, paginação, FPU) é declarado direto em `main.rs`.
//!
//! A árvore de `src/` é plana: o timer (antigo `arch/x86_64/timer.rs`) é
//! `arch_timer.rs` e o hub o expõe no caminho `arch::x86_64::timer`.

#[path = "arch_timer.rs"]
pub mod timer;

pub mod x86_64 {
    pub use super::timer;
}
