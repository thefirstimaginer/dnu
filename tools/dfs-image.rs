use block::BlockDevice;
use std::cell::RefCell;
use std::env;
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::process::ExitCode;

#[path = "../src/block.rs"]
mod block;
#[path = "../src/dzimage.rs"]
mod dzimage;
#[path = "../src/dfs.rs"]
mod dfs;
#[path = "../src/gpt.rs"]
mod gpt;

mod ata {
    use crate::block::{BlockDevice, BlockError, SECTOR_SIZE};

    pub struct PrimaryMaster;

    impl BlockDevice for PrimaryMaster {
        fn sector_count(&self) -> u64 {
            0
        }

        fn read_sector(
            &self,
            _lba: u64,
            _buffer: &mut [u8; SECTOR_SIZE],
        ) -> Result<(), BlockError> {
            Err(BlockError::NotReady)
        }

        fn write_sector(&self, _lba: u64, _buffer: &[u8; SECTOR_SIZE]) -> Result<(), BlockError> {
            Err(BlockError::NotReady)
        }

        fn flush(&self) -> Result<(), BlockError> {
            Err(BlockError::NotReady)
        }
    }
}

struct ImageDisk(RefCell<std::fs::File>);

impl block::BlockDevice for ImageDisk {
    fn sector_count(&self) -> u64 {
        self.0
            .borrow()
            .metadata()
            .map_or(0, |metadata| metadata.len() / 512)
    }

    fn read_sector(
        &self,
        lba: u64,
        output: &mut [u8; block::SECTOR_SIZE],
    ) -> Result<(), block::BlockError> {
        let mut file = self.0.borrow_mut();
        file.seek(SeekFrom::Start(lba * block::SECTOR_SIZE as u64))
            .and_then(|_| file.read_exact(output))
            .map_err(|_| block::BlockError::DeviceError)
    }

    fn write_sector(
        &self,
        lba: u64,
        input: &[u8; block::SECTOR_SIZE],
    ) -> Result<(), block::BlockError> {
        let mut file = self.0.borrow_mut();
        file.seek(SeekFrom::Start(lba * block::SECTOR_SIZE as u64))
            .and_then(|_| file.write_all(input))
            .map_err(|_| block::BlockError::DeviceError)
    }

    fn flush(&self) -> Result<(), block::BlockError> {
        self.0
            .borrow_mut()
            .sync_all()
            .map_err(|_| block::BlockError::DeviceError)
    }
}

const USAGE: &str =
    "usage: dfs-image DISK-IMAGE RAMFS-USTAR | dfs-image --dump-initramfs DZIMAGE OUT.tar | \
     dfs-image --dump-kernel DZIMAGE OUT.elf";

/// Extrai um membro do contêiner `dzImage` (o `initramfs` ou o `kernel`).
///
/// O disco instalado é semeado a partir do **mesmo** dzImage que a ISO
/// carrega, sem um `initramfs-installed.tar` paralelo: os dois lados leem a
/// tabela de membros com as constantes do formato.
fn dump_member(archive: &std::path::Path, output: &std::path::Path, kind: u32) -> Result<(), String> {
    let image = std::fs::read(archive).map_err(|error| format!("cannot read dzImage: {error}"))?;
    // A ABI do cabeçalho é aceita como esperada: a ferramenta extrai o membro,
    // não o executa. O CRC continua sendo conferido por `verify`.
    let abi = dzimage::header_abi(&image).ok_or("truncated dzImage")?;
    dzimage::verify(&image, abi.into()).map_err(|error| format!("bad dzImage: {error:?}"))?;
    let mut found = None;
    dzimage::for_each_member(&image, |member| {
        if member.kind == kind {
            found = Some(member);
        }
    })
    .ok_or("truncated dzImage")?;
    let member = found.ok_or_else(|| format!("dzImage has no member of kind {kind}"))?;
    std::fs::write(output, member.payload(&image))
        .map_err(|error| format!("cannot write {}: {error}", output.display()))?;
    println!("member {kind}: {}", output.display());
    Ok(())
}

fn run() -> Result<(), String> {
    let mut arguments = env::args_os().skip(1);
    let first = arguments.next().ok_or_else(|| USAGE.to_owned())?;
    // Modos utilitários: `--dump-initramfs`/`--dump-kernel DZIMAGE OUT`.
    if let Some(kind) = first.to_str().and_then(|flag| match flag {
        "--dump-initramfs" => Some(dzimage::KIND_INITRAMFS),
        "--dump-kernel" => Some(dzimage::KIND_KERNEL),
        _ => None,
    }) {
        let archive = arguments.next().ok_or_else(|| USAGE.to_owned())?;
        let output = arguments.next().ok_or_else(|| USAGE.to_owned())?;
        if arguments.next().is_some() {
            return Err(USAGE.to_owned());
        }
        return dump_member(
            std::path::Path::new(&archive),
            std::path::Path::new(&output),
            kind,
        );
    }
    // Modo principal: `dfs-image DISK-IMAGE RAMFS-USTAR` formata e semeia.
    let ramfs = arguments.next().ok_or_else(|| USAGE.to_owned())?;
    if arguments.next().is_some() {
        return Err(USAGE.to_owned());
    }
    let device = ImageDisk(RefCell::new(
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(first)
            .map_err(|error| format!("cannot open disk image: {error}"))?,
    ));
    let archive =
        std::fs::read(ramfs).map_err(|error| format!("cannot read RAMFS archive: {error}"))?;
    let table =
        gpt::Gpt::read_primary(&device).map_err(|error| format!("cannot read GPT: {error:?}"))?;
    let partition = table
        .find_partition(&device, &gpt::DFS_PARTITION_TYPE_GUID)
        .map_err(|error| format!("cannot find DFS partition: {error:?}"))?;
    let fs = dfs::Dfs::format(
        &device,
        partition.first_lba,
        partition.last_lba - partition.first_lba + 1,
    )
    .map_err(|error| format!("cannot format DFS: {error:?}"))?;
    fs.seed_from_ustar(&device, &archive)
        .map_err(|error| format!("cannot seed DFS: {error:?}"))?;
    device
        .flush()
        .map_err(|error| format!("cannot flush DFS: {error:?}"))?;
    println!(
        "Formatted and seeded DFS on GPT partition {}",
        partition.first_lba
    );
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("dfs-image: {error}");
            ExitCode::FAILURE
        }
    }
}
