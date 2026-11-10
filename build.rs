// build.rs - Compiles the assembly entry point and links it with the kernel

use std::process::Command;
use std::path::Path;

fn main() {
    // Tell cargo to re-run if the assembly file changes
    println!("cargo:rerun-if-changed=src/boot/entry.asm");
    println!("cargo:rerun-if-changed=src/linker.ld");

    // Compile the assembly entry point with nasm
    let asm_path = Path::new("src/boot/entry.asm");
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let obj_path = Path::new(&out_dir).join("entry.o");

    let status = Command::new("nasm")
        .args(["-f", "elf64", "-o", obj_path.to_str().unwrap(), asm_path.to_str().unwrap()])
        .status()
        .expect("Failed to execute nasm");

    if !status.success() {
        panic!("nasm failed to compile entry.asm");
    }

    // Tell cargo to link the object file
    println!("cargo:rustc-link-arg={}", obj_path.display());
    println!("cargo:rustc-link-search={}", out_dir);
}