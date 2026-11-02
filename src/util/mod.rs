// Utilitários compartilhados do kernel DNU.
//
// Este módulo concentra funções auxiliares que eram duplicadas em vários
//! arquivos do kernel (crc32, leituras little-endian, etc.), evitando
//! duplicação de código e facilitando a manutenção.

use core::convert::TryInto;

// ---------------------------------------------------------------------------
// CRC-32 (IEEE 802.3, polinômio 0xEDB88320)
// ---------------------------------------------------------------------------

/// Computa o CRC-32 de uma fatia de bytes.
///
/// É o mesmo algoritmo usado pelo descompactador do kernel, pelo instalador
/// de disco, pela tabela GPT e pelo sistema de arquivos DFS.
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut checksum = !0u32;
    for &byte in bytes {
        checksum ^= byte as u32;
        for _ in 0..8 {
            checksum = (checksum >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(checksum & 1)));
        }
    }
    !checksum
}

/// Atualiza um CRC-32 incremental com mais bytes.
pub fn crc32_update(mut crc: u32, bytes: &[u8]) -> u32 {
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    crc
}

// ---------------------------------------------------------------------------
// Leitura little-endian de inteiros em slices de bytes
// ---------------------------------------------------------------------------

/// Lê um u16 little-endian a partir de `offset`, retornando `None` se houver
/// truncamento.
pub fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    bytes
        .get(offset..offset + 2)
        .and_then(|slice| slice.try_into().ok())
        .map(u16::from_le_bytes)
}

/// Lê um u32 little-endian a partir de `offset`, retornando `None` se houver
/// truncamento.
pub fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    bytes
        .get(offset..offset + 4)
        .and_then(|slice| slice.try_into().ok())
        .map(u32::from_le_bytes)
}

/// Lê um u64 little-endian a partir de `offset`, retornando `None` se houver
/// truncamento.
pub fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    bytes
        .get(offset..offset + 8)
        .and_then(|slice| slice.try_into().ok())
        .map(u64::from_le_bytes)
}
