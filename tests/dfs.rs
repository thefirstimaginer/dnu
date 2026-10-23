#![allow(dead_code)]

#[path = "../drivers/block.rs"]
mod block;
#[path = "../fs/dfs/dfs.rs"]
mod dfs;
#[path = "../fs/gpt.rs"]
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

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::vec;
use std::vec::Vec;

use block::{BlockDevice, BlockError, SECTOR_SIZE};

struct SparseDisk {
    sectors: u64,
    contents: RefCell<BTreeMap<u64, [u8; SECTOR_SIZE]>>,
    writes_before_failure: Cell<Option<usize>>,
}

impl SparseDisk {
    fn new(sectors: u64) -> Self {
        Self {
            sectors,
            contents: RefCell::new(BTreeMap::new()),
            writes_before_failure: Cell::new(None),
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
        if let Some(remaining) = self.writes_before_failure.get() {
            if remaining == 0 {
                self.writes_before_failure.set(None);
                return Err(BlockError::DeviceError);
            }
            self.writes_before_failure.set(Some(remaining - 1));
        }
        self.contents.borrow_mut().insert(lba, *buffer);
        Ok(())
    }

    fn flush(&self) -> Result<(), BlockError> {
        Ok(())
    }
}

#[test]
fn persists_files_and_recursively_removes_directories() {
    let disk = SparseDisk::new(32_768);
    let start_lba = 2_048;
    let sectors = 28_000;
    let fs = dfs::Dfs::format(&disk, start_lba, sectors).unwrap();
    fs.create_directory(&disk, "home", 0o755, 1000, 1000)
        .unwrap();
    fs.create_directory(&disk, "home/.cache", 0o700, 1000, 1000)
        .unwrap();
    fs.create_directory(&disk, "home/.cache/nested", 0o700, 1000, 1000)
        .unwrap();

    let contents: Vec<u8> = (0..20_000).map(|byte| (byte % 251) as u8).collect();
    assert_eq!(
        fs.write_file(
            &disk,
            "home/.cache/nested/data",
            0,
            &contents,
            true,
            0o600,
            1000,
            1000
        )
        .unwrap(),
        contents.len()
    );

    let mounted = dfs::Dfs::mount(&disk, start_lba, sectors).unwrap();
    let mut readback = vec![0; contents.len()];
    assert_eq!(
        mounted
            .read_file(&disk, "home/.cache/nested/data", 0, &mut readback)
            .unwrap(),
        contents.len()
    );
    assert_eq!(readback, contents);

    let mut visible = [0; 64];
    assert_eq!(
        mounted
            .read_directory(&disk, "home", false, &mut visible)
            .unwrap(),
        0
    );
    assert_eq!(
        mounted
            .read_directory(&disk, "home", true, &mut visible)
            .unwrap(),
        7
    );
    assert_eq!(&visible[..7], b".cache\n");

    assert_eq!(
        mounted.remove(&disk, "home/.cache", false),
        Err(dfs::DfsError::DirectoryNotEmpty)
    );
    mounted.remove(&disk, "home/.cache", true).unwrap();
    assert_eq!(
        dfs::Dfs::mount(&disk, start_lba, sectors)
            .unwrap()
            .stat(&disk, "home/.cache"),
        Err(dfs::DfsError::NotFound)
    );
}

#[test]
fn replays_a_committed_metadata_transaction_after_interrupted_write() {
    let disk = SparseDisk::new(16_384);
    let start_lba = 2_048;
    let sectors = 12_000;
    dfs::Dfs::format(&disk, start_lba, sectors).unwrap();

    // Fail while applying the inode after the journal payload and commit record
    // have reached the device.
    disk.writes_before_failure.set(Some(2));
    assert!(dfs::Dfs::mount(&disk, start_lba, sectors)
        .unwrap()
        .create_directory(&disk, "recovered", 0o755, 0, 0)
        .is_err());

    let mounted = dfs::Dfs::mount(&disk, start_lba, sectors).unwrap();
    assert!(mounted.stat(&disk, "recovered").unwrap().is_directory);
}

#[test]
fn reserves_metadata_spanning_multiple_bitmap_sectors() {
    let disk = SparseDisk::new(140_000_000);
    let fs = dfs::Dfs::format(&disk, 0, disk.sector_count()).unwrap();
    let mut reserved_metadata_sector = [0; SECTOR_SIZE];
    disk.read_sector(4_096, &mut reserved_metadata_sector)
        .unwrap();

    fs.write_file(&disk, "probe", 0, b"dfs", true, 0o600, 0, 0)
        .unwrap();

    let mut after_write = [0; SECTOR_SIZE];
    disk.read_sector(4_096, &mut after_write).unwrap();
    assert_eq!(after_write, reserved_metadata_sector);
    let mounted = dfs::Dfs::mount(&disk, 0, disk.sector_count()).unwrap();
    let mut contents = [0; 3];
    assert_eq!(
        mounted.read_file(&disk, "probe", 0, &mut contents).unwrap(),
        contents.len()
    );
    assert_eq!(&contents, b"dfs");
}
