#![allow(dead_code)]

#[path = "../drivers/block.rs"]
mod block;
#[path = "../fs/gpt.rs"]
mod gpt;

use block::{BlockDevice, BlockError, SECTOR_SIZE};
use gpt::{Gpt, GptError};

const DISK_SECTORS: usize = 128;

struct MemoryDisk([[u8; SECTOR_SIZE]; DISK_SECTORS]);

impl BlockDevice for MemoryDisk {
    fn sector_count(&self) -> u64 {
        DISK_SECTORS as u64
    }

    fn read_sector(
        &self,
        lba: u64,
        buffer: &mut [u8; SECTOR_SIZE],
    ) -> Result<(), BlockError> {
        let Some(sector) = self.0.get(lba as usize) else {
            return Err(BlockError::OutOfBounds);
        };
        buffer.copy_from_slice(sector);
        Ok(())
    }

    fn write_sector(
        &self,
        _lba: u64,
        _buffer: &[u8; SECTOR_SIZE],
    ) -> Result<(), BlockError> {
        Err(BlockError::DeviceError)
    }

    fn flush(&self) -> Result<(), BlockError> {
        Ok(())
    }
}

fn valid_gpt() -> MemoryDisk {
    let mut disk = MemoryDisk([[0; SECTOR_SIZE]; DISK_SECTORS]);
    let entries = &mut disk.0[2];
    entries[..16].copy_from_slice(&[0x42; 16]);
    entries[16..32].copy_from_slice(&[0x24; 16]);
    entries[32..40].copy_from_slice(&34u64.to_le_bytes());
    entries[40..48].copy_from_slice(&90u64.to_le_bytes());
    entries[48..56].copy_from_slice(&0u64.to_le_bytes());
    let entries_crc = crc32(entries);

    let header = &mut disk.0[1];
    header[..8].copy_from_slice(b"EFI PART");
    header[8..12].copy_from_slice(&0x0001_0000u32.to_le_bytes());
    header[12..16].copy_from_slice(&92u32.to_le_bytes());
    header[24..32].copy_from_slice(&1u64.to_le_bytes());
    header[32..40].copy_from_slice(&127u64.to_le_bytes());
    header[40..48].copy_from_slice(&34u64.to_le_bytes());
    header[48..56].copy_from_slice(&126u64.to_le_bytes());
    header[72..80].copy_from_slice(&2u64.to_le_bytes());
    header[80..84].copy_from_slice(&4u32.to_le_bytes());
    header[84..88].copy_from_slice(&128u32.to_le_bytes());
    header[88..92].copy_from_slice(&entries_crc.to_le_bytes());
    header[16..20].fill(0);
    let header_crc = crc32(&header[..92]);
    header[16..20].copy_from_slice(&header_crc.to_le_bytes());
    disk
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

#[test]
fn reads_and_finds_a_gpt_partition() {
    let disk = valid_gpt();
    let table = Gpt::read_primary(&disk).unwrap();
    let partition = table.find_partition(&disk, &[0x42; 16]).unwrap();
    assert_eq!(partition.first_lba, 34);
    assert_eq!(partition.last_lba, 90);
    assert_eq!(partition.unique_guid, [0x24; 16]);
}

#[test]
fn rejects_corrupted_primary_header_checksum() {
    let mut disk = valid_gpt();
    disk.0[1][40] ^= 1;
    assert_eq!(
        Gpt::read_primary(&disk).unwrap_err(),
        GptError::HeaderChecksum
    );
}

#[test]
fn rejects_corrupted_partition_array_checksum() {
    let mut disk = valid_gpt();
    disk.0[2][32] ^= 1;
    assert_eq!(
        Gpt::read_primary(&disk).unwrap_err(),
        GptError::EntryChecksum
    );
}

#[test]
fn rejects_non_gpt_disks() {
    let disk = MemoryDisk([[0; SECTOR_SIZE]; DISK_SECTORS]);
    assert_eq!(
        Gpt::read_primary(&disk).unwrap_err(),
        GptError::InvalidSignature
    );
}
