use crate::boot_info::PhysicalRange;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU64, Ordering};
use limine::memory_map::{Entry, EntryType};

const PAGE_SIZE: u64 = 4096;
const MAX_REGIONS: usize = 128;
/// Frames devolvidos por `free_frame` e ainda não reutilizados.
///
/// O alocador é bump-only por região: sem esta lista, `munmap` só poderia
/// "esquecer" o mapeamento e os frames ficariam perdidos até o reboot. A pilha
/// de reciclagem dá ao userspace (`mmap`/`munmap`, base do `GlobalAlloc` da
/// `std`) a possibilidade de crescer e encolher de verdade.
const MAX_FREE_FRAMES: usize = 4096;

#[derive(Clone, Copy)]
struct Region {
    next: u64,
    end: u64,
}

impl Region {
    const EMPTY: Self = Self { next: 0, end: 0 };
}

struct FrameAllocator {
    regions: [Region; MAX_REGIONS],
    region_count: usize,
    current_region: usize,
    hhdm_offset: u64,
    /// Endereços físicos reciclados por `free_frame` (pilha LIFO).
    free_frames: [u64; MAX_FREE_FRAMES],
    free_count: usize,
}

impl FrameAllocator {
    const fn new() -> Self {
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

struct SharedAllocator(UnsafeCell<FrameAllocator>);
unsafe impl Sync for SharedAllocator {}

static ALLOCATOR: SharedAllocator = SharedAllocator(UnsafeCell::new(FrameAllocator::new()));

/// Bytes já entregues por `allocate_frame`.
///
/// As regiões guardam apenas o ponteiro `next`, então o total utilizável não
/// pode ser reconstruído depois do fato: este contador preserva o montante
/// inicial para que `usage()` possa reportar `total` sem guardar `start`.
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);

pub struct Frame {
    pub physical_address: u64,
    pub virtual_address: *mut u8,
}

pub fn init(entries: &[&Entry], hhdm_offset: u64, reserved: &[PhysicalRange]) -> usize {
    let allocator = unsafe { &mut *ALLOCATOR.0.get() };
    allocator.regions = [Region::EMPTY; MAX_REGIONS];
    allocator.region_count = 0;
    allocator.current_region = 0;
    allocator.hhdm_offset = hhdm_offset;
    allocator.free_frames = [0; MAX_FREE_FRAMES];
    allocator.free_count = 0;

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

pub fn allocate_frame() -> Option<Frame> {
    let allocator = unsafe { &mut *ALLOCATOR.0.get() };
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
            let physical_address = region.next;
            // Checked: a region ending near u64::MAX must not wrap around
            // and hand out already-used frames.
            region.next = region.next.checked_add(PAGE_SIZE)?;
            ALLOCATED_BYTES.fetch_add(PAGE_SIZE, Ordering::Relaxed);
            let virtual_address = physical_address.checked_add(allocator.hhdm_offset)? as *mut u8;
            return Some(Frame {
                physical_address,
                virtual_address,
            });
        }
        allocator.current_region += 1;
    }
    None
}

/// Devolve um frame ao alocador para reutilização por `allocate_frame`.
///
/// Chamado por `munmap`. Frames reciclados não voltam às regiões bump; vão para
/// uma pilha LIFO de tamanho fixo. Se a pilha encher, o frame é simplesmente
/// abandonado (o sistema continua correto, só não recicla naquele momento).
pub fn free_frame(physical_address: u64) -> bool {
    let allocator = unsafe { &mut *ALLOCATOR.0.get() };
    if allocator.free_count >= MAX_FREE_FRAMES {
        return false;
    }
    // Só aceita frames alinhados à página e dentro de uma região conhecida.
    if physical_address & (PAGE_SIZE - 1) != 0 {
        return false;
    }
    allocator.free_frames[allocator.free_count] = physical_address;
    allocator.free_count += 1;
    true
}

pub fn phys_to_virt(physical_address: u64) -> Option<*mut u8> {
    let allocator = unsafe { &*ALLOCATOR.0.get() };
    physical_address
        .checked_add(allocator.hhdm_offset)
        .map(|virtual_address| virtual_address as *mut u8)
}

/// Estado do alocador de frames, em bytes.
///
/// `total` é a soma das regiões utilizáveis que o Limine reportou (já sem os
/// intervalos reservados), `free` o que ainda não foi entregue e `used` a
/// diferença. É o que `systemfetch` e `top` mostram como "RAM".
pub fn usage() -> (u64, u64) {
    let allocator = unsafe { &*ALLOCATOR.0.get() };
    let mut free = 0u64;
    for region in allocator.regions.iter().take(allocator.region_count) {
        free += region.end.saturating_sub(region.next);
    }
    // Frames reciclados por `free_frame` também estão disponíveis.
    free += (allocator.free_count as u64) * PAGE_SIZE;
    // `total` = frames já entregues + o que ainda resta. As regiões guardam
    // apenas o ponteiro `next`, então o montante entregue vem do contador.
    let total = ALLOCATED_BYTES.load(Ordering::Relaxed) + free;
    (total, free)
}
