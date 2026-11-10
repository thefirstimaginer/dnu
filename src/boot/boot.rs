//! Colheita das respostas do protocolo Limine.
//!
//! O Limine carrega dois módulos do `dzImage`: o ELF do kernel (e salta para
//! `_start`) e o próprio arquivo `dzImage`, que a tabela de membros
//! ([`crate::dzimage`]) usa para encontrar o `initramfs`. Aqui montamos o
//! [`BootInfo`] que o resto do kernel espera.
//!
//! Além do caminho via Limine (`limine_start`), este módulo oferece
//! `direct_start`, usado pelo `dnuboot` para bootar o `dzImage` sem ISO nem
//! bootloader externo. Nesse caminho, as estruturas do `BootInfo` são montadas
//! com valores padrão e o kernel ELF + initramfs são carregados diretamente na
//! memória por um pequeno stub de boot (veja `tools/dnuboot/`).
#![allow(missing_docs)]

use core::arch::asm;
use core::mem::MaybeUninit;
use core::ptr;

use limine::BaseRevision;
use limine::memory_map::Entry;
use limine::request::{
    FramebufferRequest, HhdmRequest, MemoryMapRequest, ModuleRequest, RsdpRequest, StackSizeRequest,
};
use limine::response::{FramebufferResponse, MemoryMapResponse, ModuleResponse};

use crate::boot_info::{BootInfo, PhysicalRange, MAX_RESERVED_RANGES};

/// Armazenamento estático para as entradas do memory map do Limine.
/// Colocado em `.data` para garantir que esteja mapeado e acessível.
#[unsafe(link_section = ".data")]
static mut LIMINE_MEMORY_ENTRIES: [Entry; 256] = unsafe { core::mem::zeroed() };

unsafe extern "C" {
    static _initramfs_start: u8;
    static _initramfs_end: u8;
}

struct BootInfoStorage(core::cell::UnsafeCell<MaybeUninit<BootInfo>>);
unsafe impl Sync for BootInfoStorage {}

/// Colocado em `.data` para evitar page faults em páginas de `.bss` com bits
/// reservados nas entradas de tabela de páginas do Limine.
#[unsafe(link_section = ".data")]
static BOOT_INFO: BootInfoStorage = BootInfoStorage(core::cell::UnsafeCell::new(MaybeUninit::uninit()));

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
#[used]
#[unsafe(link_section = ".requests")]
static STACK_SIZE_REQUEST: StackSizeRequest = StackSizeRequest::new().with_size(1024 * 1024);

static EMPTY_RESERVED: [PhysicalRange; 1] = [PhysicalRange::EMPTY];

/// Nome do módulo que o `limine.conf` do dzImage passa ao Limine.
const DZIMAGE_MODULE: &[u8] = b"dzImage";

/// Mascara todas as linhas dos dois PICs 8259 herdados.
///
/// O kernel usa o timer PIT e não tem handlers para IRQ legadas; deixá-las
/// desmascaradas faria uma interrupção espúria cair em vetores sem stub.
pub fn mask_legacy_pic() {
    unsafe {
        out(0x21, 0xff);
        out(0xa1, 0xff);
    }
}

/// Retorna a resposta do framebuffer do Limine, se disponível.
pub fn framebuffer_response() -> Option<&'static limine::response::FramebufferResponse> {
    FRAMEBUFFER_REQUEST.get_response()
}

unsafe fn out(port: u16, value: u8) {
    asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
}

/// HHDM offset padrão usado no boot direto. Em endereço baixo (identity-mapped)
/// é zero, pois não há tradução de endereços.
const DIRECT_HHDM_OFFSET: u64 = 0;

/// Endereço físico onde o `dnuboot` carrega o initramfs no boot direto.
const DIRECT_INITRAMFS_ADDR: u64 = 0x0000_1000_0000_0000;

/// Tamanho máximo da região de RAM reportada no boot direto.
const DIRECT_MEMORY_SIZE: u64 = 512 * 1024 * 1024; // 512 MiB

/// Coleta as respostas do Limine e transfere o controle para `kernel_entry`.
pub fn limine_start() -> ! {
    // Inicializa a UART cedo para diagnóstico, antes de qualquer verificação que
    // possa fazer panic.
    crate::console::serial::init();
    crate::serial_write(b"boot: entering limine_start\r\n");

    if !BASE_REVISION.is_supported() {
        crate::serial_write(b"boot: BASE_REVISION not supported\r\n");
        panic!("Limine protocol revision is unsupported");
    }
    crate::serial_write(b"boot: BASE_REVISION ok\r\n");
    let memory_map = MEMORY_MAP_REQUEST
        .get_response()
        .expect("Limine did not provide a memory map");
    crate::serial_write(b"boot: memory map ok\r\n");
    let hhdm = HHDM_REQUEST
        .get_response()
        .expect("Limine did not provide an HHDM mapping");
    crate::serial_write(b"boot: HHDM ok\r\n");
    let module_response = MODULE_REQUEST
        .get_response()
        .expect("Limine did not provide modules");
    crate::serial_write(b"boot: modules ok\r\n");
    // O dzImage é o único artefato de boot: a tabela de membros dele aponta
    // para o kernel e para o initramfs. Sem ele não há raiz volátil.
    let dzimage = module_response
        .modules()
        .iter()
        .find(|module| module.path().to_bytes().ends_with(DZIMAGE_MODULE));
    let Some(dzimage) = dzimage else {
        crate::serial_write(b"boot: dzImage module NOT FOUND\r\n");
        panic!("required /boot/dzImage module is missing");
    };
    crate::serial_write(b"boot: dzImage module found\r\n");
    let dzimage_bytes = unsafe {
        core::slice::from_raw_parts(dzimage.addr(), dzimage.size() as usize)
    };
    crate::serial_write(b"boot: dzImage bytes mapped\r\n");
    if let Err(error) = crate::dzimage::verify(dzimage_bytes, crate::abi::ABI_VERSION) {
        crate::serial_write(b"boot: dzImage verify FAILED\r\n");
        panic!("dzImage failed verification: {error:?}");
    }
    crate::serial_write(b"boot: dzImage verify OK\r\n");

    // Sem bootstrap: nada além do que o próprio Limine usa
    // (incluindo a árvore de páginas de boot e o ELF carregado) fica vivo
    // depois do salto, então nenhum intervalo extra é reservado.
    let reserved: &[PhysicalRange] = &EMPTY_RESERVED;

    // Copia as entradas do memory map do Limine para armazenamento estático
    // acessível via HHDM, para não depender dos mapeamentos de página do Limine.
    let limine_entries = memory_map.entries();
    let entries_count = limine_entries.len().min(256);
    unsafe {
        for (i, entry) in limine_entries.iter().take(entries_count).enumerate() {
            LIMINE_MEMORY_ENTRIES[i] = **entry;
        }
    }
    let memory_entries_ptr = unsafe { core::ptr::addr_of!(LIMINE_MEMORY_ENTRIES) as *const Entry };

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
        memory_entries: memory_entries_ptr,
        memory_entries_count: entries_count,
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
        dzimage: dzimage_bytes.as_ptr(),
        dzimage_size: dzimage_bytes.len(),
    };
    let boot_info_pointer = unsafe { (*BOOT_INFO.0.get()).write(boot_info) as *const BootInfo };
    crate::kernel_entry(boot_info_pointer)
}

/// Inicialização de boot direto (sem Limine).
///
/// Usado pelo `dnuboot`: constrói um [`BootInfo`] com valores padrão e chama
/// [`crate::kernel_entry`]. O initramfs é lido dos símbolos
/// `_binary_initramfs_tar_start` / `_binary_initramfs_tar_end` (definidos pelo
/// `dnuboot` via `objcopy -I binary` e vinculados ao binário de boot).
pub fn direct_start() -> ! {
    crate::console::serial::init();
    crate::serial_write(b"boot: entering direct_start\r\n");

    let initramfs_start = unsafe { &_initramfs_start as *const u8 };
    let initramfs_end = unsafe { &_initramfs_end as *const u8 };
    let initramfs_size = initramfs_end as usize - initramfs_start as usize;

    let reserved: &[PhysicalRange] = &EMPTY_RESERVED;

    let tsc_start = crate::time::read_tsc();
    let tsc_frequency = crate::time::cpuid_tsc_frequency()
        .or_else(crate::time::calibrate_tsc)
        .unwrap_or(0);

    // Monta um ModuleResponse mínimo apontando para o initramfs. O kernel
    // procura pelo módulo chamado "dzImage" e lê seus bytes como o contêiner
    // dzImage, do qual extrai o initramfs. No boot direto, o próprio initramfs
    // (tar USTAR) é colocado como payload de um módulo sintético.
    let mut module_response = DirectModuleResponse {
        modules: [DirectModule {
            path: [0; 128],
            pad: [0; 32],
            addr: initramfs_start as u64,
            size: initramfs_size as u64,
        }],
        count: 1,
    };

    // Copiar "dzImage" para o path do módulo
    let path_bytes = b"dzImage";
    let module_path = unsafe { &mut module_response.modules[0].path as *mut [u8; 128] };
    unsafe {
        let len = path_bytes.len().min(128);
        core::ptr::copy_nonoverlapping(path_bytes.as_ptr(), (*module_path).as_mut_ptr(), len);
    }

    let boot_info = BootInfo {
        memory_entries: ptr::null(),
        memory_entries_count: 0,
        framebuffer: ptr::null(),
        modules: &module_response as *const DirectModuleResponse as *const ModuleResponse,
        rsdp_address: 0,
        hhdm_offset: DIRECT_HHDM_OFFSET,
        tsc_start,
        tsc_frequency,
        reserved_ranges: reserved.as_ptr(),
        reserved_count: reserved.len(),
        dzimage: initramfs_start,
        dzimage_size: initramfs_size,
    };
    let boot_info_pointer = unsafe { (*BOOT_INFO.0.get()).write(boot_info) as *const BootInfo };
    crate::kernel_entry(boot_info_pointer)
}

/// Módulo sintético para o boot direto. Espelha os campos de `limine::response::Module`.
/// Resposta de módulos sintética para o boot direto.
#[repr(C)]
#[allow(missing_docs)]
struct DirectModule {
    path: [u8; 128],
    pad: [u8; 32],
    addr: u64,
    size: u64,
}

/// Resposta de módulos sintética para o boot direto.
#[repr(C)]
#[allow(missing_docs)]
struct DirectModuleResponse {
    modules: [DirectModule; 1],
    count: usize,
}

// Mantém o tamanho máximo referenciado (o array de reserva é fixo).
#[allow(dead_code)]
const _: () = assert!(MAX_RESERVED_RANGES >= 1);
