//! Entry point da ferramenta de host `dzimage`.
//!
//! Compilado diretamente pelo Makefile do DNU com `std` para produzir o
//! binário `.build/dzimage`, usado para empacotar/extrair o contêiner dzImage.
//! Delega a leitura/validação do contêiner ao módulo em `src/dzimage.rs`
//! (compatível com `no_std`) e adiciona aqui as operações de I/O de arquivo
//! que exigem `std`.

use std::env;
use std::fs;
use std::process::ExitCode;

#[path = "src/dzimage.rs"]
mod dzimage;
#[path = "src/util/mod.rs"]
mod util;

use dzimage::{
    align8, crc32, for_each_member, get_u32, get_u64, header_abi, kind_name, KIND_INITRAMFS,
    KIND_KERNEL, MAGIC, VERSION,
};

fn put_u32(out: &mut [u8], value: u32) {
    out[..4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(out: &mut [u8], value: u64) {
    out[..8].copy_from_slice(&value.to_le_bytes());
}

fn pack(arguments: &mut impl Iterator<Item = String>) -> Result<(), String> {
    let mut kernel: Option<dzimage::Member> = None;
    let mut initramfs: Option<dzimage::Member> = None;
    let mut output: Option<String> = None;
    let mut abi = 0u32;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--kernel" => {
                let path = arguments.next().ok_or("--kernel needs a path")?;
                let data: &'static [u8] = Box::leak(
                    fs::read(&path).map_err(|e| {
                        let mut s = String::from("cannot read ");
                        s.push_str(&path);
                        s.push_str(": ");
                        s.push_str(&e.to_string());
                        s
                    })?.into_boxed_slice()
                );
                if data.get(..4) != Some(b"\x7fELF") {
                    return Err(format!("{path} is not a kernel ELF"));
                }
                kernel = Some(dzimage::Member::from_data(
                    KIND_KERNEL,
                    0,
                    data.len() as u64,
                    data,
                ));
            }
            "--initramfs" => {
                let path = arguments.next().ok_or("--initramfs needs a path")?;
                let data: &'static [u8] = Box::leak(
                    fs::read(&path).map_err(|e| {
                        let mut s = String::from("cannot read ");
                        s.push_str(&path);
                        s.push_str(": ");
                        s.push_str(&e.to_string());
                        s
                    })?.into_boxed_slice()
                );
                initramfs = Some(dzimage::Member::from_data(
                    KIND_INITRAMFS,
                    0,
                    data.len() as u64,
                    data,
                ));
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

    let table_end = dzimage::HEADER_SIZE + members.len() * 24;
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
        let entry = dzimage::HEADER_SIZE + index * 24;
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

fn read_entries(image: &[u8]) -> Result<(u32, Vec<dzimage::Member>), String> {
    let abi = header_abi(image).ok_or("truncated dzImage")? as u64;
    verify_image(image, abi)?;
    let mut members = Vec::new();
    for_each_member(image, |member| members.push(member));
    Ok((abi as u32, members))
}

fn verify_image(image: &[u8], abi: u64) -> Result<(), String> {
    dzimage::verify(image, abi)
}

fn extract(arguments: &mut impl Iterator<Item = String>) -> Result<(), String> {
    let archive = arguments.next().ok_or("extract needs a dzImage path")?;
    let kernel_output = arguments.next();
    let initramfs_output = arguments.next();
    let image = fs::read(&archive).map_err(|error| format!("cannot read {archive}: {error}"))?;
    let (_, members) = read_entries(&image)?;
    for member in &members {
        let kind_name = kind_name(member.kind);
        let output = match kind_name {
            "kernel" => kernel_output.as_deref(),
            "initramfs" => initramfs_output.as_deref(),
            _ => continue,
        };
        let Some(output) = output else { continue };
        fs::write(output, member.payload(&image))
            .map_err(|error| format!("cannot write {output}: {error}"))?;
        println!("{kind_name}: {output}");
    }
    Ok(())
}

fn info(arguments: &mut impl Iterator<Item = String>) -> Result<(), String> {
    let archive = arguments.next().ok_or("info needs a dzImage path")?;
    let image = fs::read(&archive).map_err(|error| format!("cannot read {archive}: {error}"))?;
    let (abi, members) = read_entries(&image)?;
    let expected = get_u32(&image, 24).ok_or("truncated header")?;
    let mut copy = image.clone();
    copy[24..28].fill(0);
    let status = if crc32(&copy) == expected {
        "ok"
    } else {
        "CORRUPT"
    };
    println!("dzImage {archive}: {status}, ABI {abi}");
    for member in &members {
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
        _ => Err(
            concat!(
                "usage: dzimage pack --kernel K [--initramfs R] [--abi N] -o dzImage",
                " | dzimage extract ARCHIVE [KERNEL] [INITRAMFS] | dzimage info ARCHIVE"
            )
            .to_owned(),
        ),
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
    use super::*;
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

        let (abi, members) = read_entries(&image).unwrap();
        assert_eq!(abi, 7);
        assert_eq!(members.len(), 2);
        assert_eq!(members[0].kind, KIND_KERNEL);
        assert_eq!(members[0].payload(&image), ELF);
        assert_eq!(members[1].kind, KIND_INITRAMFS);
        assert_eq!(members[1].payload(&image), INITRAMFS);
        assert!(dzimage::HEADER_SIZE < members[0].offset, "payloads follow the table");
        assert_eq!(members[0].offset % 8, 0, "payloads are 8-byte aligned");
        assert_eq!(members[1].offset % 8, 0, "payloads are 8-byte aligned");
    }

    #[test]
    fn kernel_only_image_is_valid() {
        let scratch = Scratch::new("kernel-only");
        let kernel = scratch.file("kernel.elf", ELF);
        let image = pack_files(&scratch, &kernel, None).unwrap();
        let (_, members) = read_entries(&image).unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].kind, KIND_KERNEL);
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
        assert!(read_entries(&image[..dzimage::HEADER_SIZE - 1]).is_err());
        assert!(read_entries(b"GARBAGE! and more bytes").is_err());
        let mut truncated = image;
        truncated.truncate(dzimage::HEADER_SIZE + 8);
        assert!(read_entries(&truncated).is_err());
    }

    #[test]
    fn rejects_unknown_arguments() {
        let mut arguments = vec!["--bogus".to_owned()].into_iter();
        assert!(pack(&mut arguments).is_err());
    }
}
