//! Parser do formato **DEF** (Dreamcore Executable Format) no kernel.
//!
//! Este módulo dá ao loader de processos a visão de *segmentos carregáveis* do
//! DEF: `entry_point`, lista de segmentos, tamanho de pilha e capacidades. O
//! DEF é o único formato de userspace aceito pelo kernel; o ELF existe apenas
//! como saída intermediária do toolchain, convertida por `dnu-pack`.
//!
//! A estrutura de bytes do cabeçalho está documentada (e compartilhada com a
//! ferramenta `dnu-pack`) em `imagineos_abi::def`. Aqui ficam apenas as leituras
//! defensivas — sem pânico, com aritmética checada — que o kernel precisa para
//! tratar um arquivo potencialmente malformado como um `Err` em vez de um
//! travamento.
//!
//! Referência de layout (little-endian; ver o módulo da ABI para o mapa completo):
//!
//! ```text
//!   0   4  magic "DNUF"
//!   4   1  arch
//!   5   1  binary_type
//!   6   2  format_version
//!   8   4  abi_version
//!   12  4  flags
//!   16  4  segment_count
//!   20  4  reserved
//!   24  8  entry_point
//!   32  8  stack_size
//!   40  8  image_base
//!   48  8  image_size
//!   56  32 capabilities
//!   88  64 signature
//!   152 .. tabela de segmentos (DEF_SEGMENT_SIZE cada)
//! ```

use imagineos_abi::def::{
    DEF_FORMAT_VERSION, DEF_HEADER_SIZE, DEF_MAGIC, DEF_MAX_SEGMENTS, DEF_SEGMENT_SIZE, DefArch,
    DefPermissions,
};

/// Erros de parsing de um DEF.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Truncated,
    InvalidMagic,
    UnsupportedFormat,
    UnsupportedArch,
    IncompatibleAbi,
    InvalidSegments,
    InvalidSegment,
    InvalidEntry,
}

/// Um segmento carregável de um DEF, já validado.
#[derive(Clone, Copy)]
pub struct Segment {
    pub virtual_address: u64,
    pub memory_size: u64,
    pub file_offset: usize,
    pub file_size: usize,
    /// Permissões de página (R/W/X). Ver [`DefPermissions`].
    pub permissions: u32,
    /// Este segmento é o bloco TLS (`.tdata` + `.tbss`).
    ///
    /// Não é carregado como página comum: `virtual_address` aqui é o endereço
    /// **dentro da imagem TLS** (offset 0 = início do bloco), e o loader monta
    /// um bloco próprio no topo da pilha, com o TCB logo depois.
    pub is_tls: bool,
}

impl Segment {
    pub fn is_executable(&self) -> bool {
        self.permissions & DefPermissions::EXECUTE != 0
    }

    pub fn is_writable(&self) -> bool {
        self.permissions & DefPermissions::WRITE != 0
    }
}

/// Um DEF validado, pronto para o loader.
pub struct Def<'a> {
    bytes: &'a [u8],
    /// Ponto de entrada virtual.
    pub entry: u64,
    /// Tamanho de pilha declarado pelo binário (0 = usar o padrão do kernel).
    pub stack_size: u64,
    /// Bitmap de capacidades concedidas (bits 0..64).
    pub capabilities: u64,
    segment_count: u32,
}

impl<'a> Def<'a> {
    /// Valida o cabeçalho e a tabela de segmentos.
    ///
    /// Não valida a assinatura criptográfica — isso é política do loader, que
    /// consulta [`Def::is_signed`] e decide o que fazer.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, Error> {
        if bytes.len() < DEF_HEADER_SIZE {
            return Err(Error::Truncated);
        }
        if bytes[..4] != DEF_MAGIC {
            return Err(Error::InvalidMagic);
        }
        let arch = DefArch::from_u8(bytes[4]).ok_or(Error::UnsupportedArch)?;
        if arch != DefArch::NATIVE {
            return Err(Error::UnsupportedArch);
        }
        if imagineos_abi::def::DefType::from_u8(bytes[5]).is_none() {
            return Err(Error::UnsupportedFormat);
        }
        if read_u16(bytes, 6)? != DEF_FORMAT_VERSION {
            return Err(Error::UnsupportedFormat);
        }
        let abi_version = read_u32(bytes, 8)? as u64;
        if abi_version > imagineos_abi::ABI_VERSION {
            return Err(Error::IncompatibleAbi);
        }
        let segment_count = read_u32(bytes, 16)?;
        if segment_count == 0 || segment_count as usize > DEF_MAX_SEGMENTS {
            return Err(Error::InvalidSegments);
        }
        let table_end = DEF_HEADER_SIZE
            .checked_add(segment_count as usize * DEF_SEGMENT_SIZE)
            .ok_or(Error::InvalidSegments)?;
        if table_end > bytes.len() {
            return Err(Error::InvalidSegments);
        }
        let entry = read_u64(bytes, 24)?;
        let stack_size = read_u64(bytes, 32)?;
        let capabilities = read_u64(bytes, 56)?;
        let def = Self {
            bytes,
            entry,
            stack_size,
            capabilities,
            segment_count,
        };
        // Valida cada segmento: aritmética checada, dentro do arquivo, sem
        // W^X, e o ponto de entrada precisa cair em um segmento executável.
        let mut loadable = false;
        let mut executable_entry = false;
        for index in 0..segment_count as usize {
            if let Some(segment) = def.segment(index)? {
                loadable = true;
                let end = segment
                    .virtual_address
                    .checked_add(segment.memory_size)
                    .ok_or(Error::InvalidSegment)?;
                if segment.is_executable() && entry >= segment.virtual_address && entry < end {
                    executable_entry = true;
                }
            }
        }
        if !loadable || !executable_entry {
            return Err(Error::InvalidEntry);
        }
        Ok(def)
    }

    /// Lê a entrada `index` da tabela de segmentos.
    ///
    /// Devolve `Ok(None)` quando a entrada existe mas não é carregável (por
    /// exemplo, `memory_size == 0`).
    pub fn segment(&self, index: usize) -> Result<Option<Segment>, Error> {
        if index >= self.segment_count as usize {
            return Ok(None);
        }
        let base = DEF_HEADER_SIZE
            .checked_add(index * DEF_SEGMENT_SIZE)
            .ok_or(Error::InvalidSegments)?;
        let file_offset = read_u64(self.bytes, base)?;
        let file_size = read_u64(self.bytes, base + 8)?;
        let memory_size = read_u64(self.bytes, base + 16)?;
        let virtual_address = read_u64(self.bytes, base + 24)?;
        let permissions = read_u32(self.bytes, base + 32)?;
        let reserved = read_u32(self.bytes, base + 36)?;
        if memory_size == 0 {
            return Ok(None);
        }
        if file_size > memory_size {
            return Err(Error::InvalidSegment);
        }
        let file_end = file_offset
            .checked_add(file_size)
            .ok_or(Error::InvalidSegment)?;
        let memory_end = virtual_address
            .checked_add(memory_size)
            .ok_or(Error::InvalidSegment)?;
        // W^X: um segmento executável não pode ser gravável.
        let writable = permissions & DefPermissions::WRITE != 0;
        let executable = permissions & DefPermissions::EXECUTE != 0;
        if writable && executable {
            return Err(Error::InvalidSegment);
        }
        if file_end > self.bytes.len() as u64
            || file_offset < DEF_HEADER_SIZE as u64
            || virtual_address < 0x400000
            || memory_end > 0x0000_8000_0000_0000
        {
            return Err(Error::InvalidSegment);
        }
        let file_offset = usize::try_from(file_offset).map_err(|_| Error::InvalidSegment)?;
        let file_size = usize::try_from(file_size).map_err(|_| Error::InvalidSegment)?;
        Ok(Some(Segment {
            virtual_address,
            memory_size,
            file_offset,
            file_size,
            permissions,
            is_tls: reserved & imagineos_abi::def::DEF_SEGMENT_TLS != 0,
        }))
    }

    /// Bytes do payload de um segmento, ou `None` se a faixa for inválida.
    pub fn file_bytes(&self, segment: Segment) -> Option<&'a [u8]> {
        let end = segment.file_offset.checked_add(segment.file_size)?;
        self.bytes.get(segment.file_offset..end)
    }

    pub fn segment_count(&self) -> usize {
        self.segment_count as usize
    }

    /// Fatia de bytes do arquivo DEF.
    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    pub fn is_signed(&self) -> bool {
        read_u32(self.bytes, 12)
            .map(|flags| flags & 1 != 0)
            .unwrap_or(false)
    }

    pub fn stack_guard(&self) -> bool {
        read_u32(self.bytes, 12)
            .map(|flags| flags & 4 != 0)
            .unwrap_or(false)
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, Error> {
    Ok(u16::from_le_bytes(
        bytes
            .get(offset..offset + 2)
            .ok_or(Error::Truncated)?
            .try_into()
            .map_err(|_| Error::Truncated)?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or(Error::Truncated)?
            .try_into()
            .map_err(|_| Error::Truncated)?,
    ))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, Error> {
    Ok(u64::from_le_bytes(
        bytes
            .get(offset..offset + 8)
            .ok_or(Error::Truncated)?
            .try_into()
            .map_err(|_| Error::Truncated)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::{Def, Error};
    use imagineos_abi::def::{
        DEF_FORMAT_VERSION, DEF_HEADER_SIZE, DEF_MAGIC, DEF_SEGMENT_SIZE, DefArch, DefPermissions,
        DefType,
    };

    /// Monta um DEF mínimo porém válido: um único segmento de texto contendo o
    /// ponto de entrada.
    fn valid_def() -> [u8; 184] {
        let mut bytes = [0u8; 184];
        bytes[..4].copy_from_slice(&DEF_MAGIC);
        bytes[4] = DefArch::X86_64 as u8;
        bytes[5] = DefType::App as u8;
        bytes[6..8].copy_from_slice(&DEF_FORMAT_VERSION.to_le_bytes());
        bytes[8..12].copy_from_slice(&(imagineos_abi::ABI_VERSION as u32).to_le_bytes());
        bytes[16..20].copy_from_slice(&1u32.to_le_bytes()); // segment_count
        bytes[24..32].copy_from_slice(&0x400000u64.to_le_bytes()); // entry_point
        bytes[32..40].copy_from_slice(&(32u64 * 4096).to_le_bytes()); // stack_size
        bytes[40..48].copy_from_slice(&0x400000u64.to_le_bytes()); // image_base
        bytes[48..56].copy_from_slice(&0x1000u64.to_le_bytes()); // image_size

        let base = DEF_HEADER_SIZE;
        bytes[base..base + 8].copy_from_slice(&152u64.to_le_bytes()); // file_offset
        bytes[base + 8..base + 16].copy_from_slice(&8u64.to_le_bytes()); // file_size
        bytes[base + 16..base + 24].copy_from_slice(&0x1000u64.to_le_bytes()); // memory_size
        bytes[base + 24..base + 32].copy_from_slice(&0x400000u64.to_le_bytes()); // vaddr
        bytes[base + 32..base + 36].copy_from_slice(&DefPermissions::TEXT.to_le_bytes()); // perms
        let payload = DEF_HEADER_SIZE + DEF_SEGMENT_SIZE;
        bytes[payload..payload + 8].copy_from_slice(b"testcode");
        bytes
    }

    #[test]
    fn parses_valid_def() {
        let bytes = valid_def();
        let def = Def::parse(&bytes).expect("valid def");
        assert_eq!(def.entry, 0x400000);
        assert_eq!(def.segment_count(), 1);
        let segment = def.segment(0).expect("segment").expect("loadable");
        assert_eq!(segment.virtual_address, 0x400000);
        assert!(segment.is_executable());
        assert!(!segment.is_writable());
        assert_eq!(def.file_bytes(segment).unwrap(), b"testcode");
    }

    #[test]
    fn rejects_bad_magic_and_truncation() {
        assert!(matches!(Def::parse(&[0u8; 8]), Err(Error::Truncated)));
        let mut bytes = valid_def();
        bytes[0] = b'X';
        assert!(matches!(Def::parse(&bytes), Err(Error::InvalidMagic)));
    }

    #[test]
    fn rejects_wxor_writable_executable_segment() {
        let mut bytes = valid_def();
        let base = DEF_HEADER_SIZE;
        let rwx = DefPermissions::READ | DefPermissions::WRITE | DefPermissions::EXECUTE;
        bytes[base + 32..base + 36].copy_from_slice(&rwx.to_le_bytes());
        assert!(matches!(Def::parse(&bytes), Err(Error::InvalidSegment)));
    }

    #[test]
    fn rejects_entry_outside_executable_segment() {
        let mut bytes = valid_def();
        bytes[24..32].copy_from_slice(&0xdead_beefu64.to_le_bytes());
        assert!(matches!(Def::parse(&bytes), Err(Error::InvalidEntry)));
    }

    #[test]
    fn rejects_newer_abi() {
        let mut bytes = valid_def();
        let abi = (imagineos_abi::ABI_VERSION as u32) + 1;
        bytes[8..12].copy_from_slice(&abi.to_le_bytes());
        assert!(matches!(Def::parse(&bytes), Err(Error::IncompatibleAbi)));
    }

    #[test]
    fn rejects_segment_payload_out_of_file() {
        let mut bytes = valid_def();
        let base = DEF_HEADER_SIZE;
        bytes[base..base + 8].copy_from_slice(&u64::MAX.to_le_bytes()); // file_offset
        assert!(matches!(Def::parse(&bytes), Err(Error::InvalidSegment)));
    }
}
