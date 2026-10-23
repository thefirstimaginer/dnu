use core::arch::asm;
use core::cell::UnsafeCell;

use crate::block::{BlockDevice, BlockError, SECTOR_SIZE};

const DATA: u16 = 0x1f0;
const SECTOR_COUNT: u16 = 0x1f2;
const LBA_LOW: u16 = 0x1f3;
const LBA_MID: u16 = 0x1f4;
const LBA_HIGH: u16 = 0x1f5;
const DRIVE: u16 = 0x1f6;
const STATUS_COMMAND: u16 = 0x1f7;
const ALT_STATUS_CONTROL: u16 = 0x3f6;
const POLL_LIMIT: usize = 1_000_000;
const LBA28_SECTOR_LIMIT: u64 = 1 << 28;

struct DiskState {
    sectors: u64,
    ready: bool,
}

impl DiskState {
    const fn new() -> Self {
        Self {
            sectors: 0,
            ready: false,
        }
    }
}

struct SharedDisk(UnsafeCell<DiskState>);
unsafe impl Sync for SharedDisk {}
static DISK: SharedDisk = SharedDisk(UnsafeCell::new(DiskState::new()));

pub fn init() -> Result<u64, BlockError> {
    unsafe {
        *DISK.0.get() = DiskState::new();
    }
    identify()?;
    let mut identify_data = [0u8; SECTOR_SIZE];
    for word in 0..256 {
        let value = read_word(DATA);
        identify_data[word * 2..word * 2 + 2].copy_from_slice(&value.to_le_bytes());
    }

    let sectors = u32::from_le_bytes([
        identify_data[120],
        identify_data[121],
        identify_data[122],
        identify_data[123],
    ]) as u64;
    if sectors == 0 {
        return Err(BlockError::DeviceError);
    }

    unsafe {
        *DISK.0.get() = DiskState {
            sectors: sectors.min(LBA28_SECTOR_LIMIT),
            ready: true,
        };
    }
    Ok(sectors.min(LBA28_SECTOR_LIMIT))
}

fn identify() -> Result<(), BlockError> {
    write_port(DRIVE, 0xa0);
    io_delay();
    write_port(SECTOR_COUNT, 0);
    write_port(LBA_LOW, 0);
    write_port(LBA_MID, 0);
    write_port(LBA_HIGH, 0);
    write_port(STATUS_COMMAND, 0xec);
    io_delay();

    let status = read_port(STATUS_COMMAND);
    if status == 0 || status == 0xff {
        return Err(BlockError::NotReady);
    }

    for _ in 0..POLL_LIMIT {
        let status = read_port(STATUS_COMMAND);
        if status & 0x80 != 0 {
            core::hint::spin_loop();
        } else if status & 0x01 != 0 {
            return Err(BlockError::DeviceError);
        } else if status & 0x08 != 0 {
            return Ok(());
        } else {
            core::hint::spin_loop();
        }
    }
    Err(BlockError::Timeout)
}

fn read_sector(lba: u64, buffer: &mut [u8; SECTOR_SIZE]) -> Result<(), BlockError> {
    check_lba(lba)?;
    select_lba(lba);
    write_port(SECTOR_COUNT, 1);
    write_port(STATUS_COMMAND, 0x20);
    io_delay();
    wait_for_data()?;
    for chunk in buffer.chunks_exact_mut(2) {
        chunk.copy_from_slice(&read_word(DATA).to_le_bytes());
    }
    wait_not_busy()?;
    Ok(())
}

fn write_sector(lba: u64, buffer: &[u8; SECTOR_SIZE]) -> Result<(), BlockError> {
    check_lba(lba)?;
    select_lba(lba);
    write_port(SECTOR_COUNT, 1);
    write_port(STATUS_COMMAND, 0x30);
    io_delay();
    wait_for_data()?;
    for chunk in buffer.chunks_exact(2) {
        write_word(DATA, u16::from_le_bytes([chunk[0], chunk[1]]));
    }
    wait_not_busy()?;
    Ok(())
}

fn flush() -> Result<(), BlockError> {
    write_port(STATUS_COMMAND, 0xe7);
    io_delay();
    wait_not_busy()?;
    Ok(())
}

fn check_lba(lba: u64) -> Result<(), BlockError> {
    let state = unsafe { &*DISK.0.get() };
    if !state.ready {
        return Err(BlockError::NotReady);
    }
    if lba >= state.sectors || lba >= LBA28_SECTOR_LIMIT {
        return Err(BlockError::OutOfBounds);
    }
    Ok(())
}

fn select_lba(lba: u64) {
    write_port(DRIVE, 0xe0 | ((lba >> 24) as u8 & 0x0f));
    io_delay();
    write_port(LBA_LOW, lba as u8);
    write_port(LBA_MID, (lba >> 8) as u8);
    write_port(LBA_HIGH, (lba >> 16) as u8);
}

fn wait_for_data() -> Result<(), BlockError> {
    for _ in 0..POLL_LIMIT {
        let status = read_port(STATUS_COMMAND);
        if status & 0x01 != 0 || status & 0x20 != 0 {
            return Err(BlockError::DeviceError);
        }
        if status & 0x80 == 0 && status & 0x08 != 0 {
            return Ok(());
        }
        core::hint::spin_loop();
    }
    Err(BlockError::Timeout)
}

fn wait_not_busy() -> Result<(), BlockError> {
    for _ in 0..POLL_LIMIT {
        let status = read_port(STATUS_COMMAND);
        if status & 0x01 != 0 || status & 0x20 != 0 {
            return Err(BlockError::DeviceError);
        }
        if status & 0x80 == 0 {
            return Ok(());
        }
        core::hint::spin_loop();
    }
    Err(BlockError::Timeout)
}

fn io_delay() {
    for _ in 0..4 {
        let _ = read_port(ALT_STATUS_CONTROL);
    }
}

fn read_port(port: u16) -> u8 {
    let value: u8;
    unsafe {
        asm!(
            "in al, dx",
            in("dx") port,
            out("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}

fn write_port(port: u16, value: u8) {
    unsafe {
        asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

fn read_word(port: u16) -> u16 {
    let value: u16;
    unsafe {
        asm!(
            "in ax, dx",
            in("dx") port,
            out("ax") value,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}

fn write_word(port: u16, value: u16) {
    unsafe {
        asm!(
            "out dx, ax",
            in("dx") port,
            in("ax") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

pub struct PrimaryMaster;

pub fn is_ready() -> bool {
    unsafe { (*DISK.0.get()).ready }
}

pub fn sector_count() -> u64 {
    unsafe { (*DISK.0.get()).sectors }
}

impl BlockDevice for PrimaryMaster {
    fn sector_count(&self) -> u64 {
        sector_count()
    }

    fn read_sector(&self, lba: u64, buffer: &mut [u8; SECTOR_SIZE]) -> Result<(), BlockError> {
        read_sector(lba, buffer)
    }

    fn write_sector(&self, lba: u64, buffer: &[u8; SECTOR_SIZE]) -> Result<(), BlockError> {
        write_sector(lba, buffer)
    }

    fn flush(&self) -> Result<(), BlockError> {
        flush()
    }
}
