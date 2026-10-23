//! Relatório de subsistemas inicializados no boot.
//!
//! Cada etapa do bring-up do kernel marca o seu subsistema como pronto e o
//! resultado é impresso de uma vez, em bloco, no fim do boot. Concentrar o
//! relatório aqui (em vez de espalhar `console_write` pelo caminho de boot) tem
//! duas vantagens:
//!
//! * a mensagem final é uniforme — o operador vê de relance o que subiu;
//! * uma etapa que falha aparece como `[ -- ]` em vez de simplesmente não
//!   aparecer, o que evita a leitura otimista de "não vi erro, então está ok".
//!
//! O estado é um bitmap atômico: o boot é single-thread, mas as consultas
//! podem vir de qualquer contexto depois que os processos começam.

use core::sync::atomic::{AtomicU32, Ordering};

/// Subsistemas cujo estado o boot reporta, na ordem em que são inicializados.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Subsystem {
    /// GDT/TSS e IDT: o mínimo para tratar exceções e interrupções.
    DescriptorTables = 0,
    /// Alocador de frames físicos e heap do kernel.
    Memory = 1,
    /// Framebuffer, fonte PSF e cursor.
    Console = 2,
    /// PIT (IRQ0) e preempção por prioridade.
    Timer = 3,
    /// ACPI: RSDP, tabelas e caminho de desligamento.
    Acpi = 4,
    /// Disco ATA primário e leitura da GPT.
    Storage = 5,
    /// RAMFS montado a partir do `initramfs.img`.
    Ramfs = 6,
    /// Raiz persistente DFS (opcional: o sistema sobe sem ela).
    Dfs = 7,
    /// Configuração global (`global.conf`) e layout de teclado.
    Configuration = 8,
    /// O primeiro processo (PID 1, init) carregado e pronto para rodar.
    InitProcess = 9,
}

impl Subsystem {
    /// Nome exibido no relatório, alinhado em 12 colunas.
    pub const fn label(self) -> &'static str {
        match self {
            Self::DescriptorTables => "gdt/idt",
            Self::Memory => "memory",
            Self::Console => "console",
            Self::Timer => "timer",
            Self::Acpi => "acpi",
            Self::Storage => "storage",
            Self::Ramfs => "ramfs",
            Self::Dfs => "dfs",
            Self::Configuration => "config",
            Self::InitProcess => "init",
        }
    }

    /// Todo subsistema é obrigatório, exceto o DFS: sem partição persistente o
    /// sistema roda no initramfs.
    pub const fn is_required(self) -> bool {
        !matches!(self, Self::Dfs)
    }

    const fn bit(self) -> u32 {
        1 << (self as u8)
    }

    /// Todos os subsistemas, na ordem de inicialização.
    pub const ALL: [Self; 10] = [
        Self::DescriptorTables,
        Self::Memory,
        Self::Console,
        Self::Timer,
        Self::Acpi,
        Self::Storage,
        Self::Ramfs,
        Self::Dfs,
        Self::Configuration,
        Self::InitProcess,
    ];
}

static READY: AtomicU32 = AtomicU32::new(0);

/// Marca um subsistema como inicializado com sucesso.
pub fn mark_ready(subsystem: Subsystem) {
    READY.fetch_or(subsystem.bit(), Ordering::Relaxed);
}

/// Marca um subsistema como indisponível (falhou ou não existe neste boot).
pub fn mark_unavailable(subsystem: Subsystem) {
    READY.fetch_and(!subsystem.bit(), Ordering::Relaxed);
}

pub fn is_ready(subsystem: Subsystem) -> bool {
    READY.load(Ordering::Relaxed) & subsystem.bit() != 0
}

/// Verdadeiro quando todos os subsistemas obrigatórios subiram.
pub fn all_required_ready() -> bool {
    Subsystem::ALL
        .iter()
        .all(|subsystem| !subsystem.is_required() || is_ready(*subsystem))
}

/// Escreve uma linha por subsistema e um resumo final.
///
/// `ok`/`--` distinguem o que subiu do que não subiu; `(opcional)` marca o que
/// não impede o boot.
pub fn report() {
    crate::console_write("boot: subsystem status\n");
    for subsystem in Subsystem::ALL {
        crate::console_write("  [");
        crate::console_write(if is_ready(subsystem) { " ok " } else { " -- " });
        crate::console_write("] ");
        crate::console_write(subsystem.label());
        if !subsystem.is_required() {
            crate::console_write(" (optional)");
        }
        crate::console_write("\n");
    }
    crate::console_write(if all_required_ready() {
        "boot: all required subsystems ready\n"
    } else {
        "boot: some required subsystems are NOT ready\n"
    });
}
