//! **dnutest** — ferramenta de QA para validar o kernel DNU.
//!
//! Compila e executa os harnesses de teste em `tests/` (que rodam no host,
//! não no kernel bare-metal) e o teste do contêiner `dzimage`. Espelha o
//! alvo `make test` do DNU.

use std::env;
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let root = env::var("DNU_ROOT").unwrap_or_else(|_| ".".to_string());
    let tests_dir = format!("{}/tests", root);
    let build_dir = format!("{}/.build/tests", root);

    let _ = std::fs::create_dir_all(&build_dir);

    let host = std::env::var("HOST_TRIPLE").unwrap_or_default();
    let target = "x86_64-unknown-none";
    let abi_flags = if host != target {
        format!("--target {} --edition 2021", host)
    } else {
        String::new()
    };

    let mut failed = 0u32;
    let mut ran = 0u32;

    let dir_entries = match std::fs::read_dir(&tests_dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("dnutest: não foi possível ler {}: {}", tests_dir, e);
            return ExitCode::FAILURE;
        }
    };

    let mut harnesses: Vec<String> = dir_entries
        .filter_map(|e| {
            let e = e.ok()?;
            let path = e.path();
            if path.extension().map_or(false, |ext| ext == "rs") {
                path.file_stem().and_then(|s| s.to_str()).map(String::from)
            } else {
                None
            }
        })
        .collect();
    harnesses.sort();

    for harness in &harnesses {
        ran += 1;
        let out = format!("{}/{}-tests", build_dir, harness);
        let src = format!("{}/{}.rs", tests_dir, harness);
        println!("== tests/{}", harness);

        let status = Command::new("rustup")
            .args(["run", "nightly", "rustc", "--test"])
            .args(abi_flags.split_whitespace())
            .arg(&src)
            .arg("-o")
            .arg(&out)
            .status();
        match status {
            Ok(s) if s.success() => {}
            _ => {
                eprintln!("dnutest: falha ao compilar tests/{}", harness);
                failed += 1;
                continue;
            }
        }

        let status = Command::new(&out).arg("--test-threads=1").status();
        match status {
            Ok(s) if s.success() => {}
            Ok(s) => {
                eprintln!("dnutest: tests/{} falhou (código {:?})", harness, s.code());
                failed += 1;
            }
            Err(e) => {
                eprintln!("dnutest: não foi executar tests/{}: {}", harness, e);
                failed += 1;
            }
        }
    }

    // Teste do contêiner dzimage (host).
    let dzimage_src = format!("{}/tools/dzimage.rs", root);
    let dzimage_out = format!("{}/dzimage-host", build_dir);
    println!("== dzimage (host)");
    let status = Command::new("rustup")
        .args(["run", "nightly", "rustc", "--edition", "2021", "--test"])
        .arg(&dzimage_src)
        .arg("-o")
        .arg(&dzimage_out)
        .status();
    if status.map_or(false, |s| s.success()) {
        let status = Command::new(&dzimage_out).arg("--test-threads=1").status();
        match status {
            Ok(s) if s.success() => {}
            Ok(s) => {
                eprintln!("dnutest: dzimage (host) falhou (código {:?})", s.code());
                failed += 1;
            }
            Err(e) => {
                eprintln!("dnutest: não foi executar dzimage (host): {}", e);
                failed += 1;
            }
        }
    } else {
        eprintln!("dnutest: falha ao compilar dzimage (host)");
        failed += 1;
    }

    println!();
    println!("dnutest: {} harness executado(s), {} falha(s)", ran, failed);
    if failed > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
