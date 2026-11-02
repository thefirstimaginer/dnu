//! Implementação do algoritmo CRC-32 (IEEE 802.3, polinômio 0xEDB88320).
//! Usado tanto pelo kernel (`no_std`) quanto pelas ferramentas de host (`std`).

/// Computa o CRC-32 de uma fatia de bytes.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 == 1 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }
    crc ^ 0xFFFFFFFF
}

/// Atualiza um CRC-32 incremental com mais bytes.
pub fn crc32_update(mut crc: u32, data: &[u8]) -> u32 {
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 == 1 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }
    crc
}