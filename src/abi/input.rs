//! Stub de entrada de teclado/display.
//!
//! O estado atual do repositório não continha estes módulos, embora
//! `drivers/drv_input.rs` os referencie. Eles são placeholders mínimos para
//! permitir a compilação; a implementação real de interrupções de teclado e
//! display deve ser concluída separadamente.

/// Desabilita interrupções, devolvendo se estavam habilitadas antes.
pub fn disable_interrupts() -> bool {
    false
}

/// Restaura o estado de interrupções.
pub fn restore_interrupts(_was_enabled: bool) {}

/// Lê um caractere do teclado (stub: nenhum caractere disponível).
pub fn read_character() -> char {
    '\0'
}
