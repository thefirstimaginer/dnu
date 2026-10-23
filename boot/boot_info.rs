use limine::{
    memory_map::Entry,
    response::{FramebufferResponse, MemoryMapResponse, ModuleResponse},
};

#[allow(dead_code)]
pub const MAX_RESERVED_RANGES: usize = 1024;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PhysicalRange {
    pub start: u64,
    pub length: u64,
}

impl PhysicalRange {
    pub const EMPTY: Self = Self {
        start: 0,
        length: 0,
    };
}

#[repr(C)]
pub struct BootInfo {
    pub memory_map: *const MemoryMapResponse,
    pub framebuffer: *const FramebufferResponse,
    pub modules: *const ModuleResponse,
    pub rsdp_address: u64,
    pub hhdm_offset: u64,
    pub tsc_start: u64,
    pub tsc_frequency: u64,
    pub reserved_ranges: *const PhysicalRange,
    pub reserved_count: usize,
}

impl BootInfo {
    #[allow(dead_code)]
    pub unsafe fn memory_entries(&self) -> &[&'static Entry] {
        // SAFETY: o memory map foi validado pelo Limine no boot.
        unsafe { (&*self.memory_map).entries() }
    }
    #[allow(dead_code)]
    pub unsafe fn reserved(&self) -> &[PhysicalRange] {
        // SAFETY: `reserved_ranges`/`reserved_count` valem `0`/nulo quando o
        // Limine carrega o kernel diretamente (nenhum intervalo extra reservado).
        unsafe { core::slice::from_raw_parts(self.reserved_ranges, self.reserved_count) }
    }
}
