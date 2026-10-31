// `drivers/keyboard.rs` resolve o layout por `crate::usr::config`. O módulo real
// é declarado com `#[path]` relativo a este arquivo e reexportado sob `usr`.
#[path = "../src/config.rs"]
pub mod config_impl;
pub mod usr {
    pub use super::config_impl as config;
}

#[path = "../src/keyboard.rs"]
mod keyboard;
