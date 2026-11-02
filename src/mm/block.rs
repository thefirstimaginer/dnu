//! Dispositivos de bloco: trait genérica para leitura/escrita de setores.
//!
//! Esta é a abstração usada por ATA (`drivers/ata.rs`), GPT (`fs/gpt.rs`),
//! DFS (`fs/dfs.rs`) e pelo registro de drivers (`drivers/drv_block.rs`).
//! Cada driver concreto implementa esta trait e a publica no registro central.

/// Tamanho de um setor, em bytes.
pub const SECTOR_SIZE: usize = 512;

/// Erros de dispositivos de bloco.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockError {
    /// O dispositivo não está pronto (ex.: não respondeu ao IDENTIFY).
    NotReady,
    /// O endereço LBA está fora dos limites do disco.
    OutOfBounds,
    /// Erro de dispositivo (ex.: erro de CRC/setor defeituoso).
    DeviceError,
    /// Tempo esgotado esperando o dispositivo.
    Timeout,
}

/// Dispositivo de bloco genérico: leitura/escrita de setores de `SECTOR_SIZE`
/// bytes e.flush de caches.
pub trait BlockDevice {
    /// Número total de setores no dispositivo.
    fn sector_count(&self) -> u64;
    /// Lê um setor para `buffer` (deve ter exatamente `SECTOR_SIZE` bytes).
    fn read_sector(&self, lba: u64, buffer: &mut [u8; SECTOR_SIZE]) -> Result<(), BlockError>;
    /// Escreve um setor a partir de `buffer` (exatamente `SECTOR_SIZE` bytes).
    fn write_sector(&self, lba: u64, buffer: &[u8; SECTOR_SIZE]) -> Result<(), BlockError>;
    /// Flush de caches do dispositivo.
    fn flush(&self) -> Result<(), BlockError>;
}
