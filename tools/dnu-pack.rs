//! **dnu-pack** — empacotador ELF → DEF (Dreamcore Executable Format).
//!
//! O ImagineOS carrega binários no formato DEF (ver
//! `imagine-common/abi/src/def.rs`). O toolchain de userspace, porém, produz ELF
//! estático (`x86_64-unknown-none`) porque é o que o `rustc`/`ld` sabem emitir.
//! Este utilitário é a ponte: lê o ELF já linkado, extrai os segmentos `PT_LOAD`
//! e reescreve tudo em um contêiner DEF que o loader do kernel aceita, validando
//! o resultado com a mesma [`validate_image`] usada pelo kernel. Depois disso o
//! ELF original não é usado: o kernel lê apenas DEF.
//!
//! É um programa de **host** (roda no Linux, não dentro do ImagineOS), então não
//! depende de nada do kernel além do crate `imagineos_abi`, que é `no_std` e
//! compila em qualquer lugar. Isso garante que a definição do formato exista em
//! um único lugar: se o layout do DEF mudar, este arquivo acompanha sozinho.
//!
//! Uso:
//!
//! ```text
//!   dnu-pack <entrada.elf> <saida> [--type app|server|driver|runtime]
//!                                  [--caps fs,net,display,...]
//!                                  [--stack 1048576]
//!                                  [--stack-guard]
//! ```
//!
//! A saída **não** leva extensão: os programas são instalados como `/bin/ls`,
//! `/sbin/init` etc. Sem `--caps`, o binário é empacotado como um `App` sem
//! capacidades declaradas (o kernel decide o default). Sem `--stack`, usa a
//! pilha padrão [`DEFAULT_STACK_SIZE`].

// Como o kernel, este arquivo não usa a `std`; mas por ser host, ligamos `std`
// para ter acesso a `fs`/`args` sem `unsafe` de syscalls cruas. O `no_std` aqui
// seria teatro: a ferramenta roda no Linux.
use std::env;
use std::fs;
use std::process::exit;

// A ABI é compilada dentro deste crate (ver `tools/abi.rs`): o `rustc` direto
// não consome o `.rlib` só-de-metadados do `build.rs` do crate dela.
#[path = "abi.rs"]
mod abi;
use crate::abi::imagineos_abi::{def, ABI_VERSION};
use def::{
    DEF_FORMAT_VERSION, DEF_HEADER_SIZE, DEF_MAGIC, DEF_MAX_SEGMENTS, DEF_SEGMENT_SIZE, DefArch,
    DefCapability, DefFlags, DefHeader, DefPermissions, DefSegment, DefType, validate_image,
};

// ==============================================================================
// Constantes do ELF64 (apenas o que precisamos ler)
// ==============================================================================
const ELF_HEADER_SIZE: usize = 64;
const ELF_PROGRAM_HEADER_SIZE: usize = 56;
const PT_LOAD: u32 = 1;
const ET_EXEC: u16 = 2;
const EM_X86_64: u16 = 62;
const PF_X: u32 = 1 << 0;
const PF_W: u32 = 1 << 1;
const PF_R: u32 = 1 << 2;

/// Pilha padrão quando `--stack` não é passado: 1 MiB.
const DEFAULT_STACK_SIZE: u64 = 1024 * 1024;

// ==============================================================================
// Leitura sem pânico de campos little-endian
// ==============================================================================
fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let slice = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| format!("leitura u16 fora dos limites em 0x{offset:x}"))?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| format!("leitura u32 fora dos limites em 0x{offset:x}"))?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, String> {
    let slice = bytes
        .get(offset..offset + 8)
        .ok_or_else(|| format!("leitura u64 fora dos limites em 0x{offset:x}"))?;
    Ok(u64::from_le_bytes([
        slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7],
    ]))
}

// ==============================================================================
// Um segmento PT_LOAD extraído do ELF
// ==============================================================================
#[derive(Clone, Copy)]
struct LoadSegment {
    file_offset: u64,
    file_size: u64,
    memory_size: u64,
    virtual_address: u64,
    permissions: u32,
}

/// Converte as flags de página do ELF (`p_flags`) nas permissões DEF.
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

/// Faz o parsing do cabeçalho ELF64 e devolve a entrada e a lista de segmentos
/// `PT_LOAD`, já no formato do DEF. Aplica W^X: um segmento executável nunca
/// carrega a permissão de escrita, do contrário o loader do kernel recusa.
fn parse_elf(bytes: &[u8]) -> Result<(u64, Vec<LoadSegment>), String> {
    if bytes.len() < ELF_HEADER_SIZE {
        return Err("arquivo menor que o cabeçalho ELF64".into());
    }
    if &bytes[0..4] != b"\x7fELF" {
        return Err("não é um arquivo ELF (magic inválido)".into());
    }
    if bytes[4] != 2 {
        return Err("ELF não é 64-bit (EI_CLASS != ELFCLASS64)".into());
    }
    if bytes[5] != 1 {
        return Err("ELF não é little-endian (EI_DATA != ELFDATA2LSB)".into());
    }
    if read_u16(bytes, 16)? != ET_EXEC {
        return Err("ELF não é executável estático (e_type != ET_EXEC)".into());
    }
    if read_u16(bytes, 18)? != EM_X86_64 {
        return Err("ELF não é x86_64 (e_machine != EM_X86_64)".into());
    }
    let entry = read_u64(bytes, 24)?;
    let phoff = usize::try_from(read_u64(bytes, 32)?)
        .map_err(|_| "offset da tabela de program headers não cabe em usize".to_string())?;
    let phentsize = read_u16(bytes, 54)? as usize;
    let phnum = read_u16(bytes, 56)? as usize;
    if phentsize < ELF_PROGRAM_HEADER_SIZE || phnum == 0 || phnum > DEF_MAX_SEGMENTS {
        return Err(format!(
            "tabela de program headers inválida (entsize={phentsize}, num={phnum})"
        ));
    }
    let table_size = phentsize
        .checked_mul(phnum)
        .ok_or("tabela de program headers grande demais")?;
    let table_end = phoff
        .checked_add(table_size)
        .ok_or("tabela estoura o arquivo")?;
    if table_end > bytes.len() {
        return Err("tabela de program headers fora do arquivo".into());
    }

    let mut segments = Vec::new();
    for index in 0..phnum {
        let header = phoff + index * phentsize;
        if read_u32(bytes, header)? != PT_LOAD {
            continue;
        }
        let flags = read_u32(bytes, header + 4)?;
        let file_offset = read_u64(bytes, header + 8)?;
        let virtual_address = read_u64(bytes, header + 16)?;
        let file_size = read_u64(bytes, header + 32)?;
        let memory_size = read_u64(bytes, header + 40)?;
        if file_size == 0 && memory_size == 0 {
            continue;
        }
        if file_size > memory_size {
            return Err(format!(
                "segmento {index}: file_size ({file_size}) > memory_size ({memory_size})"
            ));
        }
        let file_end = file_offset
            .checked_add(file_size)
            .ok_or("segmento estoura o arquivo")?;
        if file_end > bytes.len() as u64 {
            return Err(format!("segmento {index} aponta para fora do arquivo ELF"));
        }
        // O loader do kernel exige endereços virtuais >= 0x400000 e que todo
        // mapeamento fique abaixo de 0x8000_0000_0000. Rejeitamos aqui para
        // produzir uma mensagem clara em vez de um erro obscuro no boot.
        let memory_end = virtual_address
            .checked_add(memory_size)
            .ok_or("segmento estoura o espaço de endereços")?;
        if virtual_address < 0x400000 || memory_end > 0x0000_8000_0000_0000 {
            return Err(format!(
                "segmento {index}: faixa virtual inválida [{virtual_address:#x}, {memory_end:#x})"
            ));
        }

        let mut permissions = elf_flags_to_def(flags);
        // W^X: se o ELF marcou o segmento como executável *e* gravável (algo
        // comum no stack/rodata de linkers preguiçosos), preservamos apenas o
        // executável — dados nunca são executáveis no ImagineOS.
        if permissions & DefPermissions::EXECUTE != 0 {
            permissions &= !DefPermissions::WRITE;
        }
        if permissions == 0 {
            // Segmento mapeado sem nenhuma permissão não é carregável: o kernel
            // o ignora, então nem o empacotamos.
            continue;
        }

        segments.push(LoadSegment {
            file_offset,
            file_size,
            memory_size,
            virtual_address,
            permissions,
        });
    }

    if segments.is_empty() {
        return Err("o ELF não tem nenhum segmento PT_LOAD carregável".into());
    }

    // O entry_point precisa cair em um segmento executável, do contrário o
    // loader recusaria o DEF. Verificamos aqui também.
    let entry_ok = segments.iter().any(|s| {
        s.permissions & DefPermissions::EXECUTE != 0
            && entry >= s.virtual_address
            && entry < s.virtual_address + s.memory_size
    });
    if !entry_ok {
        return Err(format!(
            "entry_point {entry:#x} não cai em nenhum segmento executável"
        ));
    }

    Ok((entry, segments))
}

// ==============================================================================
// Montagem do contêiner DEF
// ==============================================================================
/// Empacota um ELF em um contêiner DEF completo. `binary_type` e `capabilities`
/// são política do empacotador; `stack_size`/`stack_guard` vêm da linha de
/// comando. Devolve os bytes do arquivo DEF prontos para gravar.
fn pack_elf(
    elf: &[u8],
    binary_type: DefType,
    capabilities: u64,
    stack_size: u64,
    stack_guard: bool,
) -> Result<Vec<u8>, String> {
    let (entry, segments) = parse_elf(elf)?;
    if segments.len() > DEF_MAX_SEGMENTS {
        return Err(format!(
            "{} segmentos carregáveis excedem o máximo de {DEF_MAX_SEGMENTS}",
            segments.len()
        ));
    }

    // Layout: cabeçalho fixo + tabela de segmentos + payloads concatenados.
    // Cada payload é copiado byte a byte a partir do ELF, na mesma ordem.
    let table_end = DEF_HEADER_SIZE + segments.len() * DEF_SEGMENT_SIZE;
    let payload_total: u64 = segments.iter().map(|s| s.file_size).sum();
    let total = table_end as u64 + payload_total;
    let mut image = vec![0u8; total as usize];

    // Escreve o cabeçalho DEF. Usamos DefHeader::EMPTY como base e só
    // sobrescrevemos os campos relevantes, para que magic/versão de formato/ABI
    // venham da definição canônica.
    let mut header = DefHeader::EMPTY;
    header.arch = DefArch::NATIVE as u8;
    header.binary_type = binary_type as u8;
    header.format_version = DEF_FORMAT_VERSION;
    header.abi_version = ABI_VERSION as u32;
    header.segment_count = segments.len() as u32;
    header.entry_point = entry;
    header.stack_size = stack_size;
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
    if capabilities != 0 {
        header.flags |= DefFlags::HAS_CAPABILITIES;
        header.capabilities[..8].copy_from_slice(&capabilities.to_le_bytes());
    }
    if stack_guard {
        header.flags |= DefFlags::STACK_GUARD;
    }
    write_header(&mut image, &header);

    // Escreve a tabela de segmentos e copia os payloads.
    let mut cursor = table_end as u64;
    for (index, seg) in segments.iter().enumerate() {
        let base = DEF_HEADER_SIZE + index * DEF_SEGMENT_SIZE;
        let def_segment = DefSegment {
            file_offset: cursor,
            file_size: seg.file_size,
            memory_size: seg.memory_size,
            virtual_address: seg.virtual_address,
            permissions: seg.permissions,
            reserved: 0,
        };
        write_segment(&mut image, base, &def_segment);

        let src_start = seg.file_offset as usize;
        let src_end = src_start + seg.file_size as usize;
        let dst_start = cursor as usize;
        let dst_end = dst_start + seg.file_size as usize;
        image[dst_start..dst_end].copy_from_slice(&elf[src_start..src_end]);
        cursor += seg.file_size;
    }

    // Valida com a mesma função do kernel: se isto passa aqui, o loader aceita.
    validate_image(&image, ABI_VERSION, DefArch::NATIVE)
        .map_err(|err| format!("DEF gerado inválido: {err:?}"))?;
    Ok(image)
}

/// Serializa o cabeçalho DEF nos primeiros [`DEF_HEADER_SIZE`] bytes da imagem.
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

/// Serializa uma entrada da tabela de segmentos DEF em `base`.
fn write_segment(image: &mut [u8], base: usize, segment: &DefSegment) {
    image[base..base + 8].copy_from_slice(&segment.file_offset.to_le_bytes());
    image[base + 8..base + 16].copy_from_slice(&segment.file_size.to_le_bytes());
    image[base + 16..base + 24].copy_from_slice(&segment.memory_size.to_le_bytes());
    image[base + 24..base + 32].copy_from_slice(&segment.virtual_address.to_le_bytes());
    image[base + 32..base + 36].copy_from_slice(&segment.permissions.to_le_bytes());
    image[base + 36..base + 40].copy_from_slice(&segment.reserved.to_le_bytes());
}

// ==============================================================================
// Linha de comando
// ==============================================================================
struct Options {
    input: String,
    output: String,
    binary_type: DefType,
    capabilities: u64,
    stack_size: u64,
    stack_guard: bool,
}

fn parse_type(name: &str) -> Result<DefType, String> {
    match name {
        "app" => Ok(DefType::App),
        "server" => Ok(DefType::HybridServer),
        "driver" => Ok(DefType::UserDriver),
        "runtime" => Ok(DefType::Runtime),
        other => Err(format!(
            "tipo desconhecido '{other}' (use app|server|driver|runtime)"
        )),
    }
}

/// Traduz um nome de capacidade em seu bit. Aceita os mesmos nomes do crate
/// `imagineos_abi`, para que o empacotador e o kernel falem a mesma língua.
fn parse_capability(name: &str) -> Result<u64, String> {
    match name {
        "fs" => Ok(DefCapability::FS),
        "net" => Ok(DefCapability::NET),
        "device" => Ok(DefCapability::DEVICE),
        "spawn" => Ok(DefCapability::SPAWN),
        "thread" => Ok(DefCapability::THREAD),
        "display" => Ok(DefCapability::DISPLAY),
        "supervise" => Ok(DefCapability::SUPERVISE),
        "install" => Ok(DefCapability::INSTALL),
        other => Err(format!("capacidade desconhecida '{other}'")),
    }
}

fn parse_args() -> Result<Options, String> {
    let args: Vec<String> = env::args().collect();
    let mut input = None;
    let mut output = None;
    let mut binary_type = DefType::App;
    let mut capabilities = 0u64;
    let mut stack_size = DEFAULT_STACK_SIZE;
    let mut stack_guard = false;

    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--type" => {
                index += 1;
                let value = args.get(index).ok_or("--type exige um valor")?;
                binary_type = parse_type(value)?;
            }
            "--caps" => {
                index += 1;
                let value = args.get(index).ok_or("--caps exige um valor")?;
                for name in value.split(',') {
                    let name = name.trim();
                    if name.is_empty() {
                        continue;
                    }
                    capabilities |= parse_capability(name)?;
                }
            }
            "--stack" => {
                index += 1;
                let value = args.get(index).ok_or("--stack exige um valor")?;
                stack_size = value
                    .parse()
                    .map_err(|_| format!("--stack inválido: '{value}'"))?;
                if stack_size == 0 {
                    return Err("--stack não pode ser zero".into());
                }
            }
            "--stack-guard" => stack_guard = true,
            "-h" | "--help" => {
                print_usage();
                exit(0);
            }
            other if other.starts_with("--") => {
                return Err(format!("opção desconhecida '{other}'"));
            }
            _ => {
                if input.is_none() {
                    input = Some(args[index].clone());
                } else if output.is_none() {
                    output = Some(args[index].clone());
                } else {
                    return Err(format!("argumento extra inesperado '{}'", args[index]));
                }
            }
        }
        index += 1;
    }

    Ok(Options {
        input: input.ok_or("faltou o arquivo de entrada (.elf)")?,
        output: output.ok_or("faltou o arquivo de saída")?,
        binary_type,
        capabilities,
        stack_size,
        stack_guard,
    })
}

fn print_usage() {
    eprintln!(
        "uso: dnu-pack <entrada.elf> <saida> [opções]\n\
         \n\
         opções:\n\
           --type app|server|driver|runtime   tipo do binário DEF (padrão: app)\n\
           --caps fs,net,device,...           capacidades concedidas\n\
           --stack <bytes>                    tamanho da pilha (padrão: {DEFAULT_STACK_SIZE})\n\
           --stack-guard                      habilita guard page na pilha\n\
           -h, --help                         mostra esta ajuda"
    );
}

fn main() {
    let options = match parse_args() {
        Ok(options) => options,
        Err(err) => {
            eprintln!("dnu-pack: {err}");
            print_usage();
            exit(2);
        }
    };

    let elf = match fs::read(&options.input) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("dnu-pack: não consegui ler '{}': {err}", options.input);
            exit(1);
        }
    };

    let image = match pack_elf(
        &elf,
        options.binary_type,
        options.capabilities,
        options.stack_size,
        options.stack_guard,
    ) {
        Ok(image) => image,
        Err(err) => {
            eprintln!("dnu-pack: {err}");
            exit(1);
        }
    };

    if let Err(err) = fs::write(&options.output, &image) {
        eprintln!("dnu-pack: não consegui gravar '{}': {err}", options.output);
        exit(1);
    }

    eprintln!(
        "dnu-pack: '{}' -> '{}' ({} bytes)",
        options.input,
        options.output,
        image.len()
    );
}
