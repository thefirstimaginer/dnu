#![no_std]
#![no_main]
#![allow(missing_docs)]
// Código de kernel: funções `unsafe fn` usam operações unsafe no corpo por
// contrato (asm de E/S, voláteis de MMIO). Edition 2024 pede blocos internos;
// mantemos o estilo legado de contrato-por-função até uma revisão completa.
#![allow(unsafe_op_in_unsafe_fn)]
use core::arch::asm;
use core::panic::PanicInfo;
use core::ptr;
use core::sync::atomic::{AtomicBool, Ordering};

// Grafo do kernel: cada `mod` declara suas folhas com #[path] relativo a ele.
// Apenas main.rs, linker.ld e linker_direct.ld permanecem na raiz de src/.
#[path = "arch/arch_hub.rs"]
mod arch_hub;
#[path = "ipc/ipc.rs"]
mod ipc;
#[path = "console/console_hub.rs"]
mod console;
#[path = "console/display_input.rs"]
mod display_input;

#[path = "drivers/drivers_hub.rs"]
pub mod drivers;
#[path = "mm/block.rs"]
mod block;
#[path = "boot/boot_info.rs"]
mod boot_info;
#[path = "boot/dzimage.rs"]
mod dzimage;
#[path = "config/distro.rs"]
mod distro;
#[path = "config/config.rs"]
mod config;
#[path = "subsystems/subsystems.rs"]
mod subsystems;
use subsystems::Subsystem;
#[path = "fs/dfs.rs"]
mod dfs;
#[path = "boot/boot.rs"]
mod boot;
#[path = "process/mod.rs"]
pub mod process;
#[path = "drivers/ata.rs"]
mod ata;
#[path = "init/init_hub.rs"]
pub mod init;
#[path = "arch/fpu.rs"]
mod fpu;
#[path = "arch/gdt.rs"]
mod gdt;
#[path = "fs/gpt.rs"]
pub mod gpt;
#[path = "mm/allocator.rs"]
mod allocator;
#[path = "arch/idt.rs"]
mod idt;
#[path = "log/kernel_log.rs"]
pub mod kernel_log;
#[path = "drivers/keyboard.rs"]
mod keyboard;
#[path = "drivers/mouse.rs"]
pub mod mouse;
#[path = "mm/memory.rs"]
mod memory;
#[path = "mm/paging.rs"]
mod paging;
#[path = "subsystems/permissions.rs"]
mod permissions;
#[path = "drivers/acpi.rs"]
mod acpi;
#[path = "fs/ramfs.rs"]
mod ramfs;
#[path = "console/surface.rs"]
mod surface;
#[path = "fs/vfs.rs"]
pub mod vfs;
#[path = "abi/lib.rs"]
mod abi;

#[path = "time/time.rs"]
mod time;

// Reexports dos submódulos de `console` que o resto do kernel referencia pelo
// caminho curto (`crate::framebuffer`, `crate::utf8`).
use console::framebuffer;
use console::serial as uart;
use console::utf8;
// Helpers legados de log/diagnóstico mantêm o caminho curto `crate::
pub(crate) use console::serial::serial_write;
#[cfg(feature = "kernel-debug")]
pub(crate) use console::serial::write_number as serial_write_number;
pub(crate) use crate::halt as kernel_halt;

// Nota ELF PVH exigida pelo QEMU para aceitar kernels x86_64 carregados com
// `-kernel`. Colocada na seção .note.pvh, que o linker_direct.ld mapeia para
// o segmento PT_NOTE.
#[unsafe(link_section = ".note.pvh")]
#[used]
static PVH_NOTE: [u8; 16] = *b"\x04\x00\x00\x00\x00\x00\x00\x00\x03\x00\x00\x00Xen\0";

static LOG_LINE_START: AtomicBool = AtomicBool::new(true);
// Fonte PSF embutida no kernel: o dzImage também a leva dentro do initramfs,
// mas o boot precisa desenhar o console antes de montar a raiz volátil.
static EMBEDDED_FONT: &[u8] = include_bytes!("assets/zap-vga16.psf");

/// Ponto de entrada ELF visto pelo Limine. O módulo `boot` coleta as respostas
/// das requisições do protocolo Limine, monta o `BootInfo` e chama `kernel_entry`.
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    boot::limine_start()
}

/// Ponto de entrada para boot direto via `dnuboot` (feature `direct-boot`).
/// Chama `boot::direct_start()`, que monta um `BootInfo` com valores padrão e
/// lê o initramfs da seção `.initramfs` do binário.
#[cfg(feature = "direct-boot")]
#[unsafe(no_mangle)]
pub extern "C" fn _direct_start() -> ! {
    boot::direct_start()
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
    // Inicializa o alocador global (GlobalAlloc) antes de qualquer uso de Box/Vec/String.
    unsafe {
        allocator::allocator_init();
    }
    subsystems::mark_ready(Subsystem::Allocator);
    framebuffer::init(
        (!boot_info.framebuffer.is_null())
            .then(|| unsafe { &*boot_info.framebuffer })
            .and_then(|response| response.framebuffers().next()),
    );
    // O servidor de display precisa do endereço físico da memória de vídeo,
    // que é obtido subtraindo o HHDM do ponteiro virtual do framebuffer.
    framebuffer::set_hhdm_offset(boot_info.hhdm_offset);
    drivers::video::register();
    if !framebuffer::load_font(EMBEDDED_FONT) {
        console_write("WARNING: embedded .psf is invalid; using fallback font\n");
    }
    mouse::init();
    // Registro central de drivers: teclado/mouse agora disponíveis.
    drivers::input::register_all();
    console_write("DNU Kernel v0.1_01\n"); // Incrementar se houver mudanças relevantes
    report_boot_frames(frame_count, &boot_frame);
    report_clock();
    subsystems::mark_ready(Subsystem::Console);
    acpi::init(boot_info.rsdp_address, boot_info.hhdm_offset);
    drivers::platform::register();
    subsystems::mark_ready(Subsystem::Acpi);

    // Referência única aos globais de I/O da ABI: mantê-los vivos mesmo quando
    // nenhuma syscall é compilada no binário (builds mínimos/testes).
    let disk_ready = probe_storage();
    mount_root_filesystems(boot_info, disk_ready);

    // Lê o nome da distribuição da configuração do RAMFS (se disponível).
    if let Some(settings) = ramfs::read(crate::distro::DISTRO_CONFIG_PATH) {
        distro::init_from_bytes(settings);
    }

    // Sistema inicia direto na GUI — não abre shell/login.
    // O sistema permanece na área de trabalho até o desligamento.
    loop {
        unsafe { asm!("hlt", options(nomem, nostack)); }
    }
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
    let Some(sectors) = drivers::block::init() else {
        console_write("No usable ATA primary-master disk; continuing without storage\n");
        return false;
    };
    console_write("ATA primary master ready; sectors: ");
    console_number(sectors);
    console_write("\n");
    true
}

/// Monta o RAMFS com o membro `initramfs` do `dzImage` e, se houver, o DFS
/// persistente por cima. Aplica também a configuração global e a fonte PSF da
/// raiz ativa. No boot direto (sem dzImage), o initramfs pode estar ausente;
/// nesse caso, o sistema continua sem raiz volátil.
fn mount_root_filesystems(boot_info: &boot_info::BootInfo, disk_ready: bool) {
    let Some(ramfs_image) = embedded_initramfs(boot_info) else {
        console_write("No initramfs available; continuing without volatile root\n");
        subsystems::mark_unavailable(Subsystem::Ramfs);
        return;
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

/// Extrai o membro `initramfs` do `dzImage` carregado pelo Limine.
fn embedded_initramfs(boot_info: &boot_info::BootInfo) -> Option<&'static [u8]> {
    if boot_info.dzimage.is_null() {
        return None;
    }
    let image = unsafe {
        core::slice::from_raw_parts(boot_info.dzimage, boot_info.dzimage_size)
    };
    let member = dzimage::initramfs_member(image).ok()?;
    // `member.payload(image)` devolve uma fatia que referencia `image`, que
    // tem vida estática (vem do ponteiro raw do Limine). Usamos unsafe para
    // afirmar a vida estática.
    Some(unsafe { &*(member.payload(image) as *const [u8]) })
}

fn load_global_config() {
    if let Some(settings) = ramfs::read(crate::distro::DISTRO_CONFIG_PATH) {
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

/// Escreve um u64 em hexadecimal na serial (sem newline).
fn serial_write_hex(mut value: u64) {
    let mut digits = [0u8; 16];
    let mut cursor = digits.len();
    if value == 0 {
        serial_write(b"0");
        return;
    }
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
    serial_write(&digits[cursor..]);
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