#![allow(dead_code)]
//! Integration test for the on-disk installer.
//!
//! `installer.rs` reaches into `crate::accounts` (to validate and build the
//! credentials database) and `accounts` in turn uses `crate::crypto`, so both
//! modules have to be declared here for the harness to build.


// A ABI é compilada dentro do harness (ver `tools/abi.rs`): sem cargo não há
// `--extern` para o crate dela.
#[path = "../tools/abi.rs"]
pub mod abi;

#[path = "../src/block.rs"]
mod block;

// `installer.rs` alcança `crate::usr::accounts` (que usa `crate::usr::crypto`).
// Os módulos reais são declarados com `#[path]` relativo a este arquivo e
// reexportados sob `usr`, o mesmo caminho que o kernel usa em produção.
#[path = "../src/accounts.rs"]
pub mod accounts_impl;
#[path = "../src/crypto.rs"]
pub mod crypto_impl;
pub mod usr {
    pub use super::accounts_impl as accounts;
    pub use super::crypto_impl as crypto;
}

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

mod ramfs {
    pub fn read(_path: &str) -> Option<&'static [u8]> {
        None
    }

    // Sem raiz volátil montada no harness, a gravação em RAMFS é um no-op.
    pub fn write_file_as(
        _path: &str,
        _bytes: &[u8],
        _uid: u32,
        _gid: u32,
    ) -> Result<(), FsError> {
        Ok(())
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum FsError {
        NotFound,
    }
}

// O harness não monta raiz persistente: o instalador escreve só no
// dispositivo que recebe (SparseDisk), pelas mesmas chamadas do kernel.
mod vfs {
    pub fn has_persistent_root() -> bool {
        false
    }
}

#[path = "../src/installer.rs"]
mod installer;

use std::cell::RefCell;
use std::collections::BTreeMap;

use block::{BlockDevice, BlockError, SECTOR_SIZE};
use gpt::Gpt;
use std::vec::Vec;

struct SparseDisk {
    sectors: u64,
    contents: RefCell<BTreeMap<u64, [u8; SECTOR_SIZE]>>,
}

impl SparseDisk {
    fn new(sectors: u64) -> Self {
        Self {
            sectors,
            contents: RefCell::new(BTreeMap::new()),
        }
    }
}

impl BlockDevice for SparseDisk {
    fn sector_count(&self) -> u64 {
        self.sectors
    }

    fn read_sector(&self, lba: u64, buffer: &mut [u8; SECTOR_SIZE]) -> Result<(), BlockError> {
        if lba >= self.sectors {
            return Err(BlockError::OutOfBounds);
        }
        buffer.copy_from_slice(
            self.contents
                .borrow()
                .get(&lba)
                .map_or(&[0; SECTOR_SIZE], |sector| sector),
        );
        Ok(())
    }

    fn write_sector(&self, lba: u64, buffer: &[u8; SECTOR_SIZE]) -> Result<(), BlockError> {
        if lba >= self.sectors {
            return Err(BlockError::OutOfBounds);
        }
        self.contents.borrow_mut().insert(lba, *buffer);
        Ok(())
    }

    fn flush(&self) -> Result<(), BlockError> {
        Ok(())
    }
}

#[test]
fn installs_a_gpt_esp_and_boot_payload() {
    let disk = SparseDisk::new(300_000);
    let ramfs = minimal_ustar();
    installer::install(
        &disk,
        b"compressed kernel",
        &ramfs,
        b"efi loader",
        b"limine config",
        b"startup script",
    )
    .unwrap();

    let gpt = Gpt::read_primary(&disk).unwrap();
    let efi_partition = gpt
        .find_partition(
            &disk,
            &[
                0x28, 0x73, 0x2a, 0xc1, 0x1f, 0xf8, 0xd2, 0x11, 0xba, 0x4b, 0, 0xa0, 0xc9, 0x3e,
                0xc9, 0x3b,
            ],
        )
        .unwrap();
    let dfs = gpt
        .find_partition(&disk, &gpt::DFS_PARTITION_TYPE_GUID)
        .unwrap();
    assert_eq!(efi_partition.first_lba, 2048);
    assert_eq!(dfs.first_lba, 264_192);
    let mounted = dfs::Dfs::mount(&disk, dfs.first_lba, dfs.last_lba - dfs.first_lba + 1).unwrap();
    let mut init = [0; 16];
    assert_eq!(
        mounted.read_file(&disk, "sbin/init", 0, &mut init).unwrap(),
        8
    );
    assert_eq!(&init[..8], b"initdata");

    let mut boot = [0; SECTOR_SIZE];
    disk.read_sector(efi_partition.first_lba, &mut boot)
        .unwrap();
    assert_eq!(&boot[510..512], &[0x55, 0xaa]);
    assert_eq!(u16::from_le_bytes([boot[22], boot[23]]), 128);

    let mut root = [0; SECTOR_SIZE];
    disk.read_sector(2048 + 1 + 128 * 2, &mut root).unwrap();
    assert_eq!(&root[..11], b"EFI        ");
    assert_eq!(&root[32..43], b"BOOT       ");
    assert_eq!(root[64 + 11], 0x0f);
    assert_eq!(&root[96..107], b"LIMINE  CNF");
    assert_eq!(root[128 + 11], 0x0f);
    assert_eq!(&root[160..171], b"STARTUP NSH");

    let limine_cluster = u16::from_le_bytes([root[96 + 26], root[96 + 27]]) as u64;
    let data_start = 2048 + 1 + 128 * 2 + 32;
    let mut payload = [0; SECTOR_SIZE];
    disk.read_sector(data_start + (limine_cluster - 2) * 8, &mut payload)
        .unwrap();
    assert_eq!(&payload[..13], b"limine config");

    let mut boot_directory = [0; SECTOR_SIZE];
    disk.read_sector(data_start + 2 * 8, &mut boot_directory)
        .unwrap();
    assert_eq!(boot_directory[64 + 11], 0x0f);
    assert_eq!(&boot_directory[96..107], b"DZIMAGE    ");
    assert_eq!(boot_directory[128 + 11], 0x0f);
    assert_eq!(&boot_directory[160..171], b"INITRAM IMG");

    let mut protective_mbr = [0; SECTOR_SIZE];
    disk.read_sector(0, &mut protective_mbr).unwrap();
    assert_eq!(protective_mbr[450], 0xee);
    assert_eq!(&protective_mbr[510..512], &[0x55, 0xaa]);
}

fn minimal_ustar() -> Vec<u8> {
    let mut archive = vec![0u8; SECTOR_SIZE * 4];
    let dir = &mut archive[..SECTOR_SIZE];
    dir[..5].copy_from_slice(b"sbin/");
    dir[100..108].copy_from_slice(b"0000755\0");
    dir[124..136].copy_from_slice(b"00000000000\0");
    dir[156] = b'5';

    let file = &mut archive[SECTOR_SIZE..SECTOR_SIZE * 2];
    file[..9].copy_from_slice(b"sbin/init");
    file[100..108].copy_from_slice(b"0000755\0");
    file[124..136].copy_from_slice(b"00000000010\0");
    file[156] = b'0';
    archive[SECTOR_SIZE * 2..SECTOR_SIZE * 2 + 8].copy_from_slice(b"initdata");
    archive
}

#[test]
fn refuses_a_disk_too_small_before_writing() {
    let disk = SparseDisk::new(100_000);
    let result = installer::install(&disk, b"d", b"r", b"e", b"c", b"s");
    assert_eq!(result, Err(installer::InstallError::DiskTooSmall));
    assert!(disk.contents.borrow().is_empty());
}
