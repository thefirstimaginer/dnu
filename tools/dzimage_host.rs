//! Ferramenta de host dzimage — empacota/extrai contêineres dzImage.
//!
//! Este é um wrapper simples ao redor do módulo `crate::dzimage` que fornece
//! uma interface de linha de comando para empacotar/extrair o contêiner dzImage.
//! É compilado como um binário autônomo (com std) e usado pelo Makefile do DNU
//! para gerar o arquivo .build/dzimage.

//! Compilação como crate standalone com std.
//! Re-exporta o módulo dzimage para uso como binário.
#[path = "../src/dzimage.rs"]
mod dzimage;

use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Uso: dzimage <pack|extract|info> [argumentos...]");
        return ExitCode::FAILURE;
    }

    match args[1].as_str() {
        "pack" => {
            // pack --kernel <kernel> [--initramfs <initramfs>] [--abi <n>] -o <output>
            let mut args_iter = args.iter().skip(2);
            let mut kernel: Option<String> = None;
            let mut initramfs: Option<String> = None;
            let mut output: Option<String> = None;
            let mut abi: u32 = 1;

            while let Some(arg) = args_iter.next() {
                match arg.as_str() {
                    "--kernel" => kernel = args_iter.next().map(|s| s.clone()),
                    "--initramfs" => initramfs = args_iter.next().map(|s| s.clone()),
                    "--abi" => {
                        if let Some(v) = args_iter.next() {
                            abi = v.parse().unwrap_or(1);
                        }
                    }
                    "-o" => output = args_iter.next().map(|s| s.clone()),
                    _ => {}
                }
            }

            let kernel = match kernel {
                Some(k) => k,
                None => {
                    eprintln!("Erro: --kernel é obrigatório");
                    return ExitCode::FAILURE;
                }
            };
            let output = match output {
                Some(o) => o,
                None => {
                    eprintln!("Erro: -o é obrigatório");
                    return ExitCode::FAILURE;
                }
            };

            // Lê o kernel
            let kernel_data = match std::fs::read(&kernel) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("Erro ao ler kernel {}: {}", kernel, e);
                    return ExitCode::FAILURE;
                }
            };

            // Lê o initramfs se fornecido
            let initramfs_data = if let Some(path) = &initramfs {
                match std::fs::read(path) {
                    Ok(d) => Some(d),
                    Err(e) => {
                        eprintln!("Erro ao ler initramfs {}: {}", path, e);
                        return ExitCode::FAILURE;
                    }
                }
            } else {
                None
            };

            // Constrói o dzImage
            let mut members: Vec<(u32, Vec<u8>)> = vec![(dzimage::KIND_KERNEL, kernel_data)];
            if let Some(ir) = initramfs_data {
                members.push((dzimage::KIND_INITRAMFS, ir));
            }

            // Calcula tamanhos
            let header_size = dzimage::HEADER_SIZE;
            let entry_size = 24;
            let table_end = header_size + members.len() * entry_size;

            // Calcula offsets
            let mut offsets = Vec::with_capacity(members.len());
            let mut cursor = table_end;
            for (_, data) in &members {
                offsets.push(cursor);
                cursor = (cursor + data.len() + 7) & !7; // alinha a 8
            }

            let total_size = cursor;
            let mut image = vec![0u8; total_size];

            // Cabeçalho
            image[..8].copy_from_slice(dzimage::MAGIC);
            image[8..12].copy_from_slice(&dzimage::VERSION.to_le_bytes());
            image[12..16].copy_from_slice(&abi.to_le_bytes());
            image[16..20].copy_from_slice(&(members.len() as u32).to_le_bytes());
            image[20..24].copy_from_slice(&0u32.to_le_bytes()); // reserved

            // Tabela de membros
            for (i, (kind, data)) in members.iter().enumerate() {
                let entry = header_size + i * entry_size;
                image[entry..entry + 4].copy_from_slice(&(*kind as u32).to_le_bytes());
                image[entry + 4..entry + 8].copy_from_slice(&0u32.to_le_bytes()); // reserved
                image[entry + 8..entry + 16].copy_from_slice(&(offsets[i] as u64).to_le_bytes());
                image[entry + 16..entry + 24].copy_from_slice(&(data.len() as u64).to_le_bytes());
            }

            // Payloads
            for ((_, data), offset) in members.iter().zip(offsets.iter()) {
                image[*offset..*offset + data.len()].copy_from_slice(data);
            }

            // CRC-32
            let crc = dzimage::crc32(&image);
            image[24..28].copy_from_slice(&crc.to_le_bytes());

            // Escreve o arquivo
            if let Err(e) = std::fs::write(&output, &image) {
                eprintln!("Erro ao escrever {}: {}", output, e);
                return ExitCode::FAILURE;
            }

            println!(
                "dzImage: {} ({} bytes, ABI {}, {} membros)",
                output,
                image.len(),
                abi,
                members.len()
            );
            ExitCode::SUCCESS
        }
        "info" => {
            // info <dzimage>
            let mut args = args.iter().skip(2);
            let archive = match args.next() {
                Some(a) => a,
                None => {
                    eprintln!("Erro: dzimage path é obrigatório");
                    return ExitCode::FAILURE;
                }
            };

            let image = match std::fs::read(&archive) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("Erro ao ler {}: {}", archive, e);
                    return ExitCode::FAILURE;
                }
            };

            // Verificar a imagem
            match dzimage::verify(&image, 1) {
                Ok(()) => {
                    // Extrair informações do cabeçalho
                    let abi = dzimage::header_abi(&image).unwrap_or(0);
                    let members_count = dzimage::get_u32(&image, 16).unwrap_or(0);

                    println!("dzImage {}: ok, ABI {}", archive, abi);

                    // Listar membros usando for_each_member
                    dzimage::for_each_member(&image, |member| {
                        let kind_name = match member.kind {
                            dzimage::KIND_KERNEL => "kernel",
                            dzimage::KIND_INITRAMFS => "initramfs",
                            _ => "unknown",
                        };
                        println!(
                            "  {:<9} offset {:<10} size {}",
                            kind_name, member.offset, member.payload(&image).len()
                        );
                    });
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("dzImage {}: FAILED - {}", archive, e);
                    ExitCode::FAILURE
                }
            }
        }
        "extract" => {
            // extract <dzimage> <kernel_out> [initramfs_out]
            let mut args = args.iter().skip(2);
            let archive = match args.next() {
                Some(a) => a,
                None => {
                    eprintln!("Erro: dzimage path é obrigatório");
                    return ExitCode::FAILURE;
                }
            };
            let kernel_out = args.next();
            let initramfs_out = args.next();

            let image = match std::fs::read(&archive) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("Erro ao ler {}: {}", archive, e);
                    return ExitCode::FAILURE;
                }
            };

            // Verificar a imagem primeiro
            if dzimage::verify(&image, 1).is_err() {
                eprintln!("dzImage {}: corrompido ou inválido", archive);
                return ExitCode::FAILURE;
            }

            // Extrair membros
            dzimage::for_each_member(&image, |member| {
                let kind_name = match member.kind {
                    dzimage::KIND_KERNEL => "kernel",
                    dzimage::KIND_INITRAMFS => "initramfs",
                    _ => "unknown",
                };
                let output = match kind_name {
                    "kernel" => kernel_out.as_deref(),
                    "initramfs" => initramfs_out.as_deref(),
                    _ => None,
                };

                if let Some(out_path) = output {
                    if let Err(e) = std::fs::write(out_path, member.payload(&image)) {
                        eprintln!("Erro ao escrever {}: {}", out_path, e);
                    } else {
                        println!("{} -> {}", kind_name, out_path);
                    }
                } else {
                    println!(
                        "{}: {} bytes, offset {}",
                        kind_name,
                        member.payload(&image).len(),
                        member.offset
                    );
                }
            });
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("Comando desconhecido: {}", args[1]);
            ExitCode::FAILURE
        }
    }
}
