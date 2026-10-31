//! Ferramenta de host que monta o **dzImage** (DNU Zipped Image).
//!
//! O `dzImage` é o único artefato que o repositório DNU publica; toda a
//! estrutura do ImagineOS (ISO, imagem de disco, diretório do instalador)
//! consome só ele.
//!
//! Layout (valores little-endian, payloads alinhados a 8 bytes):
//!
//! ```text
//! +0   magic   [8]  "DZIMAGE\0"
//! +8   version u32  = 2
//! +12  abi     u32  ABI_VERSION do crate imagineos_abi
//! +16  entries u32  quantidade de entradas da tabela
//! +20  rsvd    u32  = 0
//! +24  crc     u32  CRC-32 do arquivo inteiro com este campo zerado
//! +28  rsvd    u32  = 0
//! +32  table   entries x 24 bytes:
//!        kind      u32  1 = kernel (ELF), 2 = initramfs (USTAR)
//!        flags     u32  reservado, 0
//!        offset    u64  início do payload
//!        size      u64  tamanho do payload
//! ```
//!
//! O membro `kernel` deve vir primeiro: é ele que o Limine carrega e no qual
//! salta. O `initramfs` é o segundo módulo de boot (raiz volátil). O leitor do
//! lado do kernel vive em `boot/dzimage.rs`; as duas pontas usam as mesmas
//! constantes (magic, versão, header, kinds) e o mesmo CRC-32.
//!
//! Uso:
//!
//! ```text
//!   dzimage pack --kernel K.elf [--initramfs initramfs.tar] -o dzImage
//!   dzimage extract ARCHIVE [KERNEL-OUT] [INITRAMFS-OUT]
//!   dzimage info ARCHIVE
//! ```

use std::env;
use std::fs;
use std::process::ExitCode;

// O formato é lido pelo kernel com `src/dzimage.rs`; a ferramenta inclui o
// mesmo módulo para a validação (`verify`) e a tabela de membros
// (`for_each_member`) serem exatamente as do boot.
#[path = "src/dzimage.rs"]
#[allow(dead_code)]
mod dzimage;

pub const MAGIC: &[u8; 8] = b"DZIMAGE\0";
pub const VERSION: u32 = 2;
pub const HEADER_SIZE: usize = 32;
const ENTRY_SIZE: usize = 24;
pub const KIND_KERNEL: u32 = 1;
pub const KIND_INITRAMFS: u32 = 2;

pub fn kind_name(kind: u32) -> &'static str {
    match kind {
        KIND_KERNEL => "kernel",
        KIND_INITRAMFS => "initramfs",
        _ => "invalid",
    }
}

fn align8(value: usize) -> usize {
    value.div_ceil(8) * 8
}

/// CRC-32 (IEEE 802.3), o mesmo algoritmo do descompactador do kernel.
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
    Some(u32::from_le_bytes(input.get(offset..offset + 4)?.try_into().ok()?))
}

pub fn get_u64(input: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(input.get(offset..offset + 8)?.try_into().ok()?))
}

struct Member {
    kind: u32,
    data: Vec<u8>,
}

impl Member {
    fn file(kind: u32, path: &str) -> Result<Self, String> {
        let data = fs::read(path).map_err(|error| format!("cannot read {path}: {error}"))?;
        Ok(Self { kind, data })
    }
}

fn pack(arguments: &mut impl Iterator<Item = String>) -> Result<(), String> {
    let mut kernel: Option<Member> = None;
    let mut initramfs: Option<Member> = None;
    let mut output: Option<String> = None;
    let mut abi = 0u32;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--kernel" => {
                let path = arguments.next().ok_or("--kernel needs a path")?;
                let member = Member::file(KIND_KERNEL, &path)?;
                if member.data.get(..4) != Some(b"\x7fELF") {
                    return Err(format!("{path} is not a kernel ELF"));
                }
                kernel = Some(member);
            }
            "--initramfs" => {
                let path = arguments.next().ok_or("--initramfs needs a path")?;
                initramfs = Some(Member::file(KIND_INITRAMFS, &path)?);
            }
            "--abi" => {
                let value = arguments.next().ok_or("--abi needs a number")?;
                abi = value
                    .parse()
                    .map_err(|_| format!("invalid --abi value: {value}"))?;
            }
            "-o" => output = Some(arguments.next().ok_or("-o needs a path")?),
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    let kernel = kernel.ok_or("missing --kernel")?;
    let output = output.ok_or("missing -o")?;

    let mut members = vec![kernel];
    if let Some(initramfs) = initramfs {
        members.push(initramfs);
    }

    let table_end = HEADER_SIZE + members.len() * ENTRY_SIZE;
    let mut offsets = Vec::with_capacity(members.len());
    let mut cursor = align8(table_end);
    for member in &members {
        offsets.push(cursor);
        cursor = align8(cursor + member.data.len());
    }

    let mut image = vec![0u8; cursor];
    image[..8].copy_from_slice(MAGIC);
    put_u32(&mut image[8..], VERSION);
    put_u32(&mut image[12..], abi);
    put_u32(&mut image[16..], members.len() as u32);
    for (index, member) in members.iter().enumerate() {
        let entry = HEADER_SIZE + index * ENTRY_SIZE;
        put_u32(&mut image[entry..], member.kind);
        put_u32(&mut image[entry + 4..], 0);
        put_u64(&mut image[entry + 8..], offsets[index] as u64);
        put_u64(&mut image[entry + 16..], member.data.len() as u64);
    }
    for (member, offset) in members.iter().zip(offsets.iter()) {
        image[*offset..*offset + member.data.len()].copy_from_slice(&member.data);
    }
    let checksum = crc32(&image);
    put_u32(&mut image[24..], checksum);

    fs::write(&output, &image).map_err(|error| format!("cannot write {output}: {error}"))?;
    println!(
        "dzImage: {output} ({} bytes, ABI {abi}, {} membros)",
        image.len(),
        members.len()
    );
    Ok(())
}

/// Leitura da tabela delegada ao módulo do kernel (`src/dzimage.rs`): a
/// ferramenta e o boot leem o contêiner com o mesmo código, e `verify` confere
/// o CRC antes de qualquer extração.
struct Entries(Vec<dzimage::Member>);

fn read_entries(image: &[u8]) -> Result<(u32, Entries), String> {
    let abi = dzimage::header_abi(image).ok_or("truncated dzImage")? as u64;
    dzimage::verify(image, abi).map_err(|error| format!("bad dzImage: {error:?}"))?;
    let mut members = Vec::new();
    let (abi, _) = dzimage::for_each_member(image, |member| members.push(member))
        .ok_or("truncated dzImage")?;
    Ok((abi, Entries(members)))
}

impl Entries {
    fn find(&self, kind: u32) -> Option<&dzimage::Member> {
        self.0.iter().find(|member| member.kind == kind)
    }

    fn iter(&self) -> impl Iterator<Item = &dzimage::Member> {
        self.0.iter()
    }
}

fn extract(arguments: &mut impl Iterator<Item = String>) -> Result<(), String> {
    let archive = arguments.next().ok_or("extract needs a dzImage path")?;
    let kernel_output = arguments.next();
    let initramfs_output = arguments.next();
    let image = fs::read(&archive).map_err(|error| format!("cannot read {archive}: {error}"))?;
    let (_, entries) = read_entries(&image)?;
    for (kind, output) in [
        (KIND_KERNEL, kernel_output),
        (KIND_INITRAMFS, initramfs_output),
    ] {
        let Some(output) = output else { continue };
        let member = entries
            .find(kind)
            .ok_or_else(|| format!("dzImage has no {} member", kind_name(kind)))?;
        fs::write(&output, member.payload(&image))
            .map_err(|error| format!("cannot write {output}: {error}"))?;
        println!("{}: {output}", kind_name(kind));
    }
    Ok(())
}

fn info(arguments: &mut impl Iterator<Item = String>) -> Result<(), String> {
    let archive = arguments.next().ok_or("info needs a dzImage path")?;
    let image = fs::read(&archive).map_err(|error| format!("cannot read {archive}: {error}"))?;
    let (abi, entries) = read_entries(&image)?;
    let expected = get_u32(&image, 24).ok_or("truncated header")?;
    let mut copy = image.clone();
    put_u32(&mut copy[24..], 0);
    let status = if crc32(&copy) == expected {
        "ok"
    } else {
        "CORRUPT"
    };
    println!("dzImage {archive}: {status}, ABI {abi}");
    for member in entries.iter() {
        println!(
            "  {:<9} offset {:<10} size {}",
            kind_name(member.kind),
            member.offset,
            member.size
        );
    }
    Ok(())
}

fn run() -> Result<(), String> {
    let mut arguments = env::args().skip(1);
    match arguments.next().as_deref() {
        Some("pack") => pack(&mut arguments),
        Some("extract") => extract(&mut arguments),
        Some("info") => info(&mut arguments),
        _ => Err(concat!(
            "usage: dzimage pack --kernel K [--initramfs R] [--abi N] -o dzImage",
            " | dzimage extract ARCHIVE [KERNEL] [INITRAMFS] | dzimage info ARCHIVE"
        )
        .to_owned()),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("dzimage: {message}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        crc32, get_u32, pack, read_entries, HEADER_SIZE, KIND_INITRAMFS, KIND_KERNEL, MAGIC,
        VERSION,
    };
    use std::path::PathBuf;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("dzimage-{name}-{unique}"));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn file(&self, name: &str, contents: &[u8]) -> String {
            let path = self.0.join(name);
            std::fs::write(&path, contents).unwrap();
            path.display().to_string()
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Empacota e devolve o arquivo resultante.
    fn pack_files(
        scratch: &Scratch,
        kernel: &str,
        initramfs: Option<&str>,
    ) -> Result<Vec<u8>, String> {
        let mut arguments = vec![
            "--kernel".to_owned(),
            kernel.to_owned(),
            "--abi".to_owned(),
            "7".to_owned(),
        ];
        if let Some(initramfs) = initramfs {
            arguments.push("--initramfs".to_owned());
            arguments.push(initramfs.to_owned());
        }
        let output = scratch.0.join("dzImage");
        arguments.push("-o".to_owned());
        arguments.push(output.display().to_string());
        pack(&mut arguments.into_iter())?;
        Ok(std::fs::read(&output).unwrap())
    }

    const ELF: &[u8] = b"\x7fELF pretend kernel";
    const INITRAMFS: &[u8] = b"ustar pretend initramfs";

    #[test]
    fn packs_kernel_and_initramfs_with_valid_checksum() {
        let scratch = Scratch::new("pack");
        let kernel = scratch.file("kernel.elf", ELF);
        let initramfs = scratch.file("initramfs.tar", INITRAMFS);
        let image = pack_files(&scratch, &kernel, Some(&initramfs)).unwrap();

        assert_eq!(&image[..8], MAGIC);
        assert_eq!(get_u32(&image, 8), Some(VERSION));
        assert_eq!(get_u32(&image, 12), Some(7), "ABI is copied into the header");

        let stored = get_u32(&image, 24).unwrap();
        let mut zeroed = image.clone();
        zeroed[24..28].fill(0);
        assert_eq!(crc32(&zeroed), stored, "checksum covers the whole file");

        let (abi, entries) = read_entries(&image).unwrap();
        assert_eq!(abi, 7);
        let members: Vec<_> = entries.iter().collect();
        assert_eq!(members.len(), 2);
        assert_eq!(members[0].kind, KIND_KERNEL);
        assert_eq!(members[0].payload(&image), ELF);
        assert_eq!(members[1].kind, KIND_INITRAMFS);
        assert_eq!(members[1].payload(&image), INITRAMFS);
        assert!(HEADER_SIZE < members[0].offset, "payloads follow the table");
        assert_eq!(members[0].offset % 8, 0, "payloads are 8-byte aligned");
        assert_eq!(members[1].offset % 8, 0, "payloads are 8-byte aligned");
    }

    #[test]
    fn kernel_only_image_is_valid() {
        let scratch = Scratch::new("kernel-only");
        let kernel = scratch.file("kernel.elf", ELF);
        let image = pack_files(&scratch, &kernel, None).unwrap();
        let (_, entries) = read_entries(&image).unwrap();
        assert_eq!(entries.iter().count(), 1);
        assert_eq!(entries.iter().next().map(|member| member.kind), Some(KIND_KERNEL));
    }

    #[test]
    fn rejects_non_elf_kernel() {
        let scratch = Scratch::new("not-elf");
        let kernel = scratch.file("kernel.bin", b"not an elf");
        assert!(pack_files(&scratch, &kernel, None).is_err());
    }

    #[test]
    fn rejects_truncated_or_foreign_images() {
        let scratch = Scratch::new("truncated");
        let kernel = scratch.file("kernel.elf", ELF);
        let image = pack_files(&scratch, &kernel, None).unwrap();
        assert!(read_entries(&image[..HEADER_SIZE - 1]).is_err());
        assert!(read_entries(b"GARBAGE! and more bytes").is_err());
        let mut truncated = image;
        truncated.truncate(HEADER_SIZE + 8);
        assert!(read_entries(&truncated).is_err());
    }

    #[test]
    fn rejects_unknown_arguments() {
        let mut arguments = vec!["--bogus".to_owned()].into_iter();
        assert!(pack(&mut arguments).is_err());
    }
}
