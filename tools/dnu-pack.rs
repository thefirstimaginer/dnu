//! **dnu-pack** — converte um binário de userspace para o formato DEF
//! (Dreamcore Executable Format).
//!
//! Uso: dnu-pack <entrada> <saida.def>
//!
//! A entrada pode ser:
//!   * Um binário flat (código puro, sem cabeçalho ELF) — carregado em
//!     0x400000 com entry point no início.
//!   * Um arquivo ELF64 (via objcopy -O binary para flatten primeiro).
//!
//! Cria um contêiner DEF com um único segmento carregável. O kernel DNU
//! carrega DEF via seu loader interno.

use std::env;
use std::fs;
use std::process::ExitCode;

const DEF_MAGIC: &[u8; 4] = b"DNUF";
const DEF_FORMAT_VERSION: u16 = 1;
const DEF_HEADER_SIZE: usize = 152;
const DEF_SEGMENT_SIZE: usize = 40;
const DEFAULT_VADDR: u64 = 0x0000_0000_0040_0000;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 || args.len() > 4 {
        eprintln!("Uso: dnu-pack <entrada> <saida.def> [vaddr_hex]");
        return ExitCode::FAILURE;
    }

    let input_path = &args[1];
    let output_path = &args[2];
    let virtual_address: u64 = if args.len() == 4 {
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
    let (entry_point, payload, virtual_address, permissions) =
        if data.len() >= 4 && &data[0..4] == b"\x7fELF" {
            // ELF64: extrai entry point do cabeçalho (offset 0x18).
            // AVISO: este empacotador não faz parse de segmentos ELF; use
            // `objcopy -O binary` para converter para flat primeiro.
            eprintln!(
                "dnu-pack: aviso: entrada é ELF64; empacotando como flat binary.\n  \
                 Use `objcopy -O binary <elf> <flat>` para evitar cabeçalhos ELF."
            );
            let entry_point = u64::from_le_bytes([
                data[0x18], data[0x19], data[0x1a], data[0x1b],
                data[0x1c], data[0x1d], data[0x1e], data[0x1f],
            ]);
            (entry_point, data, virtual_address, 5u32) // R|X
        } else {
            // Binário flat: entry point no início do segmento.
            (virtual_address, data, virtual_address, 5u32) // R|X
        };

    // Cabeçalho DEF (152 bytes, tudo little-endian):
    //   0   4  magic "DNUF"
    //   4   1  arch = X86_64 (0x01)
    //   5   1  binary_type = APP (0x01)
    //   6   2  format_version
    //   8   4  abi_version
    //   12  4  flags
    //   16  4  segment_count
    //   20  4  reserved
    //   24  8  entry_point
    //   32  8  stack_size
    //   40  8  image_base
    //   48  8  image_size
    //   56  32 capabilities
    //   88  64 signature
    //   152 ... tabela de segmentos
    let segment_count: u32 = 1;
    let table_end = DEF_HEADER_SIZE + segment_count as usize * DEF_SEGMENT_SIZE;
    let file_offset = table_end as u64;
    let file_size = payload.len() as u64;
    let memory_size = file_size;

    let total_size = table_end + payload.len();
    let mut buf = vec![0u8; total_size];

    // magic
    buf[0..4].copy_from_slice(DEF_MAGIC);
    // arch = X86_64
    buf[4] = 0x01;
    // binary_type = APP
    buf[5] = 0x01;
    // format_version (u16 LE)
    buf[6..8].copy_from_slice(&DEF_FORMAT_VERSION.to_le_bytes());
    // abi_version (u32 LE)
    buf[8..12].copy_from_slice(&1u32.to_le_bytes());
    // flags
    buf[12..16].copy_from_slice(&0u32.to_le_bytes());
    // segment_count (u32 LE)
    buf[16..20].copy_from_slice(&segment_count.to_le_bytes());
    // reserved
    buf[20..24].copy_from_slice(&0u32.to_le_bytes());
    // entry_point (u64 LE)
    buf[24..32].copy_from_slice(&entry_point.to_le_bytes());
    // stack_size (u64 LE) — 64 KiB
    buf[32..40].copy_from_slice(&(64 * 1024u64).to_le_bytes());
    // image_base (u64 LE)
    buf[40..48].copy_from_slice(&virtual_address.to_le_bytes());
    // image_size (u64 LE)
    buf[48..56].copy_from_slice(&memory_size.to_le_bytes());
    // capabilities (32 bytes)
    let capabilities: u64 = (1 << 0) | (1 << 3) | (1 << 5) | (1 << 6);
    buf[56..64].copy_from_slice(&capabilities.to_le_bytes());
    // signature (64 bytes, zerado) — já está zero

    // Tabela de segmentos (1 entrada, 40 bytes):
    let seg_start = DEF_HEADER_SIZE;
    buf[seg_start..seg_start + 8].copy_from_slice(&file_offset.to_le_bytes());
    buf[seg_start + 8..seg_start + 16].copy_from_slice(&file_size.to_le_bytes());
    buf[seg_start + 16..seg_start + 24].copy_from_slice(&memory_size.to_le_bytes());
    buf[seg_start + 24..seg_start + 32].copy_from_slice(&virtual_address.to_le_bytes());
    buf[seg_start + 32..seg_start + 36].copy_from_slice(&permissions.to_le_bytes());
    // reserved (4 bytes, já está zero)

    // Payload
    buf[table_end..table_end + payload.len()].copy_from_slice(&payload);

    if let Err(e) = fs::write(output_path, &buf) {
        eprintln!("dnu-pack: não foi escrever {}: {}", output_path, e);
        return ExitCode::FAILURE;
    }

    println!(
        "dnu-pack: {} -> {} ({} bytes, 1 segmento, entry=0x{:016x})",
        input_path,
        output_path,
        buf.len(),
        entry_point
    );
    ExitCode::SUCCESS
}
