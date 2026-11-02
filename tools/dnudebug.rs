//! **dnudebug** — ferramenta de depuração do kernel DNU.
//!
//! Valida um contêiner `dzImage` e exibe suas informações (magic, versão, ABI,
//! membros e checksum). Útil para conferir que o artefato publicado está íntegro
//! antes de bootar.

use std::env;
use std::fs;
use std::process::ExitCode;

#[path = "../src/boot/dzimage.rs"]
mod dzimage;

use dzimage::{for_each_member, get_u32, header_abi, kind_name, KIND_INITRAMFS, KIND_KERNEL, MAGIC, VERSION};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let archive = args.get(1).map(String::as_str).unwrap_or(".build/dzImage");

    let image = match fs::read(archive) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("dnudebug: não foi possível ler {}: {}", archive, e);
            return ExitCode::FAILURE;
        }
    };

    match dzimage::verify(&image, 1) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("dnudebug: {} inválido - {}", archive, e);
            return ExitCode::FAILURE;
        }
    }

    let abi = header_abi(&image).unwrap_or(0);
    let magic_ok = &image[..8] == MAGIC;
    let version = get_u32(&image, 8).unwrap_or(0);

    println!("dnudebug: {}", archive);
    println!("  magic:       {} (esperado DZIMAGE\\0)", if magic_ok { "ok" } else { "INVÁLIDO" });
    println!("  versão:      {} (esperado {})", version, VERSION);
    println!("  ABI:         {}", abi);
    println!("  tamanho:     {} bytes", image.len());
    println!("  membros:");

    let mut count = 0u32;
    for_each_member(&image, |member| {
        count += 1;
        let name = kind_name(member.kind);
        let payload = member.payload(&image);
        println!(
            "    {}  kind={} ({}) offset={} size={}",
            count, member.kind, name, member.offset, payload.len()
        );
    });

    if count == 0 {
        println!("    (nenhum membro)");
    }

    // Confere que não há membros estranhos (apenas kernel e/ou initramfs).
    let mut kinds_seen = [false; 3];
    for_each_member(&image, |member| {
        if member.kind == KIND_KERNEL {
            kinds_seen[0] = true;
        } else if member.kind == KIND_INITRAMFS {
            kinds_seen[1] = true;
        } else {
            kinds_seen[2] = true;
        }
    });
    if kinds_seen[2] {
        eprintln!("dnudebug: AVISO: contém membros de tipo desconhecido");
    }

    println!("dnudebug: imagem válida");
    ExitCode::SUCCESS
}
