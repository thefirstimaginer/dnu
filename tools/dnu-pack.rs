//! **dnu-pack** — converte um binário de userspace para o formato DEF
//! (Dreamcore Executable Format).
//!
//! Uso: dnu-pack <entrada> <saida.def>
//!
//! A entrada pode ser:
//!   * Um binário flat (código puro, sem cabeçalho ELF) — carregado em
//!     0x400000 com entry point no início.
//!   * Um arquivo ELF64 — os segmentos `PT_LOAD` são extraídos e viram
//!     segmentos DEF individuais, com endereços virtuais e permissões
//!     corretos.
//!
//! Cria um contêiner DEF com um segmento carregável por `PT_LOAD` ELF. O
//! kernel DNU carrega DEF via seu loader interno.

use std::env;
use std::fs;
use std::process::ExitCode;

const DEF_MAGIC: &[u8; 4] = b"DNUF";
const DEF_FORMAT_VERSION: u16 = 1;
const DEF_HEADER_SIZE: usize = 152;
const DEF_SEGMENT_SIZE: usize = 40;
const DEFAULT_VADDR: u64 = 0x0000_0000_0040_0000;

// Identificação ELF64.
const ELF_MAGIC: &[u8; 4] = b"\x7fELF";
const PT_LOAD: u32 = 1;
const PF_R: u32 = 4;
const PF_W: u32 = 2;
const PF_X: u32 = 1;

// Offsets do cabeçalho ELF64 (64 bytes).
const ELF_E_ENTRY: usize = 0x18;
const ELF_E_PHOFF: usize = 0x20;
const ELF_E_PHENTSIZE: usize = 0x36;
const ELF_E_PHNUM: usize = 0x38;

// Offsets de um cabeçalho de programa ELF64 (56 bytes).
const ELF_P_TYPE: usize = 0x00;
const ELF_P_FLAGS: usize = 0x04;
const ELF_P_OFFSET: usize = 0x08;
const ELF_P_VADDR: usize = 0x10;
const ELF_P_FILESZ: usize = 0x20;
const ELF_P_MEMSZ: usize = 0x28;

// Permissões de página (compatíveis com DefPermissions do kernel).
const PERM_READ: u32 = 1 << 0;
const PERM_WRITE: u32 = 1 << 1;
const PERM_EXECUTE: u32 = 1 << 2;

/// Um segmento carregável, com offsets tanto no arquivo de entrada (ELF)
/// quanto no arquivo de saída (DEF).
struct LoadSegment {
    elf_offset: u64,    // onde ler os dados no arquivo de entrada
    def_offset: u64,    // onde escrever os dados no arquivo DEF de saída
    file_size: u64,     // bytes a copiar (do arquivo)
    memory_size: u64,   // bytes a reservar na memória (>= file_size)
    virtual_address: u64,
    permissions: u32,   // R/W/X
}

fn read_u16(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

fn read_u32(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

fn read_u64(data: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
        data[offset + 4],
        data[offset + 5],
        data[offset + 6],
        data[offset + 7],
    ])
}

/// Permissões a partir dos flags `p_flags` do ELF (PF_R/PF_W/PF_X).
fn elf_flags_to_perms(flags: u32) -> u32 {
    let mut p: u32 = 0;
    if flags & PF_R != 0 {
        p |= PERM_READ;
    }
    if flags & PF_W != 0 {
        p |= PERM_WRITE;
    }
    if flags & PF_X != 0 {
        p |= PERM_EXECUTE;
    }
    p
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 || args.len() > 4 {
        eprintln!("Uso: dnu-pack <entrada> <saida.def> [vaddr_hex]");
        return ExitCode::FAILURE;
    }

    let input_path = &args[1];
    let output_path = &args[2];
    let default_vaddr: u64 = if args.len() == 4 {
        u64::from_str_radix(args[3].trim_start_matches("0x"), 16).unwrap_or(DEFAULT_VADDR)
    } else {
        DEFAULT_VADDR
    };

    let data = match fs::read(input_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("dnu-pack: não foi ler {}: {}", input_path, e);
            return ExitCode::FAILURE;
        }
    };

    // Detecta se é ELF64 (magic \x7fELF) ou binário flat.
    let (entry_point, segments, image_base, image_size): (
        u64,
        Vec<LoadSegment>,
        u64,
        u64,
    ) = if data.len() >= 16 && &data[0..4] == ELF_MAGIC {
        // ELF64: parseia os cabeçalhos de programa e extrai PT_LOAD.
        let entry = read_u64(&data, ELF_E_ENTRY);
        let phoff = read_u64(&data, ELF_E_PHOFF) as usize;
        let phentsize = read_u16(&data, ELF_E_PHENTSIZE) as usize;
        let phnum = read_u16(&data, ELF_E_PHNUM) as usize;

        if phoff == 0 || phentsize < 56 || phnum == 0 {
            eprintln!(
                "dnu-pack: ELF inválido: phoff={} phentsize={} phnum={}",
                phoff, phentsize, phnum
            );
            return ExitCode::FAILURE;
        }

        let mut raw_segs: Vec<LoadSegment> = Vec::new();
        let mut min_vaddr: Option<u64> = None;
        let mut max_end: u64 = 0;

        for i in 0..phnum {
            let base = phoff + i * phentsize;
            if base + 56 > data.len() {
                eprintln!("dnu-pack: cabeçalho de programa {} excede o arquivo", i);
                return ExitCode::FAILURE;
            }
            let p_type = read_u32(&data, base + ELF_P_TYPE);
            if p_type != PT_LOAD {
                continue;
            }
            let p_flags = read_u32(&data, base + ELF_P_FLAGS);
            let p_offset = read_u64(&data, base + ELF_P_OFFSET);
            let p_vaddr = read_u64(&data, base + ELF_P_VADDR);
            let p_filesz = read_u64(&data, base + ELF_P_FILESZ);
            let p_memsz = read_u64(&data, base + ELF_P_MEMSZ);

            if p_filesz > p_memsz {
                eprintln!(
                    "dnu-pack: segment {}: filesz > memsz (0x{:x} > 0x{:x})",
                    i, p_filesz, p_memsz
                );
                return ExitCode::FAILURE;
            }
            let end = p_offset
                .checked_add(p_filesz)
                .map_or(true, |e| e as usize > data.len());
            if end {
                eprintln!("dnu-pack: segment {}: file offset fora do arquivo", i);
                return ExitCode::FAILURE;
            }

            let perms = elf_flags_to_perms(p_flags);
            raw_segs.push(LoadSegment {
                elf_offset: p_offset,
                def_offset: 0, // preenchido depois
                file_size: p_filesz,
                memory_size: p_memsz,
                virtual_address: p_vaddr,
                permissions: perms,
            });

            min_vaddr = Some(min_vaddr.map_or(p_vaddr, |v| v.min(p_vaddr)));
            let seg_end = p_vaddr.saturating_add(p_memsz);
            if seg_end > max_end {
                max_end = seg_end;
            }
        }

        if raw_segs.is_empty() {
            eprintln!("dnu-pack: ELF não tem segmentos PT_LOAD");
            return ExitCode::FAILURE;
        }

        let base = min_vaddr.unwrap();
        let size = max_end.saturating_sub(base);

        eprintln!(
            "dnu-pack: ELF64: {} segmento(s) PT_LOAD, entry=0x{:016x}, base=0x{:016x}, size=0x{:x}",
            raw_segs.len(),
            entry,
            base,
            size
        );
        (entry, raw_segs, base, size)
    } else {
        // Binário flat: entry point no início do segmento.
        let segs = vec![LoadSegment {
            elf_offset: 0,
            def_offset: 0, // preenchido depois
            file_size: data.len() as u64,
            memory_size: data.len() as u64,
            virtual_address: default_vaddr,
            permissions: PERM_READ | PERM_EXECUTE, // R|X
        }];
        (
            default_vaddr,
            segs,
            default_vaddr,
            data.len() as u64,
        )
    };

    let segment_count: u32 = segments.len() as u32;
    let table_end = DEF_HEADER_SIZE + segment_count as usize * DEF_SEGMENT_SIZE;

    // Os payloads são colocados sequencialmente após a tabela de segmentos.
    // def_offset de cada segmento aponta para onde seus bytes ficam no DEF.
    let mut segments_fixed: Vec<LoadSegment> = Vec::new();
    let mut off = table_end as u64;
    let mut total_payload: u64 = 0;
    for seg in &segments {
        segments_fixed.push(LoadSegment {
            elf_offset: seg.elf_offset,
            def_offset: off,
            file_size: seg.file_size,
            memory_size: seg.memory_size,
            virtual_address: seg.virtual_address,
            permissions: seg.permissions,
        });
        off += seg.file_size;
        total_payload += seg.file_size;
    }
    let segments = segments_fixed;

    let total_size = table_end + total_payload as usize;
    let mut buf = vec![0u8; total_size];

    // magic
    buf[0..4].copy_from_slice(DEF_MAGIC);
    // arch = X86_64
    buf[4] = 0x01;
    // binary_type = App
    buf[5] = 0x00;
    // format_version (u16 LE)
    buf[6..8].copy_from_slice(&DEF_FORMAT_VERSION.to_le_bytes());
    // abi_version (u32 LE).
    buf[8..12].copy_from_slice(&1u32.to_le_bytes());
    // flags
    buf[12..16].copy_from_slice(&0u32.to_le_bytes());
    // segment_count (u32 LE)
    buf[16..20].copy_from_slice(&segment_count.to_le_bytes());
    // reserved
    buf[20..24].copy_from_slice(&0u32.to_le_bytes());
    // entry_point (u64 LE)
    buf[24..32].copy_from_slice(&entry_point.to_le_bytes());
    // stack_size (u64 LE) — 64 KiB.
    buf[32..40].copy_from_slice(&(64 * 1024u64).to_le_bytes());
    // image_base (u64 LE)
    buf[40..48].copy_from_slice(&image_base.to_le_bytes());
    // image_size (u64 LE)
    buf[48..56].copy_from_slice(&image_size.to_le_bytes());
    // capabilities (32 bytes) — FS | SPAWN | DISPLAY | SUPERVISE.
    let capabilities: u64 = (1 << 0) | (1 << 3) | (1 << 5) | (1 << 6);
    buf[56..64].copy_from_slice(&capabilities.to_le_bytes());
    // signature (64 bytes, zerado) — já está zero.

    // Tabela de segmentos + payloads.
    let seg_start = DEF_HEADER_SIZE;
    for (index, seg) in segments.iter().enumerate() {
        let base = seg_start + index * DEF_SEGMENT_SIZE;
        buf[base..base + 8].copy_from_slice(&seg.def_offset.to_le_bytes());
        buf[base + 8..base + 16].copy_from_slice(&seg.file_size.to_le_bytes());
        buf[base + 16..base + 24].copy_from_slice(&seg.memory_size.to_le_bytes());
        buf[base + 24..base + 32].copy_from_slice(&seg.virtual_address.to_le_bytes());
        buf[base + 32..base + 36].copy_from_slice(&seg.permissions.to_le_bytes());
        // reserved (4 bytes, já está zero)

        // Copia o payload deste segmento do arquivo de entrada para o DEF.
        let src = &data[seg.elf_offset as usize..(seg.elf_offset + seg.file_size) as usize];
        buf[seg.def_offset as usize..(seg.def_offset + seg.file_size) as usize].copy_from_slice(src);
    }

    if let Err(e) = fs::write(output_path, &buf) {
        eprintln!("dnu-pack: não foi escrever {}: {}", output_path, e);
        return ExitCode::FAILURE;
    }

    println!(
        "dnu-pack: {} -> {} ({} bytes, {} segmento(s), entry=0x{:016x})",
        input_path,
        output_path,
        buf.len(),
        segment_count,
        entry_point
    );
    ExitCode::SUCCESS
}
