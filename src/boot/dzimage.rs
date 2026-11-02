//! Leitura e validação do contêiner **dzImage** (DNU Zipped Image).
//!
//! Este módulo é compilado tanto no kernel (`no_std`, via `alloc`) quanto na
//! ferramenta de host `dnu/dzimage.rs` (com `std`). Por isso só usa `alloc`/`core`.

#![allow(dead_code)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::convert::TryInto;

pub const MAGIC: &[u8; 8] = b"DZIMAGE\0";
pub const VERSION: u32 = 2;
pub const HEADER_SIZE: usize = 32;
const ENTRY_SIZE: usize = 24;
pub const KIND_KERNEL: u32 = 1;
pub const KIND_INITRAMFS: u32 = 2;

/// Maior payload aceito (initramfs em USTAR não comprimido).
pub const MAX_MEMBER_SIZE: usize = 1024 * 1024 * 1024;
const MAX_MEMBERS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnpackError {
    InvalidHeader,
    InvalidSize,
    ChecksumMismatch,
    AbiMismatch,
    MissingMember,
}

/// Um membro do dzImage, com offset/tamanho no contêiner e seus dados emprestados da imagem.
pub struct Member<'a> {
    pub kind: u32,
    pub offset: u64,
    pub size: u64,
    pub data: &'a [u8],
}

impl<'a> Member<'a> {
    /// Constrói um `Member` a partir de dados emprestados da imagem (kernel).
    pub fn from_data(kind: u32, offset: u64, size: u64, data: &'a [u8]) -> Self {
        Self {
            kind,
            offset,
            size,
            data,
        }
    }

    /// Extrai os bytes do payload a partir da imagem dzImage, usando offset/size.
    pub fn payload<'b>(&'b self, image: &'b [u8]) -> &'b [u8] {
        let start = self.offset as usize;
        let end = start + self.size as usize;
        &image[start..end]
    }
}

/// CRC-32 (IEEE 802.3).
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

fn put_u32(out: &mut [u8], value: u32) {
    out[..4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(out: &mut [u8], value: u64) {
    out[..8].copy_from_slice(&value.to_le_bytes());
}

pub fn get_u32(input: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        input.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

pub fn get_u64(input: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        input.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

pub fn kind_name(kind: u32) -> &'static str {
    match kind {
        KIND_KERNEL => "kernel",
        KIND_INITRAMFS => "initramfs",
        _ => "invalid",
    }
}

/// Lê o campo `abi_version` do cabeçalho do dzImage.
pub fn header_abi(image: &[u8]) -> Option<u32> {
    get_u32(image, 12)
}

pub fn align8(value: usize) -> usize {
    value.div_ceil(8) * 8
}

/// Valida o cabeçalho, a tabela e o CRC-32 de uma imagem em memória.
pub fn verify(image: &[u8], expected_abi: u64) -> Result<(), String> {
    if image.len() < HEADER_SIZE {
        return Err(String::from("truncated dzImage"));
    }
    if &image[..8] != MAGIC {
        return Err(String::from("invalid magic"));
    }
    let version = get_u32(image, 8).ok_or_else(|| String::from("truncated header"))?;
    if version != VERSION {
        let mut s = String::from("unsupported dzImage version ");
        s.push_str(&version.to_string());
        return Err(s);
    }
    let abi = get_u32(image, 12).ok_or_else(|| String::from("truncated header"))?;
    if u64::from(abi) != expected_abi {
        let mut s = String::from("unsupported dzImage ABI ");
        s.push_str(&abi.to_string());
        s.push_str(", expected ");
        s.push_str(&expected_abi.to_string());
        return Err(s);
    }
    let entries = get_u32(image, 16).ok_or_else(|| String::from("truncated header"))?;
    let table_end = HEADER_SIZE
        .checked_add(entries as usize * ENTRY_SIZE)
        .ok_or_else(|| String::from("overflow"))?;
    if image.len() < table_end {
        return Err(String::from("truncated member table"));
    }
    let expected = get_u32(image, 24).ok_or_else(|| String::from("truncated header"))?;
    let mut checksum = !0u32;
    for (i, &byte) in image.iter().enumerate() {
        if i >= 24 && i < 28 {
            let b = 0u8;
            checksum ^= b as u32;
            for _ in 0..8 {
                checksum = (checksum >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(checksum & 1)));
            }
        } else {
            checksum ^= byte as u32;
            for _ in 0..8 {
                checksum = (checksum >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(checksum & 1)));
            }
        }
    }
    if !checksum != expected {
        return Err(String::from("checksum mismatch"));
    }
    Ok(())
}

/// Extrai o membro `initramfs` do dzImage carregado pelo kernel.
/// Devolve um `Member` com referência emprestada à imagem (sem alocação).
#[cfg(target_os = "none")]
pub fn initramfs_member<'a>(image: &'a [u8]) -> Result<Member<'a>, String> {
    let abi = get_u32(image, 12).ok_or_else(|| String::from("truncated header"))?;
    let count = get_u32(image, 16).ok_or_else(|| String::from("truncated header"))? as usize;
    let table_end = HEADER_SIZE.checked_add(count * ENTRY_SIZE).ok_or_else(|| String::from("overflow"))?;
    if image.len() < table_end {
        return Err(String::from("truncated member table"));
    }
    for index in 0..count {
        let entry = HEADER_SIZE + index * ENTRY_SIZE;
        let kind = get_u32(image, entry).ok_or_else(|| String::from("truncated member"))?;
        let _flags = get_u32(image, entry + 4).ok_or_else(|| String::from("truncated member"))?;
        let offset = get_u64(image, entry + 8).ok_or_else(|| String::from("truncated member"))?;
        let size = get_u64(image, entry + 16).ok_or_else(|| String::from("truncated member"))?;
        if kind == KIND_INITRAMFS {
            let data = image
                .get(offset as usize..(offset + size) as usize)
                .ok_or_else(|| String::from("truncated member payload"))?;
            return Ok(Member::from_data(kind, offset, size, data));
        }
    }
    Err(String::from("initramfs member not found"))
}

/// Itera sobre todos os membros do dzImage, chamando `f` com cada um.
pub fn for_each_member<'a>(image: &'a [u8], mut f: impl FnMut(Member<'a>)) -> Option<(u32, usize)> {
    let abi = get_u32(image, 12)?;
    let entries = get_u32(image, 16)? as usize;
    let table_end = HEADER_SIZE.checked_add(entries * ENTRY_SIZE)?;
    if image.len() < table_end {
        return None;
    }
    for index in 0..entries {
        let entry = HEADER_SIZE + index * ENTRY_SIZE;
        let kind = get_u32(image, entry)?;
        let _flags = get_u32(image, entry + 4)?;
        let offset = get_u64(image, entry + 8)?;
        let size = get_u64(image, entry + 16)?;
        let data = image.get(offset as usize..(offset + size) as usize)?;
        f(Member {
            kind,
            offset,
            size,
            data,
        });
    }
    Some((abi, entries))
}
