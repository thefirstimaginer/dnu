#![allow(dead_code)]
//! Integration test for the VFS facade over the RAMFS/overlay backend.
//!
//! The module list below mirrors what `dnu_kernel/src/main.rs` wires together, so the
//! routing exercised here is the real one. Keeping this compilable also guards
//! the RAMFS tests that used to fail to build when run standalone.
//!
//! Execute com:
//! `rustup run stable rustc --edition 2021 --test dnu_kernel/src/tests/vfs.rs -o /tmp/vfs-tests && /tmp/vfs-tests`

mod ata {
    use crate::block::{BlockDevice, BlockError, SECTOR_SIZE};

    pub struct PrimaryMaster;

    /// O VFS consulta a presença do disco pelo número de setores.
    pub fn sector_count() -> u64 {
        0
    }

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

#[path = "../drivers/block.rs"]
mod block;
// `dfs::mount_primary` consulta o disco primário por `ata::sector_count`, que
// o `ata` de teste precisa expor mesmo sem I/O real.
#[path = "../fs/dfs/dfs.rs"]
mod dfs;
#[path = "../fs/gpt.rs"]
mod gpt;
#[path = "../fs/ramfs.rs"]
mod ramfs;
#[path = "../fs/vfs.rs"]
mod vfs;

// `ramfs`/`vfs` resolvem a configuração por `crate::usr::config`. O módulo real
// é declarado com `#[path]` relativo a este arquivo e reexportado sob `usr`.
#[path = "../usr/config.rs"]
pub mod config_impl;
pub mod usr {
    pub use super::config_impl as config;
}

use vfs::{FileSystem, Vfs};

/// The VFS asks `config::keyboard_layout` while parsing nothing, but the RAMFS
/// tests rely on a mounted archive. Mount a tiny USTAR image with one file.
fn mounted_overlay() -> &'static [u8] {
    // One 512-byte header for `bin/hello` plus its 5-byte payload, padded.
    let mut archive = vec![0u8; 1024];
    archive[..5].copy_from_slice(b"hello");
    archive[345..348].copy_from_slice(b"bin");
    archive[100..108].copy_from_slice(b"0000755\0");
    archive[124..136].copy_from_slice(b"00000000005\0");
    archive[156] = b'0';
    archive[257..262].copy_from_slice(b"ustar");
    archive[512..517].copy_from_slice(b"world");
    Box::leak(archive.into_boxed_slice())
}

#[test]
fn routes_reads_writes_and_metadata_through_the_active_backend() {
    let archive = mounted_overlay();
    ramfs::mount(archive);

    let fs = Vfs::current();

    // Read-only view of the USTAR layer.
    assert!(fs.is_file("bin/hello"));
    assert!(!fs.is_directory("bin/hello"));
    assert_eq!(fs.read_static("bin/hello"), Some(&b"world"[..]));
    assert_eq!(fs.file_len("bin/hello"), Ok(5));

    let mut buffer = [0u8; 8];
    assert_eq!(fs.read_at("bin/hello", 0, &mut buffer), Ok(5));
    assert_eq!(&buffer[..5], b"world");

    // A missing path is reported uniformly through the VFS error type.
    assert_eq!(
        fs.read_at("bin/missing", 0, &mut buffer),
        Err(vfs::VfsError::NotFound)
    );

    // Mutations land in the overlay and are visible through the same handle.
    assert_eq!(fs.write_file_as("bin/greeting", b"ola mundo", 0, 0), Ok(()));
    assert!(fs.is_file("bin/greeting"));
    assert_eq!(fs.read_static("bin/greeting"), Some(&b"ola mundo"[..]));

    let metadata = fs.metadata("bin/greeting").expect("overlay metadata");
    assert_eq!(metadata.size, 9);
    assert!(!metadata.is_directory);

    // Directory listing merges the archive and the overlay.
    let mut listing = [0u8; 64];
    let length = fs
        .list_directory("bin", false, &mut listing)
        .expect("listing bin");
    let listing = &listing[..length];
    assert!(listing.windows(5).any(|w| w == b"hello"));
    assert!(listing.windows(8).any(|w| w == b"greeting"));

    assert_eq!(fs.remove("bin/greeting", false), Ok(()));
    assert!(!fs.is_file("bin/greeting"));
}

#[test]
fn reports_that_no_persistent_root_is_mounted() {
    // No DFS was mounted in this test process.
    assert!(!vfs::has_persistent_root());
}

#[test]
fn error_codes_follow_the_syscall_abi() {
    assert_eq!(vfs::VfsError::NotFound.to_errno(), -2);
    assert_eq!(vfs::VfsError::IsDirectory.to_errno(), -21);
    assert_eq!(vfs::VfsError::NoSpace.to_errno(), -28);
    assert_eq!(vfs::VfsError::Io.to_errno(), -5);
}
