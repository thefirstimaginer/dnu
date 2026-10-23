use crate::block::{BlockDevice, BlockError, SECTOR_SIZE};

const GPT_HEADER_SIGNATURE: &[u8; 8] = b"EFI PART";
const GPT_HEADER_MIN_SIZE: usize = 92;
const GPT_MAX_ENTRY_COUNT: u32 = 128;
const GPT_MAX_ENTRY_SIZE: u32 = 512;
pub const DFS_PARTITION_TYPE_GUID: [u8; 16] = [
    0x9d, 0x2c, 0x7f, 0x8a, 0x31, 0x6b, 0x52, 0x4e, 0x9b, 0x14, 0x44, 0x53, 0x46, 0x53, 0x00, 0x01,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GptError {
    Block(BlockError),
    InvalidSignature,
    InvalidHeader,
    HeaderChecksum,
    EntryChecksum,
    InvalidPartition,
    PartitionNotFound,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Partition {
    pub type_guid: [u8; 16],
    pub unique_guid: [u8; 16],
    pub first_lba: u64,
    pub last_lba: u64,
    pub attributes: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct Gpt {
    first_usable_lba: u64,
    last_usable_lba: u64,
    entries_lba: u64,
    entry_count: u32,
    entry_size: u32,
}

impl Gpt {
    pub fn read_primary(device: &impl BlockDevice) -> Result<Self, GptError> {
        let mut sector = [0; SECTOR_SIZE];
        device
            .read_sector(1, &mut sector)
            .map_err(GptError::Block)?;
        if &sector[..8] != GPT_HEADER_SIGNATURE {
            return Err(GptError::InvalidSignature);
        }

        let header_size = read_u32(&sector, 12) as usize;
        let current_lba = read_u64(&sector, 24);
        let backup_lba = read_u64(&sector, 32);
        let first_usable_lba = read_u64(&sector, 40);
        let last_usable_lba = read_u64(&sector, 48);
        let entries_lba = read_u64(&sector, 72);
        let entry_count = read_u32(&sector, 80);
        let entry_size = read_u32(&sector, 84);
        let entries_checksum = read_u32(&sector, 88);

        if !(GPT_HEADER_MIN_SIZE..=SECTOR_SIZE).contains(&header_size)
            || current_lba != 1
            || backup_lba >= device.sector_count()
            || backup_lba <= last_usable_lba
            || first_usable_lba < 2
            || first_usable_lba > last_usable_lba
            || last_usable_lba >= device.sector_count()
            || entry_count == 0
            || entry_count > GPT_MAX_ENTRY_COUNT
            || entry_size < 128
            || entry_size > GPT_MAX_ENTRY_SIZE
            || entry_size % 8 != 0
            || SECTOR_SIZE as u32 % entry_size != 0
        {
            return Err(GptError::InvalidHeader);
        }

        let entries_bytes = (entry_count as u64)
            .checked_mul(entry_size as u64)
            .ok_or(GptError::InvalidHeader)?;
        let entries_sectors = entries_bytes
            .checked_add(SECTOR_SIZE as u64 - 1)
            .ok_or(GptError::InvalidHeader)?
            / SECTOR_SIZE as u64;
        let entries_end = entries_lba
            .checked_add(entries_sectors)
            .ok_or(GptError::InvalidHeader)?;
        if entries_lba < 2 || entries_end > first_usable_lba {
            return Err(GptError::InvalidHeader);
        }

        let expected_header_checksum = read_u32(&sector, 16);
        sector[16..20].fill(0);
        if crc32(&sector[..header_size]) != expected_header_checksum {
            return Err(GptError::HeaderChecksum);
        }

        if partition_entries_checksum(device, entries_lba, entries_bytes)? != entries_checksum {
            return Err(GptError::EntryChecksum);
        }

        Ok(Self {
            first_usable_lba,
            last_usable_lba,
            entries_lba,
            entry_count,
            entry_size,
        })
    }

    pub fn partition(
        &self,
        device: &impl BlockDevice,
        index: u32,
    ) -> Result<Option<Partition>, GptError> {
        if index >= self.entry_count {
            return Ok(None);
        }
        let byte_offset = index as u64 * self.entry_size as u64;
        let sector_lba = self.entries_lba + byte_offset / SECTOR_SIZE as u64;
        let entry_offset = (byte_offset % SECTOR_SIZE as u64) as usize;
        let mut sector = [0; SECTOR_SIZE];
        device
            .read_sector(sector_lba, &mut sector)
            .map_err(GptError::Block)?;
        let entry = &sector[entry_offset..entry_offset + self.entry_size as usize];
        let mut type_guid = [0; 16];
        type_guid.copy_from_slice(&entry[..16]);
        if type_guid == [0; 16] {
            return Ok(None);
        }
        let mut unique_guid = [0; 16];
        unique_guid.copy_from_slice(&entry[16..32]);
        let partition = Partition {
            type_guid,
            unique_guid,
            first_lba: read_u64(entry, 32),
            last_lba: read_u64(entry, 40),
            attributes: read_u64(entry, 48),
        };
        if partition.first_lba < self.first_usable_lba
            || partition.last_lba > self.last_usable_lba
            || partition.first_lba > partition.last_lba
        {
            return Err(GptError::InvalidPartition);
        }
        Ok(Some(partition))
    }

    pub fn find_partition(
        &self,
        device: &impl BlockDevice,
        type_guid: &[u8; 16],
    ) -> Result<Partition, GptError> {
        for index in 0..self.entry_count {
            if let Some(partition) = self.partition(device, index)? {
                if &partition.type_guid == type_guid {
                    return Ok(partition);
                }
            }
        }
        Err(GptError::PartitionNotFound)
    }
}

fn partition_entries_checksum(
    device: &impl BlockDevice,
    entries_lba: u64,
    entries_bytes: u64,
) -> Result<u32, GptError> {
    let mut checksum = !0u32;
    let mut remaining = entries_bytes;
    let mut lba = entries_lba;
    while remaining != 0 {
        let mut sector = [0; SECTOR_SIZE];
        device
            .read_sector(lba, &mut sector)
            .map_err(GptError::Block)?;
        let length = remaining.min(SECTOR_SIZE as u64) as usize;
        checksum = crc32_update(checksum, &sector[..length]);
        remaining -= length as u64;
        lba += 1;
    }
    Ok(!checksum)
}

fn crc32(bytes: &[u8]) -> u32 {
    !crc32_update(!0, bytes)
}

fn crc32_update(mut crc: u32, bytes: &[u8]) -> u32 {
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    crc
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}
