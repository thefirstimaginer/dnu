#![no_std]
#![no_main]
#![allow(missing_docs)]
#![allow(unsafe_op_in_unsafe_fn)]
use core::arch::asm;
use limine::memory_map::EntryType;
use core::panic::PanicInfo;
use core::ptr;
use core::sync::atomic::{AtomicBool, Ordering};
use crate::memory::{FrameAllocator, PAGE_SIZE};

#[unsafe(link_section = ".data.early")]
#[used]
pub static mut ALLOCATOR_PTR: *mut FrameAllocator = core::ptr::null_mut();

unsafe extern "C" {
    static _ebss: u8;
    static _end: u8;
}

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

use console::framebuffer;
use console::serial as uart;
use console::utf8;
pub(crate) use console::serial::serial_write;
#[cfg(feature = "kernel-debug")]
pub(crate) use console::serial::write_number as serial_write_number;
pub(crate) use crate::halt as kernel_halt;

#[unsafe(link_section = ".note.pvh")]
#[used]
static PVH_NOTE: [u8; 16] = *b"\x04\x00\x00\x00\x00\x00\x00\x00\x03\x00\x00\x00Xen\0";

static LOG_LINE_START: AtomicBool = AtomicBool::new(true);
static EMBEDDED_FONT: &[u8] = include_bytes!("assets/zap-vga16.psf");

#[unsafe(no_mangle)]
pub extern "C" fn _start_rust() -> ! {
    boot::limine_start()
}

#[cfg(feature = "direct-boot")]
#[unsafe(no_mangle)]
pub extern "C" fn _direct_start() -> ! {
    boot::direct_start()
}

#[unsafe(no_mangle)]
pub extern "C" fn kernel_entry(boot_info: *const boot_info::BootInfo) -> ! {
    unsafe {
        asm!("cli", options(nomem, nostack));
        let rsp: u64;
        asm!("mov {}, rsp", out(reg) rsp, options(nomem, nostack, preserves_flags));
        let mut buf = [0u8; 24];
        let mut i = 0;
        let mut v = rsp;
        if v == 0 { buf[i] = b'0'; i += 1; }
        while v > 0 { buf[i] = b'0' + (v % 16) as u8; if buf[i] > b'9' { buf[i] += 7; } i += 1; v /= 16; }
        crate::serial_write(b"RSP at entry: 0x");
        for j in (0..i).rev() { crate::serial_write(&buf[j..j+1]); }
        crate::serial_write(b"\r\n");
    }
    unsafe {
        let msg: &[u8] = b"KERNEL ENTRY\r\n";
        for &byte in msg {
            asm!("out dx, al", in("dx") 0x3f8u16, in("al") byte, options(nomem, nostack));
        }
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
    crate::serial_write(b"before idt init\r\n");
    idt::init();
    crate::serial_write(b"after idt init\r\n");
    // crate::subsystems::mark_ready(crate::subsystems::Subsystem::DescriptorTables);  // pulado: causa reset com MSRs
    crate::serial_write(b"step: desc tables ready\r\n");

    let entries = unsafe { boot_info.memory_entries() };
    crate::serial_write(b"step: mmap copied\r\n");
    crate::serial_write(b"step: mmap copied\r\n");
    crate::serial_write(b"step: about to call memory::init\r\n");
    static mut DATA_TEST: u64 = 0;
    unsafe { DATA_TEST = 0xDEADBEEF; }
    crate::serial_write(b"step: data test write ok\r\n");

    let mut allocator = memory::FrameAllocator::new();
    unsafe {
        core::ptr::write(core::ptr::addr_of_mut!(crate::ALLOCATOR_PTR), &mut allocator as *mut FrameAllocator);
    }
    let frame_count = memory::init_with_allocator(&mut allocator, entries, boot_info.hhdm_offset, &[]);
    crate::serial_write(b"step: memory init done\r\n");

    subsystems::mark_ready(Subsystem::Memory);
    let Some(boot_frame) = memory::allocate_frame(&mut allocator) else {
        crate::serial_write(b"alloc: no frames\r\n");
        crate::halt();
    };
    crate::serial_write(b"alloc: got boot frame\r\n");
    subsystems::mark_ready(Subsystem::Memory);
    crate::serial_write(b"alloc: before allocator_init\r\n");
    unsafe {
        allocator::allocator_init();
    }
    crate::serial_write(b"alloc: after allocator_init\r\n");
    subsystems::mark_ready(Subsystem::Allocator);
    crate::serial_write(b"allocator ready\r\n");
    subsystems::mark_ready(Subsystem::Console);
    subsystems::mark_ready(Subsystem::Acpi);
    crate::serial_write(b"console ready, acpi ready\r\n");
    let disk_ready = probe_storage();
    mount_root_filesystems(boot_info, disk_ready);
    if let Some(settings) = ramfs::read(crate::distro::DISTRO_CONFIG_PATH) {
        distro::init_from_bytes(settings);
    }
    framebuffer::set_hhdm_offset(boot_info.hhdm_offset);
    unsafe {
        core::ptr::write(core::ptr::addr_of_mut!(crate::ALLOCATOR_PTR), &mut allocator as *mut FrameAllocator);
    }
    crate::paging::fix_kernel_page_table_entries();
    crate::serial_write(b"kernel page tables fixed\r\n");
    if subsystems::all_required_ready() {
        crate::serial_write(b"Boot: all required subsystems ready; starting userspace init\r\n");
        // Configura MSRs de syscall por último, depois de toda a infraestrutura
        // do kernel estar pronta, para evitar reset durante inicialização de
        // memória/allocator neste QEMU.
        idt::configure_syscall_msrs();
        start_init();
    } else {
        crate::serial_write(b"Boot: some required subsystems are NOT ready; halting\r\n");
        subsystems::report();
        crate::halt();
    }
}

fn report_boot_frames(frame_count: usize, boot_frame: &memory::Frame) {
    console_write("Frame allocator ready; usable frames: ");
    console_number(frame_count.saturating_sub(1) as u64);
    console_write(";\nreserved boot frame at physical 0x");
    console_hex(boot_frame.physical_address);
    console_write("\n");
}

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

fn probe_storage() -> bool {
    let Some(sectors) = drivers::block::init() else {
        console_write("No usable ATA primary-master disk; continuing without storage\n");
        subsystems::mark_ready(Subsystem::Storage);
        return false;
    };
    subsystems::mark_ready(Subsystem::Storage);
    console_write("ATA primary master ready; sectors: ");
    console_number(sectors);
    console_write("\n");
    true
}

fn mount_root_filesystems(boot_info: &boot_info::BootInfo, disk_ready: bool) {
    crate::serial_write(b"step: about to call embedded_initramfs\r\n");
    let Some(ramfs_image) = embedded_initramfs(boot_info) else {
        console_write("No initramfs available; continuing without volatile root\n");
        subsystems::mark_unavailable(Subsystem::Ramfs);
        return;
    };
    crate::serial_write(b"step: embedded_initramfs ok\r\n");
    crate::serial_write(b"step: about to call ramfs::mount\r\n");
    ramfs::mount(ramfs_image);
    crate::serial_write(b"step: ramfs::mount ok\r\n");
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
    crate::serial_write(b"step: about to call load_global_config\r\n");
    load_global_config();
    crate::serial_write(b"step: load_global_config ok\r\n");
    crate::serial_write(b"step: about to call load_font\r\n");
    load_font();
    crate::serial_write(b"step: load_font ok\r\n");
    subsystems::mark_ready(Subsystem::Configuration);
}

fn embedded_initramfs(boot_info: &boot_info::BootInfo) -> Option<&'static [u8]> {
    crate::serial_write(b"dzimage ptr=0x");
    crate::serial_write_hex(boot_info.dzimage as u64);
    crate::serial_write(b" size=");
    crate::serial_write_hex(boot_info.dzimage_size as u64);
    crate::serial_write(b"\r\n");
    if boot_info.dzimage.is_null() {
        crate::serial_write(b"dzimage is null\r\n");
        return None;
    }
    // boot_info.dzimage já é um endereço virtual acessível (mapeado pelo
    // Limine na área alta do kernel em boot.rs). Não somamos hhdm_offset:
    // isso causaria double-mapping e endereço inválido.
    let virt = boot_info.dzimage as usize;
    crate::serial_write(b"dzimage virt=0x");
    crate::serial_write_hex(virt as u64);
    crate::serial_write(b"\r\n");
    let image = unsafe {
        core::slice::from_raw_parts(virt as *const u8, boot_info.dzimage_size)
    };
    let member = dzimage::initramfs_member(image).ok()?;
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

fn start_init() -> ! {
    crate::serial_write(b"start_init: loading /sbin/init as PID 1\r\n");
    let Some(init_program) = ramfs::read("/sbin/init") else {
        crate::serial_write(b"start_init: /sbin/init not found; halting\r\n");
        crate::halt();
    };
    crate::serial_write(b"start_init: /sbin/init found; entering DEF loader\r\n");
    crate::serial_write(b"start_init: init bytes magic: ");
    crate::serial_write(&init_program[..4.min(init_program.len())]);
    crate::serial_write(b" len=");
    crate::serial_write_hex(init_program.len() as u64);
    crate::serial_write(b"\r\n");
    if process::init(&[(init_program, 1)]).is_err() {
        crate::serial_write(b"start_init: DEF loader failed; stopping safely\r\n");
        crate::halt();
    }
    subsystems::mark_ready(Subsystem::InitProcess);
    time::configure_timer_interrupt();
    subsystems::mark_ready(Subsystem::Timer);
    crate::serial_write(b"Timer configured; preemptive priority scheduling active\r\n");
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

pub(crate) fn serial_write_hex(mut value: u64) {
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

fn write_to_console(digits: &[u8]) {
    kernel_log::append(digits);
    framebuffer::write_utf8(digits);
}

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