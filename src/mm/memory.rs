use crate::boot_info::PhysicalRange;
use core::sync::atomic::{AtomicU64, Ordering};
use limine::memory_map::{Entry, EntryType};

pub const PAGE_SIZE: u64 = 4096;
pub const MAX_REGIONS: usize = 4;

#[derive(Clone, Copy)]
pub struct Region {
    pub next: u64,
    pub end: u64,
}

impl Region {
    const EMPTY: Self = Self { next: 0, end: 0 };
}

const MAX_FREE_FRAMES: usize = 4;

#[unsafe(link_section = ".data")]
#[used]
pub static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);

pub struct Frame {
    pub physical_address: u64,
    pub virtual_address: *mut u8,
}

pub struct FrameAllocator {
    pub regions: [Region; MAX_REGIONS],
    pub region_count: usize,
    pub current_region: usize,
    pub hhdm_offset: u64,
    pub free_frames: [u64; MAX_FREE_FRAMES],
    pub free_count: usize,
}

impl FrameAllocator {
    pub const fn new() -> Self {
        Self {
            regions: [Region::EMPTY; MAX_REGIONS],
            region_count: 0,
            current_region: 0,
            hhdm_offset: 0,
            free_frames: [0; MAX_FREE_FRAMES],
            free_count: 0,
        }
    }
}

/// Define o deslocamento HHDM no alocador antes de `init()` ser chamado.
#[inline(never)]
pub(crate) fn set_hhdm_offset(allocator: &mut FrameAllocator, offset: u64) {
    allocator.hhdm_offset = offset;
}

pub fn init_with_allocator(
    allocator: &mut FrameAllocator,
    entries: &[Entry],
    hhdm_offset: u64,
    reserved: &[PhysicalRange],
) -> usize {
    crate::serial_write(b"step: memory init fn enter\r\n");
    crate::serial_write(b"step: hhdm_offset=0x");
    let mut hex = [0u8; 16];
    let mut n = 0;
    let mut v = hhdm_offset;
    if v == 0 { hex[n] = b'0'; n += 1; }
    while v > 0 { hex[n] = b'0' + (v % 16) as u8; if hex[n] > b'9' { hex[n] += 7; } n += 1; v /= 16; }
    for i in (0..n).rev() { crate::serial_write(&hex[i..i+1]); }
    crate::serial_write(b"\r\n");

    allocator.hhdm_offset = hhdm_offset;
    allocator.region_count = 0;
    allocator.current_region = 0;
    allocator.free_count = 0;
    crate::serial_write(b"step: stack allocator initialized\r\n");

    crate::serial_write(b"step: before entries loop\r\n");
    for entry in entries {
        if entry.entry_type != EntryType::USABLE || allocator.region_count == MAX_REGIONS {
            continue;
        }
        let Some(end) = entry.base.checked_add(entry.length) else {
            continue;
        };
        let Some(aligned_base) = entry.base.checked_add(PAGE_SIZE - 1) else {
            continue;
        };
        let base = aligned_base & !(PAGE_SIZE - 1);
        let end = end & !(PAGE_SIZE - 1);
        if base < end {
            add_unreserved_regions(allocator, base, end, reserved);
        }
    }

    crate::serial_write(b"step: entries loop done\r\n");
    crate::serial_write(b"step: region_count=");
    let mut n = 0;
    let mut v = allocator.region_count as u64;
    if v == 0 { crate::serial_write(b"0"); }
    while v > 0 { n += 1; v /= 10; }
    let mut digits = [0u8; 16];
    let mut cursor = digits.len();
    let mut vc = allocator.region_count as u64;
    while vc > 0 { cursor -= 1; digits[cursor] = b'0' + (vc % 10) as u8; vc /= 10; }
    crate::serial_write(&digits[cursor..]);
    crate::serial_write(b"\r\n");

    allocator
        .regions
        .iter()
        .take(allocator.region_count)
        .map(|region| ((region.end - region.next) / PAGE_SIZE) as usize)
        .sum()
}

fn add_unreserved_regions(
    allocator: &mut FrameAllocator,
    mut base: u64,
    end: u64,
    reserved: &[PhysicalRange],
) {
    for range in reserved {
        let Some(reserved_end) = range.start.checked_add(range.length) else {
            continue;
        };
        let reserved_start = range.start & !(PAGE_SIZE - 1);
        let reserved_end = reserved_end.saturating_add(PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        if reserved_end <= base || reserved_start >= end {
            continue;
        }
        if reserved_start > base && allocator.region_count < MAX_REGIONS {
            allocator.regions[allocator.region_count] = Region {
                next: base,
                end: reserved_start.min(end),
            };
            allocator.region_count += 1;
        }
        base = base.max(reserved_end);
        if base >= end {
            return;
        }
    }
    if base < end && allocator.region_count < MAX_REGIONS {
        allocator.regions[allocator.region_count] = Region { next: base, end };
        allocator.region_count += 1;
    }
}

pub fn allocate_frame(allocator: &mut FrameAllocator) -> Option<Frame> {
    crate::serial_write(b"step: allocate_frame enter\r\n");
    // Reciclagem primeiro: frames devolvidos por `free_frame`.
    if allocator.free_count > 0 {
        allocator.free_count -= 1;
        let physical_address = allocator.free_frames[allocator.free_count];
        let virtual_address = physical_address.checked_add(allocator.hhdm_offset)? as *mut u8;
        return Some(Frame {
            physical_address,
            virtual_address,
        });
    }

    while allocator.current_region < allocator.region_count {
        let region = &mut allocator.regions[allocator.current_region];
        if region.next < region.end {
            crate::serial_write(b"step: allocate_frame found region\r\n");
            let physical_address = region.next;
            region.next = region.next.checked_add(PAGE_SIZE)?;
            let virtual_address = physical_address.checked_add(allocator.hhdm_offset)? as *mut u8;
            crate::serial_write(b"step: allocate_frame returning frame\r\n");
            return Some(Frame {
                physical_address,
                virtual_address,
            });
        }
        allocator.current_region += 1;
    }
    crate::serial_write(b"step: allocate_frame no frames\r\n");
    None
}

pub fn free_frame(allocator: &mut FrameAllocator, physical_address: u64) -> bool {
    if allocator.free_count >= MAX_FREE_FRAMES {
        return false;
    }
    if physical_address & (PAGE_SIZE - 1) != 0 {
        return false;
    }
    allocator.free_frames[allocator.free_count] = physical_address;
    allocator.free_count += 1;
    true
}

pub fn phys_to_virt(allocator: &FrameAllocator, physical_address: u64) -> Option<*mut u8> {
    physical_address
        .checked_add(allocator.hhdm_offset)
        .map(|virtual_address| virtual_address as *mut u8)
}

pub fn usage(allocator: &FrameAllocator) -> (u64, u64) {
    let mut free = 0u64;
    for region in allocator.regions.iter().take(allocator.region_count) {
        free += region.end.saturating_sub(region.next);
    }
    free += (allocator.free_count as u64) * PAGE_SIZE;
    let total = ALLOCATED_BYTES.load(Ordering::Relaxed) + free;
    (total, free)
}
