#![allow(dead_code)]
//! Testes do contêiner `dzImage` no lado do kernel.
//!
//! O formato é uma ponte entre dois repositórios: a ferramenta de host
//! (`dzimage.rs`, na raiz deste repositório) escreve e o kernel
//! (`boot/dzimage.rs`) lê. Os testes abaixo cobrem o leitor e o grafo de boot
//! que o usa; a ferramenta tem a própria suíte (`cargo test` da raiz).
//!
//! Execute com:
//! `rustc --edition 2024 --test tests/dzimage.rs -o /tmp/dzimage-tests && /tmp/dzimage-tests`

#[path = "../src/dzimage.rs"]
pub mod dzimage;

/// Tamanho da entrada da tabela no formato (não exposto pelo módulo do kernel).
const ENTRY_SIZE: usize = 24;

use dzimage::{initramfs_member, kernel_member, verify, UnpackError, HEADER_SIZE};

/// Monta uma imagem de teste de `128` bytes com os membros informados.
fn image(members: &[(u32, &[u8])]) -> [u8; 128] {
    let mut bytes = [0u8; 128];
    bytes[..8].copy_from_slice(dzimage::MAGIC);
    bytes[8..12].copy_from_slice(&dzimage::VERSION.to_le_bytes());
    bytes[12..16].copy_from_slice(&1u32.to_le_bytes()); // ABI_VERSION do kernel
    bytes[16..20].copy_from_slice(&(members.len() as u32).to_le_bytes());
    let mut cursor = HEADER_SIZE + members.len() * ENTRY_SIZE;
    for (index, (kind, payload)) in members.iter().enumerate() {
        let base = HEADER_SIZE + index * ENTRY_SIZE;
        bytes[base..base + 4].copy_from_slice(&kind.to_le_bytes());
        bytes[base + 8..base + 16].copy_from_slice(&(cursor as u64).to_le_bytes());
        bytes[base + 16..base + 24].copy_from_slice(&(payload.len() as u64).to_le_bytes());
        bytes[cursor..cursor + payload.len()].copy_from_slice(payload);
        cursor += payload.len();
    }
    seal(&mut bytes);
    bytes
}

/// Calcula o CRC-32 do campo `+24` sobre o arquivo com esse campo zerado.
fn seal(bytes: &mut [u8; 128]) {
    bytes[24..28].fill(0);
    let checksum = dzimage::crc32(bytes);
    bytes[24..28].copy_from_slice(&checksum.to_le_bytes());
}

const KERNEL: &[u8] = b"\x7fELF kernel payload";
const INITRAMFS: &[u8] = b"ustar initramfs payload";

#[test]
fn accepts_a_valid_image_and_exposes_its_members() {
    let bytes = image(&[
        (dzimage::KIND_KERNEL, KERNEL),
        (dzimage::KIND_INITRAMFS, INITRAMFS),
    ]);
    assert_eq!(verify(&bytes, 1), Ok(()));
    assert_eq!(kernel_member(&bytes).unwrap().payload(&bytes), KERNEL);
    assert_eq!(initramfs_member(&bytes).unwrap().payload(&bytes), INITRAMFS);
}

#[test]
fn rejects_bad_magic_short_files_and_wrong_versions() {
    assert_eq!(verify(b"GARBAGE!", 1), Err(UnpackError::InvalidHeader));

    let mut bad = image(&[(dzimage::KIND_KERNEL, KERNEL)]);
    bad[0] ^= 0xff;
    assert_eq!(verify(&bad, 1), Err(UnpackError::InvalidHeader));

    let mut bad = image(&[(dzimage::KIND_KERNEL, KERNEL)]);
    bad[8..12].copy_from_slice(&7u32.to_le_bytes());
    assert_eq!(verify(&bad, 1), Err(UnpackError::InvalidHeader));

    let valid = image(&[(dzimage::KIND_KERNEL, KERNEL)]);
    assert_eq!(
        verify(&valid[..HEADER_SIZE - 1], 1),
        Err(UnpackError::InvalidHeader)
    );
}

#[test]
fn rejects_an_image_built_for_another_abi() {
    let bytes = image(&[(dzimage::KIND_KERNEL, KERNEL)]);
    assert_eq!(verify(&bytes, 2), Err(UnpackError::AbiMismatch));
}

#[test]
fn rejects_a_corrupted_payload() {
    let mut bytes = image(&[(dzimage::KIND_KERNEL, KERNEL)]);
    // Corrompe o payload sem re-selar: o CRC cobre o kernel em execução.
    bytes[HEADER_SIZE + ENTRY_SIZE] ^= 1;
    assert_eq!(verify(&bytes, 1), Err(UnpackError::ChecksumMismatch));
}

#[test]
fn rejects_a_table_pointing_outside_the_image() {
    let mut bytes = image(&[(dzimage::KIND_KERNEL, KERNEL)]);
    bytes[HEADER_SIZE + 16..HEADER_SIZE + 24].copy_from_slice(&4096u64.to_le_bytes());
    assert_eq!(verify(&bytes, 1), Err(UnpackError::InvalidSize));

    let mut bytes = image(&[(dzimage::KIND_KERNEL, KERNEL)]);
    bytes[16..20].copy_from_slice(&64u32.to_le_bytes());
    assert_eq!(verify(&bytes, 1), Err(UnpackError::InvalidSize));
}

#[test]
fn reports_missing_members() {
    let bytes = image(&[(dzimage::KIND_KERNEL, KERNEL)]);
    assert_eq!(
        initramfs_member(&bytes).err(),
        Some(UnpackError::MissingMember)
    );
}

#[test]
fn crc32_matches_the_ieee_check_vector() {
    // Vetor de checagem padrão do CRC-32/ISO-HDLC.
    assert_eq!(dzimage::crc32(b"123456789"), 0xcbf4_3926);
}
