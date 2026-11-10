use limine::{
    memory_map::Entry,
    response::{FramebufferResponse, ModuleResponse},
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
    /// Ponteiro para array de entradas de mapa de memória (tipo Limine Entry).
    /// Estas entradas são copiadas da resposta do Limine em boot.rs antes de
    /// chamar kernel_entry, para evitar depender dos mapeamentos de página do Limine.
    pub memory_entries: *const Entry,
    pub memory_entries_count: usize,
    pub framebuffer: *const FramebufferResponse,
    pub modules: *const ModuleResponse,
    pub rsdp_address: u64,
    pub hhdm_offset: u64,
    pub tsc_start: u64,
    pub tsc_frequency: u64,
    pub reserved_ranges: *const PhysicalRange,
    pub reserved_count: usize,
    /// Módulo `dzImage` carregado pelo Limine: a imagem inteira, da qual o
    /// `initramfs` é extraído (o kernel é o primeiro membro dela).
    pub dzimage: *const u8,
    pub dzimage_size: usize,
}

impl BootInfo {
    #[allow(dead_code)]
    pub unsafe fn memory_entries(&self) -> &[Entry] {
        // As entradas são valores Entry em memória acessível via HHDM
        // (copiadas em boot.rs). Retornamos o slice diretamente.
        unsafe { core::slice::from_raw_parts(self.memory_entries, self.memory_entries_count) }
    }
    #[allow(dead_code)]
    pub unsafe fn reserved(&self) -> &[PhysicalRange] {
        unsafe { core::slice::from_raw_parts(self.reserved_ranges, self.reserved_count) }
    }
}