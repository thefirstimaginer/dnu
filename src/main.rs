#![no_std]
#![no_main]
// Código de kernel: funções `unsafe fn` usam operações unsafe no corpo por
// contrato (asm de E/S, voláteis de MMIO). Edition 2024 pede blocos internos;
// mantemos o estilo legado de contrato-por-função até uma revisão completa.
#![allow(unsafe_op_in_unsafe_fn)]
use core::arch::asm;
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicBool, Ordering};

mod usr;
#[path = "arch/mod.rs"]
mod arch;
#[path = "ipc/ipc.rs"]
mod ipc;
#[path = "io/input.rs"]
mod input;
#[path = "drivers/ata.rs"]
pub mod ata;
#[path = "drivers/block.rs"]
pub mod block;
#[path = "boot/boot.rs"]
mod boot;
#[path = "boot/boot_info.rs"]
mod boot_info;
#[path = "boot/subsystems.rs"]
mod subsystems;
use subsystems::Subsystem;
use usr::config;
#[path = "fs/dfs/dfs.rs"]
mod dfs;
#[path = "exec/mod.rs"]
pub mod exec;
pub use exec::process;
#[path = "init/mod.rs"]
pub mod init;
#[path = "console/mod.rs"]
mod console;
#[path = "arch/x86_64/fpu.rs"]
mod fpu;
#[path = "arch/x86_64/gdt.rs"]
mod gdt;
#[path = "fs/gpt.rs"]
pub mod gpt;
#[path = "mm/heap.rs"]
mod heap;
#[path = "arch/x86_64/idt.rs"]
mod idt;
#[path = "fs/installer.rs"]
mod installer;
#[path = "console/kernel_log.rs"]
pub mod kernel_log;
#[path = "drivers/keyboard.rs"]
mod keyboard;
#[path = "drivers/mouse.rs"]
pub mod mouse;
#[path = "mm/memory.rs"]
pub mod memory;
#[path = "arch/x86_64/paging.rs"]
mod paging;
#[path = "fs/permissions.rs"]
mod permissions;
#[path = "drivers/acpi.rs"]
mod acpi;
#[path = "fs/ramfs.rs"]
mod ramfs;
#[path = "dev/display/surface.rs"]
mod surface;
#[path = "fs/vfs.rs"]
pub mod vfs;
#[path = "abi/mod.rs"]
mod abi;
#[path = "drivers/time.rs"]
mod time;

// Reexports dos submódulos de `console` que o resto do kernel referencia pelo
// caminho curto (`crate::framebuffer`, `crate::utf8`).
use console::framebuffer;
use console::serial as uart;
use console::utf8;
// Helpers legados de log/diagnóstico mantêm o caminho curto `crate::
// serial_write`, `crate::serial_write_number` e `crate::kernel_halt`.
pub(crate) use console::serial::serial_write;
#[cfg(feature = "kernel-debug")]
pub(crate) use console::serial::write_number as serial_write_number;
pub(crate) use crate::halt as kernel_halt;

static LOG_LINE_START: AtomicBool = AtomicBool::new(true);
static EMBEDDED_FONT: &[u8] = include_bytes!("../../common/tools/assets/zap-vga16.psf");

/// Ponto de entrada ELF visto pelo Limine. O módulo `boot` coleta as respostas
/// das requisições do protocolo Limine, monta o `BootInfo` e chama `kernel_entry`.
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    boot::limine_start()
}

#[unsafe(no_mangle)]
pub extern "C" fn kernel_entry(boot_info: *const boot_info::BootInfo) -> ! {
    unsafe {
        asm!("cli", options(nomem, nostack));
    }
    fpu::init();

    uart::init();
    if boot_info.is_null() {
        uart::write(b"KERNEL PANIC: Limine supplied no boot information\r\n");
        halt();
    }
    let boot_info = unsafe { &*boot_info };
    time::init_with(boot_info.tsc_start, boot_info.tsc_frequency);
    boot::mask_legacy_pic();
    gdt::init();
    idt::init();
    subsystems::mark_ready(Subsystem::DescriptorTables);

    let frame_count = memory::init(
        unsafe { boot_info.memory_entries() },
        boot_info.hhdm_offset,
        unsafe { boot_info.reserved() },
    );
    let Some(boot_frame) = memory::allocate_frame() else {
        console_write("No usable physical frames\n");
        crate::halt();
    };
    unsafe {
        core::ptr::write_bytes(boot_frame.virtual_address, 0, 4096);
    }
    subsystems::mark_ready(Subsystem::Memory);
    framebuffer::init(
        (!boot_info.framebuffer.is_null())
            .then(|| unsafe { &*boot_info.framebuffer })
            .and_then(|response| response.framebuffers().next()),
    );
    // O servidor de display precisa do endereço físico da memória de vídeo,
    // que é obtido subtraindo o HHDM do ponteiro virtual do framebuffer.
    framebuffer::set_hhdm_offset(boot_info.hhdm_offset);
    if !framebuffer::load_font(EMBEDDED_FONT) {
        kernel_panic("embedded .psf is invalid!\n");
    }
    mouse::init();
    console_write("ImagineOS Astrid w/ Dreamcore Kernel\n");
    report_boot_frames(frame_count, &boot_frame);
    report_clock();
    subsystems::mark_ready(Subsystem::Console);
    acpi::init(boot_info.rsdp_address, boot_info.hhdm_offset);
    subsystems::mark_ready(Subsystem::Acpi);

    // Referência única aos globais de I/O da ABI: mantê-los vivos mesmo quando
    // nenhuma syscall é compilada no binário (builds mínimos/testes).
    abi::anchor_runtime_state();
    let disk_ready = probe_storage();
    mount_root_filesystems(boot_info, disk_ready);
    start_init()
}

/// Anuncia o estado do alocador depois que o framebuffer está pronto.
fn report_boot_frames(frame_count: usize, boot_frame: &memory::Frame) {
    console_write("Frame allocator ready; usable frames: ");
    console_number(frame_count.saturating_sub(1) as u64);
    console_write(";\nreserved boot frame at physical 0x");
    console_hex(boot_frame.physical_address);
    console_write("\n");
}

/// Anuncia o relógio relativo de alta precisão (TSC) usado nos logs.
fn report_clock() {
    let frequency = time::tsc_frequency();
    if frequency == 0 {
        console_write("Clock: TSC frequency unavailable; logs will show [time?]\n");
        return;
    }
    console_write("Clock: TSC ");
    console_number(frequency / 1_000_000);
    console_write(".");
    console_number(frequency % 1_000_000);
    console_write(" MHz; log timestamps are microseconds since boot\n");
}

/// Inicializa o disco ATA primário e relata o estado da tabela GPT.
///
/// Devolve `true` quando há um disco utilizável — o RAMFS usa isso para expor
/// `/dev/hda`.
fn probe_storage() -> bool {
    let sectors = match ata::init() {
        Ok(sectors) => sectors,
        Err(_) => {
            console_write("No usable ATA primary-master disk; continuing without storage\n");
            return false;
        }
    };
    console_write("ATA primary master ready; sectors: ");
    console_number(sectors);
    console_write("\n");
    report_dfs_partition();
    subsystems::mark_ready(Subsystem::Storage);
    true
}

fn report_dfs_partition() {
    let table = match gpt::Gpt::read_primary(&ata::PrimaryMaster) {
        Ok(table) => table,
        Err(gpt::GptError::InvalidSignature) => {
            console_write("ATA disk has no primary GPT\n");
            return;
        }
        Err(_) => {
            console_write("ATA disk GPT is invalid or unreadable\n");
            return;
        }
    };
    match table.find_partition(&ata::PrimaryMaster, &gpt::DFS_PARTITION_TYPE_GUID) {
        Ok(partition) => {
            console_write("GPT DFS partition found at LBA ");
            console_number(partition.first_lba);
            console_write("\n");
        }
        Err(gpt::GptError::PartitionNotFound) => {
            console_write("GPT detected; no DFS partition found\n");
        }
        Err(_) => console_write("GPT DFS partition lookup failed\n"),
    }
}

/// Monta o RAMFS do `initramfs.img` e, se houver, o DFS persistente por cima.
/// Aplica também a configuração global e a fonte PSF da raiz ativa.
fn mount_root_filesystems(boot_info: &boot_info::BootInfo, disk_ready: bool) {
    let Some(ramfs_image) = embedded_initramfs(boot_info) else {
        console_write("initramfs.img missing; stopping safely\n");
        crate::halt();
    };
    ramfs::mount(ramfs_image);
    ramfs::set_block_disk_present(disk_ready);
    subsystems::mark_ready(Subsystem::Ramfs);
    match dfs::mount_primary() {
        Ok(Some(_)) => {
            console_write("DFS persistent root mounted successfully\n");
            subsystems::mark_ready(Subsystem::Dfs);
        }
        Ok(None) => {
            console_write("No DFS partition; continuing with RAMFS root\n");
            subsystems::mark_unavailable(Subsystem::Dfs);
        }
        Err(_) => {
            console_write("DFS mount or journal recovery failed; using RAMFS root\n");
            subsystems::mark_unavailable(Subsystem::Dfs);
        }
    }
    load_global_config();
    load_font();
    subsystems::mark_ready(Subsystem::Configuration);
}

/// Localiza o módulo `initramfs.img` carregado pelo Limine.
fn embedded_initramfs(boot_info: &boot_info::BootInfo) -> Option<&'static [u8]> {
    if boot_info.modules.is_null() {
        return None;
    }
    let response = unsafe { &*boot_info.modules };
    response
        .modules()
        .iter()
        .find(|module| module.path().to_bytes().ends_with(b"initramfs.img"))
        .map(|module| unsafe {
            core::slice::from_raw_parts(module.addr(), module.size() as usize)
        })
}

fn load_global_config() {
    if let Some(settings) = ramfs::read(config::GLOBAL_CONFIG_PATH) {
        if config::load(settings).is_err() {
            console_write("Global configuration invalid; using defaults\n");
        }
    } else {
        console_write("Global configuration missing; using defaults\n");
    }
}

fn load_font() {
    if let Some(font) = ramfs::find_font() {
        if !framebuffer::load_font(font) {
            console_write("RAMFS PSF invalid; retaining embedded zap-vga32.psf\n");
        }
    } else {
        console_write("RAMFS PSF not found; retaining embedded zap-vga32.psf\n");
    }
}

/// Carrega `/sbin/init` como PID 1 e entra em ring 3.
fn start_init() -> ! {
    let Some(init_program) = ramfs::read("/sbin/init") else {
        kernel_panic("required /sbin/init not found in the active root filesystem\n");
    };
    console_write("Loading /sbin/init as PID 1 in ring 3\n");
    if process::init(&[(init_program, 1)]).is_err() {
        console_write("DEF loader failed; stopping safely\n");
        crate::halt();
    }
    subsystems::mark_ready(Subsystem::InitProcess);
    time::configure_timer_interrupt();
    subsystems::mark_ready(Subsystem::Timer);
    console_write("Timer configured; preemptive priority scheduling active\n");
    subsystems::report();
    // Interrupts stay disabled here on purpose. `process::start` enters ring 3
    // through an iretq whose saved RFLAGS has IF=1, so the timer can only fire
    // once a valid user trap frame is on the stack.
    process::start()
}

pub(crate) fn console_write(text: &str) {
    let mut remaining = text.as_bytes();
    while !remaining.is_empty() {
        if LOG_LINE_START.swap(false, Ordering::Relaxed) {
            let mut timestamp = [0; 32];
            let timestamp = time::format_elapsed(&mut timestamp);
            kernel_log::append(timestamp);
            framebuffer::write_utf8(timestamp);
        }
        let length = remaining
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(remaining.len(), |newline| newline + 1);
        let line = &remaining[..length];
        // `append` já espelha os bytes na serial.
        kernel_log::append(line);
        framebuffer::write_utf8(line);
        if line.last() == Some(&b'\n') {
            LOG_LINE_START.store(true, Ordering::Relaxed);
        }
        remaining = &remaining[length..];
    }
}

pub(crate) fn console_write_bytes(bytes: &[u8]) {
    uart::write(bytes);
    framebuffer::write_ansi(bytes);
}

pub(crate) fn serial_log_timestamp() {
    let mut timestamp = [0; 32];
    uart::write(time::format_elapsed(&mut timestamp));
}

fn console_number(mut value: u64) {
    let mut digits = [0u8; 20];
    let mut cursor = digits.len();
    if value == 0 {
        uart::write(b"0");
        framebuffer::write_char('0');
        return;
    }
    while value != 0 {
        cursor -= 1;
        digits[cursor] = b'0' + (value % 10) as u8;
        value /= 10;
    }
    write_to_console(&digits[cursor..]);
}

fn console_hex(mut value: u64) {
    let mut digits = [0u8; 16];
    let mut cursor = digits.len();
    while value != 0 {
        cursor -= 1;
        let digit = (value & 0xf) as u8;
        digits[cursor] = if digit < 10 {
            b'0' + digit
        } else {
            b'a' + digit - 10
        };
        value >>= 4;
    }
    if cursor == digits.len() {
        cursor -= 1;
        digits[cursor] = b'0';
    }
    write_to_console(&digits[cursor..]);
}

#[cfg(feature = "kernel-debug")]
pub(crate) fn console_write_number(value: u64) {
    console_number(value);
}

/// Espelha dígitos ASCII (decimais/hex) no log, na serial e no framebuffer.
fn write_to_console(digits: &[u8]) {
    // `append` já espelha na serial.
    kernel_log::append(digits);
    framebuffer::write_utf8(digits);
}

/// Para a CPU para sempre com interrupções desabilitadas.
///
/// Público dentro da crate porque o escalonador e o boot o usam quando não há
/// mais nada a executar.
pub(crate) fn halt() -> ! {
    loop {
        unsafe {
            asm!("hlt", options(nomem, nostack));
        }
    }
}

fn kernel_panic(message: &str) -> ! {
    console_write("KERNEL PANIC: ");
    console_write(message);
    halt()
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    kernel_panic("unrecoverable kernel error\n")
}
