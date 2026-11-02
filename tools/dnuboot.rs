//! **dnuboot** — inicia o `dzImage` diretamente via QEMU, sem exigir ISO, mídia
//! de boot ou configuração manual de VM.
//!
//! O kernel DNU depende do protocolo Limine para bootar normalmente. Como o
//! `dnuboot` não usa um bootloader Limine, ele compila uma variante especial
//! do kernel (feature `direct-boot`) linkada em endereço baixo (identity-
//! mapped) com um entry point `_direct_start` que monta um `BootInfo` com
//! valores padrão e salta para `kernel_entry`.
//!
//! Fluxo:
//!   1. Valida o `dzImage` (magic, versão, ABI, membros, checksum);
//!   2. Compila o kernel em modo direto (`make kernel-direct`);
//!   3. Gera o binário de boot (`make boot.bin`);
//!   4. Lança `qemu-system-x86_64 -kernel .build/boot.bin -serial stdio` para
//!      exibir os logs do kernel diretamente no terminal.
//!
//! Dependências externas: `qemu-system-x86_64`, `make`, Rust nightly.

use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, ExitCode};

#[path = "../src/boot/dzimage.rs"]
mod dzimage;

use dzimage::{for_each_member, header_abi, verify};

const DEFAULT_DZIMAGE: &str = ".build/dzImage";

fn qemu_available() -> bool {
    Command::new("qemu-system-x86_64")
        .arg("--version")
        .status()
        .map_or(false, |s| s.success())
}

fn make_available() -> bool {
    Command::new("make").arg("--version").status().map_or(false, |s| s.success())
}

fn validate_dzimage(path: &str) -> bool {
    let image = match fs::read(path) {
        Ok(d) => d,
        Err(_) => return false,
    };
    if verify(&image, 1).is_err() {
        return false;
    }
    let abi = header_abi(&image).unwrap_or(0);
    let mut members = 0u32;
    for_each_member(&image, |_member| {
        members += 1;
    });
    println!("dnuboot: dzImage válido (ABI {}, {} membro(s))", abi, members);
    true
}

fn run_make(dnu_root: &str, target: &str) -> bool {
    let status = Command::new("make")
        .current_dir(dnu_root)
        .arg(target)
        .status();
    match status {
        Ok(s) => s.success(),
        Err(e) => {
            eprintln!("dnuboot: não foi executar `make {}`: {}", target, e);
            false
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let dzimage = args.get(1).map(String::as_str).unwrap_or(DEFAULT_DZIMAGE);

    let dnu_root = env::var("DNU_ROOT").unwrap_or_else(|_| ".".to_string());
    let build_dir = format!("{}/.build", dnu_root);

    println!("dnuboot: iniciando o dzImage em emulação direta (QEMU, sem ISO)");
    println!("dnuboot: dzImage = {}", dzimage);

    if !Path::new(dzimage).exists() {
        eprintln!("dnuboot: dzImage não encontrado em {}", dzimage);
        eprintln!("dnuboot: gere-o com `make dzImage` primeiro.");
        return ExitCode::FAILURE;
    }

    if !validate_dzimage(dzimage) {
        eprintln!("dnuboot: dzImage inválido; abortando o boot.");
        return ExitCode::FAILURE;
    }

    if !make_available() {
        eprintln!("dnuboot: `make` não encontrado; não é possível compilar o kernel direto.");
        return ExitCode::FAILURE;
    }

    // Compila o kernel em modo direto (sem Limine).
    println!("dnuboot: compilando kernel em modo direto...");
    if !run_make(&dnu_root, "kernel-direct") {
        eprintln!("dnuboot: falha ao compilar o kernel direto.");
        return ExitCode::FAILURE;
    }

    // Gera o binário de boot.
    println!("dnuboot: gerando binário de boot...");
    if !run_make(&dnu_root, "boot.bin") {
        eprintln!("dnuboot: falha ao gerar o binário de boot.");
        return ExitCode::FAILURE;
    }

    let boot_bin = format!("{}/boot.bin", build_dir);
    if !Path::new(&boot_bin).exists() {
        eprintln!("dnuboot: binário de boot não encontrado em {}", boot_bin);
        return ExitCode::FAILURE;
    }

    if !qemu_available() {
        eprintln!("dnuboot: qemu-system-x86_64 não encontrado; não é possível emular.");
        eprintln!("dnuboot: instale o qemu-system-x86_64 para bootar a imagem.");
        return ExitCode::FAILURE;
    }

    println!("dnuboot: lançando QEMU (logs do kernel via -serial stdio)...");
    println!("dnuboot: (pressione Ctrl+C ou use `C-a x` para sair do QEMU)");
    println!("------------------------------------------------------------");

    let status = Command::new("qemu-system-x86_64")
        .args([
            "-machine", "q35",
            "-m", "512M",
            "-serial", "stdio",
            "-vga", "std",
            "-kernel", &boot_bin,
        ])
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("------------------------------------------------------------");
            println!("dnuboot: QEMU encerrou com sucesso.");
            ExitCode::SUCCESS
        }
        Ok(s) => {
            eprintln!("dnuboot: QEMU encerrou com código {:?}", s.code());
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("dnuboot: não foi possível lançar o QEMU: {}", e);
            ExitCode::FAILURE
        }
    }
}
