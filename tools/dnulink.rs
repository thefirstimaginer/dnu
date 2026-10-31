//! **dnulink** — linker de alvo: liga objetos em ELF e emite **DEF** direto.
//!
//! O `rustc` não sabe emitir DEF: o formato que ele fala com o linker é ELF
//! (via `ld.lld`). O target `x86_64-unknown-dreamcore` aponta `linker` para
//! este programa, então toda a cadeia do `cargo build` termina aqui:
//!
//! ```text
//!   rustc (target Rust) -> .o ELF -> rust-lld -T dreamcore.ld -> ELF
//!                                     -> dnulink -> DEF
//! ```
//!
//! O `rust-lld` faz o trabalho difícil (resolver símbolos, fundir seções,
//! aplicar o script). O `dnulink` só traduz a **visão de execução** do ELF
//! para o contêiner que o kernel carrega — o mesmo trabalho do `dnu-pack`,
//! mas embutido no alvo para que `cargo build` produza DEF sem passo extra.
//!
//! Dois modos:
//!
//! * `dnulink --target-args ...` (invocado pelo `rustc`): repassa os
//!   argumentos ao `rust-lld`, grava um ELF temporário, converte e publica o
//!   DEF no caminho de saída (`-o`);
//! * `dnulink <entrada.elf> <saida>` (uso manual): converte um ELF existente.
//!
//! O DEF produzido é validado com a mesma [`validate_image`] do kernel antes
//! de ser gravado: um binário que o `dnulink` aceita, o loader aceita.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{exit, Command};

// A ABI é compilada dentro deste crate (ver `tools/abi.rs`): o `rustc` direto
// não consome o `.rlib` só-de-metadados do `build.rs` do crate dela.
#[path = "abi.rs"]
mod abi;
use crate::abi::imagineos_abi::{def, ABI_VERSION};
use def::{
    validate_image, DefArch, DefCapability, DefFlags, DefHeader, DefPermissions, DefSegment,
    DefType, DEF_FORMAT_VERSION, DEF_HEADER_SIZE, DEF_MAGIC, DEF_MAX_SEGMENTS, DEF_SEGMENT_SIZE,
    DEF_SEGMENT_TLS,
};

const DEFAULT_STACK_SIZE: u64 = 1024 * 1024;

// ELF64 — só o que é preciso ler.
const ELF_HEADER_SIZE: usize = 64;
const ELF_PROGRAM_HEADER_SIZE: usize = 56;
const PT_LOAD: u32 = 1;
const PT_TLS: u32 = 7;
const ET_EXEC: u16 = 2;
const EM_X86_64: u16 = 62;
const PF_X: u32 = 1 << 0;
const PF_W: u32 = 1 << 1;
const PF_R: u32 = 1 << 2;

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let slice = bytes
        .get(offset..offset + 2)
        .ok_or("leitura u16 fora dos limites")?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or("leitura u32 fora dos limites")?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, String> {
    let slice = bytes
        .get(offset..offset + 8)
        .ok_or("leitura u64 fora dos limites")?;
    Ok(u64::from_le_bytes([
        slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7],
    ]))
}

#[derive(Clone, Copy)]
struct LoadSegment {
    file_offset: u64,
    file_size: u64,
    memory_size: u64,
    virtual_address: u64,
    permissions: u32,
    /// `true` para o bloco `PT_TLS` (`.tdata` + `.tbss`).
    tls: bool,
}

fn elf_flags_to_def(flags: u32) -> u32 {
    let mut perms = 0;
    if flags & PF_R != 0 {
        perms |= DefPermissions::READ;
    }
    if flags & PF_W != 0 {
        perms |= DefPermissions::WRITE;
    }
    if flags & PF_X != 0 {
        perms |= DefPermissions::EXECUTE;
    }
    perms
}

/// Extrai os `PT_LOAD` do ELF já linkado pelo `rust-lld`.
fn parse_elf(bytes: &[u8]) -> Result<(u64, Vec<LoadSegment>), String> {
    if bytes.len() < ELF_HEADER_SIZE {
        return Err("ELF truncado".into());
    }
    if &bytes[0..4] != b"\x7fELF" {
        return Err("não é ELF".into());
    }
    if bytes[4] != 2 || bytes[5] != 1 {
        return Err("esperado ELF64 little-endian".into());
    }
    if read_u16(bytes, 16)? != ET_EXEC {
        return Err("e_type != ET_EXEC (o target é estático, não PIE)".into());
    }
    if read_u16(bytes, 18)? != EM_X86_64 {
        return Err("e_machine != x86_64".into());
    }
    let entry = read_u64(bytes, 24)?;
    let phoff = usize::try_from(read_u64(bytes, 32)?).map_err(|_| "phoff inválido")?;
    let phentsize = read_u16(bytes, 54)? as usize;
    let phnum = read_u16(bytes, 56)? as usize;
    if phentsize < ELF_PROGRAM_HEADER_SIZE || phnum == 0 || phnum > DEF_MAX_SEGMENTS {
        return Err(format!("program headers inválidos ({phentsize} x {phnum})"));
    }
    let table_end = phoff
        .checked_add(phentsize.checked_mul(phnum).ok_or("tabela grande demais")?)
        .ok_or("tabela estoura o arquivo")?;
    if table_end > bytes.len() {
        return Err("program headers fora do arquivo".into());
    }

    let mut segments = Vec::new();
    let mut tls_block: Option<LoadSegment> = None;
    for index in 0..phnum {
        let header = phoff + index * phentsize;
        let kind = read_u32(bytes, header)?;
        if kind == PT_TLS {
            // O bloco TLS precisa virar um segmento **próprio**: o kernel tem de
            // reconhecê-lo para montar o TCB. Ele não é carregado como página
            // comum (o `p_vaddr` é o endereço *na imagem TLS*, não um VA).
            let file_offset = read_u64(bytes, header + 8)?;
            let virtual_address = read_u64(bytes, header + 16)?;
            let file_size = read_u64(bytes, header + 32)?;
            let memory_size = read_u64(bytes, header + 40)?;
            if memory_size == 0 {
                continue;
            }
            if file_size > memory_size {
                return Err("PT_TLS: file_size > memory_size".into());
            }
            if file_offset.checked_add(file_size).is_none_or(|end| end > bytes.len() as u64) {
                return Err("PT_TLS aponta para fora do ELF".into());
            }
            tls_block = Some(LoadSegment {
                file_offset,
                file_size,
                memory_size,
                virtual_address,
                permissions: DefPermissions::READ | DefPermissions::WRITE,
                tls: true,
            });
            continue;
        }
        if kind != PT_LOAD {
            continue;
        }
        let flags = read_u32(bytes, header + 4)?;
        let file_offset = read_u64(bytes, header + 8)?;
        let virtual_address = read_u64(bytes, header + 16)?;
        let file_size = read_u64(bytes, header + 32)?;
        let memory_size = read_u64(bytes, header + 40)?;
        if memory_size == 0 {
            continue;
        }
        if file_size > memory_size {
            return Err(format!("segmento {index}: file_size > memory_size"));
        }
        let file_end = file_offset
            .checked_add(file_size)
            .ok_or("segmento estoura")?;
        if file_end > bytes.len() as u64 {
            return Err(format!("segmento {index} aponta para fora do ELF"));
        }
        let memory_end = virtual_address
            .checked_add(memory_size)
            .ok_or("segmento estoura o espaço de endereços")?;
        if virtual_address < 0x400000 || memory_end > 0x0000_8000_0000_0000 {
            return Err(format!(
                "segmento {index}: faixa inválida [{virtual_address:#x}, {memory_end:#x})"
            ));
        }
        let mut permissions = elf_flags_to_def(flags);
        // W^X: o loader recusa segmento executável e gravável. Um linker pode
        // juntar `.text` e `.data` se o script não separar; a escrita vence
        // sobre a execução (o código continua legível e executável não é
        // negociável).
        if permissions & DefPermissions::EXECUTE != 0 {
            permissions &= !DefPermissions::WRITE;
        }
        if permissions == 0 {
            continue;
        }
        segments.push(LoadSegment {
            file_offset,
            file_size,
            memory_size,
            virtual_address,
            permissions,
            tls: false,
        });
    }
    if let Some(tls) = tls_block {
        segments.push(tls);
    }
    if segments.is_empty() {
        return Err("o ELF não tem segmentos PT_LOAD".into());
    }
    let entry_ok = segments.iter().any(|s| {
        !s.tls
            && s.permissions & DefPermissions::EXECUTE != 0
            && entry >= s.virtual_address
            && entry < s.virtual_address + s.memory_size
    });
    if !entry_ok {
        return Err(format!("entry {entry:#x} fora de segmento executável"));
    }
    Ok((entry, segments))
}

fn pack_elf(elf: &[u8], stack_size: u64) -> Result<Vec<u8>, String> {
    let (entry, segments) = parse_elf(elf)?;
    let table_end = DEF_HEADER_SIZE + segments.len() * DEF_SEGMENT_SIZE;
    let payload_total: u64 = segments.iter().map(|s| s.file_size).sum();
    let mut image = vec![0u8; table_end + payload_total as usize];

    let mut header = DefHeader::EMPTY;
    header.arch = DefArch::NATIVE as u8;
    header.binary_type = DefType::App as u8;
    header.format_version = DEF_FORMAT_VERSION;
    header.abi_version = ABI_VERSION as u32;
    header.segment_count = segments.len() as u32;
    header.entry_point = entry;
    header.stack_size = stack_size;
    header.flags = DefFlags::STACK_GUARD;
    // Todo programa Rust do alvo recebe `fs` e `thread`: a `std`-less runtime
    // usa arquivos e futex, e o `main` pode criar threads. Capacidades não
    // são sandbox ainda no kernel; declarar o mínimo honesto é o contrato.
    let capabilities = DefCapability::FS | DefCapability::THREAD;
    header.flags |= DefFlags::HAS_CAPABILITIES;
    header.capabilities[..8].copy_from_slice(&capabilities.to_le_bytes());
    header.image_base = segments
        .iter()
        .map(|s| s.virtual_address)
        .min()
        .unwrap_or(0);
    header.image_size = segments
        .iter()
        .map(|s| (s.virtual_address + s.memory_size) - header.image_base)
        .max()
        .unwrap_or(0);

    write_header(&mut image, &header);

    let mut cursor = table_end as u64;
    for (index, seg) in segments.iter().enumerate() {
        let base = DEF_HEADER_SIZE + index * DEF_SEGMENT_SIZE;
        let entry = DefSegment {
            file_offset: cursor,
            file_size: seg.file_size,
            memory_size: seg.memory_size,
            virtual_address: seg.virtual_address,
            permissions: seg.permissions,
            reserved: if seg.tls { DEF_SEGMENT_TLS } else { 0 },
        };
        write_segment(&mut image, base, &entry);
        let src = seg.file_offset as usize;
        let dst = cursor as usize;
        image[dst..dst + seg.file_size as usize]
            .copy_from_slice(&elf[src..src + seg.file_size as usize]);
        cursor += seg.file_size;
    }

    validate_image(&image, ABI_VERSION, DefArch::NATIVE)
        .map_err(|err| format!("DEF gerado inválido: {err:?}"))?;
    Ok(image)
}

fn write_header(image: &mut [u8], header: &DefHeader) {
    image[0..4].copy_from_slice(&DEF_MAGIC);
    image[4] = header.arch;
    image[5] = header.binary_type;
    image[6..8].copy_from_slice(&header.format_version.to_le_bytes());
    image[8..12].copy_from_slice(&header.abi_version.to_le_bytes());
    image[12..16].copy_from_slice(&header.flags.to_le_bytes());
    image[16..20].copy_from_slice(&header.segment_count.to_le_bytes());
    image[20..24].copy_from_slice(&header.reserved.to_le_bytes());
    image[24..32].copy_from_slice(&header.entry_point.to_le_bytes());
    image[32..40].copy_from_slice(&header.stack_size.to_le_bytes());
    image[40..48].copy_from_slice(&header.image_base.to_le_bytes());
    image[48..56].copy_from_slice(&header.image_size.to_le_bytes());
    image[56..88].copy_from_slice(&header.capabilities);
    image[88..152].copy_from_slice(&header.signature);
}

fn write_segment(image: &mut [u8], base: usize, segment: &DefSegment) {
    image[base..base + 8].copy_from_slice(&segment.file_offset.to_le_bytes());
    image[base + 8..base + 16].copy_from_slice(&segment.file_size.to_le_bytes());
    image[base + 16..base + 24].copy_from_slice(&segment.memory_size.to_le_bytes());
    image[base + 24..base + 32].copy_from_slice(&segment.virtual_address.to_le_bytes());
    image[base + 32..base + 36].copy_from_slice(&segment.permissions.to_le_bytes());
    image[base + 36..base + 40].copy_from_slice(&segment.reserved.to_le_bytes());
}

/// Caminho do linker real (`rust-lld`), usado no modo invocado pelo `rustc`.
fn rust_lld() -> String {
    if let Ok(path) = env::var("DNULINK_LLD") {
        return path;
    }
    // O rustup coloca o `rust-lld` ao lado do `rustc` do toolchain ativo.
    if let Ok(rustc) = env::var("RUSTC") {
        let path = Path::new(&rustc).with_file_name("rust-lld");
        if path.exists() {
            return path.display().to_string();
        }
    }
    "rust-lld".to_string()
}

/// Divide a linha de comando: tudo antes do primeiro `-o` é entrada, depois é
/// o destino do DEF. Preserva a ordem para o linker real.
fn split_output(args: &[String]) -> (Vec<String>, Option<PathBuf>) {
    let mut passthrough = Vec::new();
    let mut output = None;
    let mut index = 0;
    while index < args.len() {
        if args[index] == "-o" {
            if let Some(path) = args.get(index + 1) {
                output = Some(PathBuf::from(path));
                index += 2;
                continue;
            }
        }
        passthrough.push(args[index].clone());
        index += 1;
    }
    (passthrough, output)
}

/// Script de linker embutido: o `dnulink` o escreve num arquivo temporário e
/// passa `-T` ao `rust-lld`.
///
/// Embutir (em vez de exigir `-C link-arg=-T...` no cargo config) faz o alvo
/// funcionar com um `cargo build` puro: qualquer projeto que use
/// `x86_64-unknown-dreamcore` recebe o layout correto sem configuração extra.
/// Caminho para a musl portada (por padrão, relativo ao próprio `dnulink`).
///
/// A `std` chama funções POSIX (`write`, `pthread_create`, `free`...) que o
/// shim `libc` declara e a musl implementa sobre `int 0x80`. Sem esta lib, o
/// link falha com símbolos indefinidos.
/// Stub de unwinding/errno (ver `std-dreamcore/unwind-stub.c`).
///
/// O alvo usa `panic = "abort"`, mas o compilador ainda referencia
/// `rust_eh_personality`/`_Unwind_*`; o stub satisfaz o link e encerra o
/// processo se algum caminho tentar desenrolar de verdade.
fn unwind_stub() -> Option<PathBuf> {
    if let Ok(path) = env::var("DNULINK_UNWIND") {
        return Some(PathBuf::from(path));
    }
    let exe = env::current_exe().ok()?;
    let root = exe.parent()?.parent()?;
    let candidate = root.join("std-dreamcore/unwind-stub.o");
    candidate.exists().then_some(candidate)
}

fn musl_library() -> Option<PathBuf> {
    if let Ok(path) = env::var("DNULINK_MUSL") {
        return Some(PathBuf::from(path));
    }
    // `dnulink` fica em `<raiz>/.build/dnulink`; a musl em `<raiz>/third_party`.
    let exe = env::current_exe().ok()?;
    let root = exe.parent()?.parent()?;
    let candidate = root.join("third_party/musl-1.2.6/lib/libc.a");
    candidate.exists().then_some(candidate)
}

const LINKER_SCRIPT: &str = r#"
ENTRY(_start)
PHDRS
{
    text   PT_LOAD FLAGS(5);
    rodata PT_LOAD FLAGS(4);
    data   PT_LOAD FLAGS(6);
    tls    PT_TLS  FLAGS(6);
}
SECTIONS
{
    . = 0x400000;
    .text : ALIGN(4096) { *(.text.entry) *(.text .text.*) *(.ltext .ltext.*) } :text
    .rodata : ALIGN(4096) { *(.rodata .rodata.*) *(.lrodata .lrodata.*)
                              *(.data.rel.ro .data.rel.ro.*) } :rodata
    /* `.tdata`/`.tbss` exigem um segmento PT_TLS: o lld recusa um simbolo
     * STT_TLS fora dele. O DEF nao carrega TLS; o bloco thread-local real vem
     * de `SYS_SET_TLS` (fs_base), e estas secoes so fazem o link fechar. */
    .tdata : ALIGN(4096) { *(.tdata .tdata.*) } :tls
    .tbss  : ALIGN(4096) { *(.tbss .tbss.*) } :tls
    .data : ALIGN(4096) { *(.data .data.*) *(.got .got.*) *(.got.plt .got.plt.*) } :data
    .bss : ALIGN(4096) { *(.bss .bss.*) *(COMMON) . = ALIGN(8); } :data
    /DISCARD/ : { *(.eh_frame .eh_frame_hdr .eh_frame.*) *(.comment)
                   *(.note .note.*) *(.debug_*) }
}
"#;

fn link_and_pack(args: &[String], output: &Path) -> Result<(), String> {
    let elf_temp = output.with_extension("elf.dnulink");
    let script_path = output.with_extension("ld.dnulink");
    fs::write(&script_path, LINKER_SCRIPT)
        .map_err(|err| format!("escrever script {}: {err}", script_path.display()))?;
    let mut command = Command::new(rust_lld());
    command.args(args);
    // O script precisa vir depois dos objetos para que o `. = 0x400000` seja a
    // base de tudo, e não só do que for encontrado depois.
    command.arg("-T").arg(&script_path);
    if let Some(stub) = unwind_stub() {
        command.arg(&stub);
    }
    if let Some(musl) = musl_library() {
        // Depois dos objetos do programa: a musl resolve os símbolos que a
        // `std` importa. `--whole-archive` não é usado — só o que é referenciado
        // entra, o que mantém o DEF pequeno.
        command.arg(&musl);
    }
    command.arg("-o").arg(&elf_temp);
    let status = command
        .status()
        .map_err(|err| format!("não consegui executar {}: {err}", rust_lld()))?;
    if !status.success() {
        return Err(format!("linker falhou com {status}"));
    }
    let elf = fs::read(&elf_temp).map_err(|err| format!("ler {}: {err}", elf_temp.display()))?;
    let image = pack_elf(&elf, DEFAULT_STACK_SIZE)?;
    fs::write(output, &image).map_err(|err| format!("gravar {}: {err}", output.display()))?;
    // `DNULINK_KEEP_ELF=1` preserva o ELF intermediário para inspeção
    // (`readelf -l`) — útil ao depurar TLS e seções.
    if env::var_os("DNULINK_KEEP_ELF").is_none() {
        let _ = fs::remove_file(&elf_temp);
    }
    Ok(())
}

fn usage() -> ! {
    eprintln!(
        "uso:\n  \
         dnulink --target <args do rust-lld...> -o <saida>\n  \
         dnulink <entrada.elf> <saida>\n\n\
         Variáveis:\n  \
         DNULINK_LLD   caminho do rust-lld (padrão: ao lado do RUSTC)"
    );
    exit(2)
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        usage();
    }
    if args[0] == "--help" || args[0] == "-h" {
        usage();
    }

    // Modo manual: exatamente dois caminhos posicionais.
    if args.len() == 2 && !args[0].starts_with('-') {
        let elf = match fs::read(&args[0]) {
            Ok(elf) => elf,
            Err(err) => {
                eprintln!("dnulink: ler '{}': {err}", args[0]);
                exit(1);
            }
        };
        let image = match pack_elf(&elf, DEFAULT_STACK_SIZE) {
            Ok(image) => image,
            Err(err) => {
                eprintln!("dnulink: {err}");
                exit(1);
            }
        };
        if let Err(err) = fs::write(&args[1], &image) {
            eprintln!("dnulink: gravar '{}': {err}", args[1]);
            exit(1);
        }
        eprintln!(
            "dnulink: '{}' -> DEF '{}' ({} bytes)",
            args[0],
            args[1],
            image.len()
        );
        return;
    }

    // Modo `rustc`.
    let (passthrough, output) = split_output(&args);
    let Some(output) = output else {
        eprintln!("dnulink: faltou -o <saida>");
        exit(2);
    };
    if let Err(err) = link_and_pack(&passthrough, &output) {
        eprintln!("dnulink: {err}");
        exit(1);
    }
}
