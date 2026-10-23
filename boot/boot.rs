//! Colheita das respostas do protocolo Limine.
//!
//! Com a remoção do bootstrap de segunda fase, o Limine carrega este ELF
//! (`dzImage` na verdade é o kernel ELF) diretamente na RAM e salta para
//! `_start`. Aqui montamos o `BootInfo` que o resto do kernel espera.

use core::arch::asm;
use core::mem::MaybeUninit;
use core::ptr;

use limine::request::{
    FramebufferRequest, HhdmRequest, MemoryMapRequest, ModuleRequest, RsdpRequest,
};
use limine::response::{FramebufferResponse, MemoryMapResponse, ModuleResponse};
use limine::BaseRevision;

use super::boot_info::{BootInfo, PhysicalRange, MAX_RESERVED_RANGES};

#[used]
#[unsafe(link_section = ".requests")]
static BASE_REVISION: BaseRevision = BaseRevision::new();
#[used]
#[unsafe(link_section = ".requests")]
static MEMORY_MAP_REQUEST: MemoryMapRequest = MemoryMapRequest::new();
#[used]
#[unsafe(link_section = ".requests")]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();
#[used]
#[unsafe(link_section = ".requests")]
static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();
#[used]
#[unsafe(link_section = ".requests")]
static MODULE_REQUEST: ModuleRequest = ModuleRequest::new();
#[used]
#[unsafe(link_section = ".requests")]
static RSDP_REQUEST: RsdpRequest = RsdpRequest::new();

static EMPTY_RESERVED: [PhysicalRange; 1] = [PhysicalRange::EMPTY];

struct BootInfoStorage(core::cell::UnsafeCell<MaybeUninit<BootInfo>>);
unsafe impl Sync for BootInfoStorage {}
static BOOT_INFO: BootInfoStorage = BootInfoStorage(core::cell::UnsafeCell::new(MaybeUninit::uninit()));

/// Mascara todas as linhas dos dois PICs 8259 herdados.
///
/// O kernel usa APIC/timer e não tem handlers para IRQ legadas; deixá-las
/// desmascaradas faria uma interrupção espúria cair em vetores sem stub.
pub fn mask_legacy_pic() {
    unsafe {
        out(0x21, 0xff);
        out(0xa1, 0xff);
    }
}

unsafe fn out(port: u16, value: u8) {
    asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
}

/// Coleta as respostas do Limine e transfere o controle para `kernel_entry`.
pub fn limine_start() -> ! {
    if !BASE_REVISION.is_supported() {
        panic!("Limine protocol revision is unsupported");
    }
    let memory_map = MEMORY_MAP_REQUEST
        .get_response()
        .expect("Limine did not provide a memory map");
    let hhdm = HHDM_REQUEST
        .get_response()
        .expect("Limine did not provide an HHDM mapping");
    let module_response = MODULE_REQUEST
        .get_response()
        .expect("Limine did not provide modules");
    if module_response
        .modules()
        .iter()
        .all(|module| !module.path().to_bytes().ends_with(b"initramfs.img"))
    {
        panic!("required /boot/initramfs.img module is missing");
    }

    // Sem bootstrap: nada além do que o próprio Limine usa
    // (incluindo a árvore de páginas de boot e o ELF carregado) fica vivo
    // depois do salto, então nenhum intervalo extra é reservado.
    let reserved: &[PhysicalRange] = &EMPTY_RESERVED;

    let tsc_start = crate::time::read_tsc();
    // Frequência do TSC, em três tentativas, da mais barata para a mais lenta:
    //   1. `cpuid` (folhas 0x15/0x16) — exato quando o firmware informa;
    //   2. calibração contra o PIT (canal 2) — sempre funciona em x86 com PIT;
    //   3. zero — o kernel cai para `[time?]` nos logs em vez de mentir.
    //
    // Sem isto o relógio relativo ficava indisponível em máquinas cujo `cpuid`
    // não expõe a frequência (a maioria dos QEMU/OVMF), e todo log saía com
    // `[time?]` em vez do tempo desde o boot.
    let tsc_frequency = crate::time::cpuid_tsc_frequency()
        .or_else(crate::time::calibrate_tsc)
        .unwrap_or(0);

    let boot_info = BootInfo {
        memory_map: memory_map as *const MemoryMapResponse,
        framebuffer: FRAMEBUFFER_REQUEST
            .get_response()
            .map_or(ptr::null(), |response| response as *const FramebufferResponse),
        modules: module_response as *const ModuleResponse,
        rsdp_address: RSDP_REQUEST
            .get_response()
            .map_or(0, |response| response.address() as u64),
        hhdm_offset: hhdm.offset(),
        tsc_start,
        tsc_frequency,
        reserved_ranges: reserved.as_ptr(),
        reserved_count: reserved.len(),
    };
    let boot_info_pointer = unsafe { (*BOOT_INFO.0.get()).write(boot_info) as *const BootInfo };
    crate::kernel_entry(boot_info_pointer)
}

// Mantém o tamanho máximo referenciado (o array de reserva é fixo).
#[allow(dead_code)]
const _: () = assert!(MAX_RESERVED_RANGES >= 1);
