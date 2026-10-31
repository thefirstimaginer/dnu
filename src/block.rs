pub const SECTOR_SIZE: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockError {
    NotReady,
    OutOfBounds,
    DeviceError,
    Timeout,
}

pub trait BlockDevice {
    /// Returns the number of addressable 512-byte sectors.
    fn sector_count(&self) -> u64;

    /// Reads one complete sector at the given logical block address.
    fn read_sector(&self, lba: u64, buffer: &mut [u8; SECTOR_SIZE]) -> Result<(), BlockError>;

    /// Writes one complete sector at the given logical block address.
    fn write_sector(&self, lba: u64, buffer: &[u8; SECTOR_SIZE]) -> Result<(), BlockError>;

    /// Flushes completed writes from the device cache to media.
    fn flush(&self) -> Result<(), BlockError>;
}
