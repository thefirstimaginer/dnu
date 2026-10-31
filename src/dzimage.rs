//! Leitor do **dzImage** (DNU Zipped Image) no lado do kernel.
//!
//! O `dzImage` é o único artefato publicado pelo repositório DNU: um contêiner
//! com uma tabela de membros seguida dos payloads. Layout little-endian:
//!
//! ```text
//! +0   magic   [8]  "DZIMAGE\0"
//! +8   version u32  = 2
//! +12  abi     u32  ABI_VERSION que o carregador de DEF espera
//! +16  entries u32  quantidade de membros
//! +20  rsvd    u32  = 0
//! +24  crc     u32  CRC-32 do arquivo inteiro com este campo zerado
//! +28  rsvd    u32  = 0
//! +32  table   entries x 24 bytes:
//!        kind      u32  1 = kernel (ELF), 2 = initramfs (USTAR)
//!        flags     u32  reservado, 0
//!        offset    u64  início do payload (alinhado a 8)
//!        size      u64  tamanho do payload
//! ```
//!
//! O membro `kernel` é este próprio ELF (o Limine o carrega como módulo) e o
//! `initramfs` é o segundo módulo de boot. O CRC-32 do arquivo cobre o kernel
//! em execução, então o cabeçalho (e a tabela) é validado no boot antes de o
//! kernel usar qualquer payload.
//!
//! A ferramenta de host que escreve o formato é `dzimage.rs` na raiz do
//! repositório; as duas pontas compartilham as constantes desta seção.


pub const MAGIC: &[u8; 8] = b"DZIMAGE\0";
pub const VERSION: u32 = 2;
pub const HEADER_SIZE: usize = 32;
const ENTRY_SIZE: usize = 24;

#[allow(dead_code)] // validado por `verify`, lido por `kernel_member` em testes.
pub const KIND_KERNEL: u32 = 1;
pub const KIND_INITRAMFS: u32 = 2;

/// Maior payload aceito (initramfs em USTAR não comprimido).
pub const MAX_MEMBER_SIZE: usize = 1024 * 1024 * 1024;
const MAX_MEMBERS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnpackError {
    /// Magic, versão ou tamanho de cabeçalho inválidos.
    InvalidHeader,
    /// A tabela de membros aponta para fora da imagem.
    InvalidSize,
    /// A imagem foi corrompida (CRC divergente).
    ChecksumMismatch,
    /// ABI do cabeçalho diferente da ABI compilada no kernel.
    AbiMismatch,
    /// Falta o membro esperado (kernel ou initramfs).
    MissingMember,
}

#[derive(Clone, Copy)]
pub struct Member {
    #[allow(dead_code)] // a tabela carrega o tipo; o kernel só consulta `offset`/`size`.
    pub kind: u32,
    pub offset: usize,
    pub size: usize,
}

fn read_u32(image: &[u8], offset: usize) -> Result<u32, UnpackError> {
    let bytes = image
        .get(offset..offset + 4)
        .ok_or(UnpackError::InvalidHeader)?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_u64(image: &[u8], offset: usize) -> Result<u64, UnpackError> {
    let bytes = image
        .get(offset..offset + 8)
        .ok_or(UnpackError::InvalidHeader)?;
    Ok(u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}

/// Valida o cabeçalho, a tabela e o CRC-32 de uma imagem em memória.
///
/// O kernel `crc32` é o mesmo algoritmo da ferramenta `dzimage` do host.
pub fn verify(image: &[u8], expected_abi: u64) -> Result<(), UnpackError> {
    if image.len() < HEADER_SIZE || &image[..8] != MAGIC {
        return Err(UnpackError::InvalidHeader);
    }
    if read_u32(image, 8)? != VERSION || read_u32(image, 20)? != 0 || read_u32(image, 28)? != 0 {
        return Err(UnpackError::InvalidHeader);
    }
    if u64::from(read_u32(image, 12)?) != expected_abi {
        return Err(UnpackError::AbiMismatch);
    }
    let entries = read_u32(image, 16)? as usize;
    if entries == 0
        || entries > MAX_MEMBERS
        || HEADER_SIZE
            .checked_add(entries * ENTRY_SIZE)
            .is_none_or(|end| end > image.len())
    {
        return Err(UnpackError::InvalidSize);
    }
    for index in 0..entries {
        let base = HEADER_SIZE + index * ENTRY_SIZE;
        let offset = usize::try_from(read_u64(image, base + 8)?).map_err(|_| UnpackError::InvalidSize)?;
        let size = usize::try_from(read_u64(image, base + 16)?).map_err(|_| UnpackError::InvalidSize)?;
        if size > MAX_MEMBER_SIZE || offset.checked_add(size).is_none_or(|end| end > image.len()) {
            return Err(UnpackError::InvalidSize);
        }
    }
    let expected = read_u32(image, 24)?;
    if checksum(image) != expected {
        return Err(UnpackError::ChecksumMismatch);
    }
    Ok(())
}

/// CRC-32 do arquivo com o campo de checksum (`+24`) zerado.
fn checksum(image: &[u8]) -> u32 {
    // O campo é zerado sem copiar a imagem: 24..28 é o único intervalo afetado.
    let mut state = !0u32;
    for (index, &byte) in image.iter().enumerate() {
        let byte = if (24..28).contains(&index) { 0 } else { byte };
        state ^= byte as u32;
        for _ in 0..8 {
            state = (state >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(state & 1)));
        }
    }
    !state
}

/// Devolve o membro `kernel` (este ELF) da imagem validada.
#[allow(dead_code)] // usado pelos testes e por um futuro carregador de ELF embutido.
pub fn kernel_member(image: &[u8]) -> Result<Member, UnpackError> {
    member(image, KIND_KERNEL).ok_or(UnpackError::MissingMember)
}

/// Devolve o membro `initramfs` (raiz volátil) da imagem validada.
pub fn initramfs_member(image: &[u8]) -> Result<Member, UnpackError> {
    member(image, KIND_INITRAMFS).ok_or(UnpackError::MissingMember)
}

fn member(image: &[u8], kind: u32) -> Option<Member> {
    let entries = read_u32(image, 16).ok()? as usize;
    for index in 0..entries {
        let base = HEADER_SIZE + index * ENTRY_SIZE;
        if read_u32(image, base).ok()? != kind {
            continue;
        }
        return Some(Member {
            kind,
            offset: usize::try_from(read_u64(image, base + 8).ok()?).ok()?,
            size: usize::try_from(read_u64(image, base + 16).ok()?).ok()?,
        });
    }
    None
}

impl Member {
    /// Bytes do payload deste membro dentro da imagem.
    pub fn payload<'a>(&self, image: &'a [u8]) -> &'a [u8] {
        &image[self.offset..self.offset + self.size]
    }
}

#[allow(dead_code)] // reexportado para os testes do formato.
/// ABI gravada no cabeçalho (`Some` apenas se a imagem tem cabeçalho).
///
/// As ferramentas extraem membros sem executá-los: a ABI esperada é a do
/// próprio cabeçalho, lida por aqui antes da validação.
pub fn header_abi(image: &[u8]) -> Option<u32> {
    read_u32(image, 12).ok()
}

/// Lê a tabela de membros de uma imagem já validada.
///
/// Usada pelas ferramentas do repositório (`dzimage` e `dfs-image`), que
/// extraem membros do contêiner publicado; o kernel usa só [`verify`] e
/// [`member`]. A validação continua sendo responsabilidade do chamador.
///
/// Devolve a ABI do cabeçalho e a quantidade de membros; cada membro vai para
/// `visit` na ordem da tabela.
pub fn for_each_member(image: &[u8], mut visit: impl FnMut(Member)) -> Option<(u32, usize)> {
    let abi = read_u32(image, 12).ok()?;
    let entries = read_u32(image, 16).ok()? as usize;
    for index in 0..entries {
        let base = HEADER_SIZE + index * ENTRY_SIZE;
        visit(Member {
            kind: read_u32(image, base).ok()?,
            offset: usize::try_from(read_u64(image, base + 8).ok()?).ok()?,
            size: usize::try_from(read_u64(image, base + 16).ok()?).ok()?,
        });
    }
    Some((abi, entries))
}

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Monta uma imagem de teste e calcula o CRC-32 do campo `+24`.
    fn image(members: &[(u32, &[u8])]) -> [u8; 128] {
        let mut bytes = [0u8; 128];
        bytes[..8].copy_from_slice(MAGIC);
        bytes[8..12].copy_from_slice(&VERSION.to_le_bytes());
        bytes[12..16].copy_from_slice(&3u32.to_le_bytes());
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
        reseal(&mut bytes);
        bytes
    }

    /// Recalcula o CRC-32 depois de editar a tabela.
    fn reseal(bytes: &mut [u8; 128]) {
        bytes[24..28].fill(0);
        let checksum = crc32(bytes);
        bytes[24..28].copy_from_slice(&checksum.to_le_bytes());
    }

    #[test]
    fn accepts_valid_image_and_exposes_members() {
        let bytes = image(&[(KIND_KERNEL, b"kernel"), (KIND_INITRAMFS, b"initramfs")]);
        assert_eq!(verify(&bytes, 3), Ok(()));
        let initramfs = initramfs_member(&bytes).unwrap();
        assert_eq!(initramfs.payload(&bytes), b"initramfs");
        assert_eq!(kernel_member(&bytes).unwrap().payload(&bytes), b"kernel");
    }

    #[test]
    fn rejects_bad_magic_version_abi_and_checksum() {
        let valid = image(&[(KIND_KERNEL, b"kernel")]);
        let mut bad = valid;
        bad[0] = b'X';
        assert_eq!(verify(&bad, 3), Err(UnpackError::InvalidHeader));

        let mut bad = valid;
        bad[8] = 9;
        assert_eq!(verify(&bad, 3), Err(UnpackError::InvalidHeader));

        assert_eq!(verify(&valid, 4), Err(UnpackError::AbiMismatch));

        let mut bad = valid;
        bad[HEADER_SIZE + 8] ^= 1;
        assert_eq!(verify(&bad, 3), Err(UnpackError::ChecksumMismatch));
    }

    #[test]
    fn rejects_member_outside_the_image() {
        let mut bad = image(&[(KIND_KERNEL, b"kernel")]);
        let size = 4096u64.to_le_bytes();
        bad[HEADER_SIZE + 16..HEADER_SIZE + 24].copy_from_slice(&size);
        // O CRC nem precisa bater: o tamanho é recusado antes.
        assert_eq!(verify(&bad, 3), Err(UnpackError::InvalidSize));
    }

    #[test]
    fn reports_missing_members() {
        let bytes = image(&[(KIND_KERNEL, b"kernel")]);
        assert_eq!(
            initramfs_member(&bytes).err(),
            Some(UnpackError::MissingMember)
        );
    }

    #[test]
    fn crc32_matches_ieee_vector() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }
}
