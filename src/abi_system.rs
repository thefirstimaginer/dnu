//! Syscalls de sistema: identidade, tempo, console, `dmesg`, disco,
//! energia e instalação.
//!
//! São chamadas administrativas: a maioria exige UID 0, PID 1 ou o privilégio
//! de instalação concedido no boot (`/bin/distroinstall`).

use imagineos_abi::{InstallConfig, MAX_CONSOLE_WRITE, MAX_IO_SIZE, SYS_INSTALL_DISK};

use super::trap::{TrapFrame, EFAULT, EINVAL, ENODEV, ENOENT, EPERM};
use super::user::{copy_struct_from_user, copy_struct_to_user};
use crate::{installer, process};

/// `getidentity(&UserIdentity)`.
pub fn identity(frame: &mut TrapFrame) -> *mut TrapFrame {
    let identity = process::current_identity();
    if copy_struct_to_user(frame.rdi, &identity) {
        frame.ok(0)
    } else {
        frame.fail(EFAULT)
    }
}

/// `system_info(&SystemInfo)`: retrato do sistema em uma única chamada.
///
/// O kernel é quem tem acesso ao TSC, ao alocador de frames, ao framebuffer e ao
/// escalonador; juntar tudo aqui garante que os números sejam coerentes entre si
/// (o userspace não pode medir uptime nem RAM por conta própria).
pub fn system_info(frame: &mut TrapFrame) -> *mut TrapFrame {
    let (memory_total, memory_free) = crate::memory::usage();
    let (framebuffer_width, framebuffer_height) = crate::framebuffer::description()
        .map_or((0, 0), |description| {
            (description.width as u32, description.height as u32)
        });
    let (_, _, process_count) = process::snapshot_with_count();
    let info = imagineos_abi::SystemInfo {
        uptime_microseconds: crate::time::elapsed_microseconds().unwrap_or(0),
        tsc_frequency: crate::time::tsc_frequency(),
        memory_total,
        memory_free,
        framebuffer_width,
        framebuffer_height,
        process_count: process_count as u32,
        priority: process::current_priority() as u32,
        pid: process::current_pid() as u64,
        abi_version: imagineos_abi::ABI_VERSION,
    };
    if copy_struct_to_user(frame.rdi, &info) {
        frame.ok(0)
    } else {
        frame.fail(EFAULT)
    }
}

/// `write(fd, buf, len)` sem descritor: escreve direto no console.
pub fn console_write(frame: &mut TrapFrame) -> *mut TrapFrame {
    let length = (frame.rsi as usize).min(MAX_CONSOLE_WRITE);
    let mut buffer = [0u8; MAX_CONSOLE_WRITE];
    if !process::copy_from_current_user(frame.rdi, &mut buffer[..length]) {
        return frame.fail(EFAULT);
    }
    crate::console_write_bytes(&buffer[..length]);
    frame.ok(length as u64)
}

/// `gettimeofday(&{seconds, microseconds})`.
pub fn gettimeofday(frame: &mut TrapFrame) -> *mut TrapFrame {
    #[repr(C)]
    struct UserTimeval {
        seconds: i64,
        microseconds: i64,
    }
    let Some(microseconds) = crate::time::elapsed_microseconds() else {
        return frame.fail(-5); // EIO
    };
    let value = UserTimeval {
        seconds: (microseconds / 1_000_000) as i64,
        microseconds: (microseconds % 1_000_000) as i64,
    };
    if copy_struct_to_user(frame.rdi, &value) {
        frame.ok(0)
    } else {
        frame.fail(EFAULT)
    }
}

/// `dmesg(buf, capacity)`: copia o log circular do kernel.
pub fn dmesg(frame: &mut TrapFrame) -> *mut TrapFrame {
    let capacity = frame.rsi as usize;
    if capacity > MAX_IO_SIZE {
        return frame.fail(EINVAL);
    }
    let mut buffer = [0u8; MAX_IO_SIZE];
    let length = crate::kernel_log::read(&mut buffer[..capacity]);
    if process::copy_to_current_user(frame.rdi, &buffer[..length]) {
        frame.ok(length as u64)
    } else {
        frame.fail(EFAULT)
    }
}

/// `disk_sectors(disk)`: número de setores do disco primário.
pub fn disk_sectors(frame: &mut TrapFrame) -> *mut TrapFrame {
    if frame.rdi == 0 && crate::ata::is_ready() {
        frame.ok(crate::ata::sector_count())
    } else {
        frame.fail(ENODEV)
    }
}

/// `install_disk(disk)` e `install_disk_config(disk, &InstallConfig)`.
pub fn install_disk(frame: &mut TrapFrame) -> *mut TrapFrame {
    let (uid, _, _) = process::current_credentials();
    if !process::can_install_to_disk() || uid != 0 {
        return frame.fail(EPERM);
    }
    if frame.rdi != 0 {
        return frame.fail(ENODEV);
    }
    let mut config = InstallConfig::default();
    let with_config = frame.rax == SYS_INSTALL_DISK;
    if with_config && !copy_struct_from_user(frame.rsi, &mut config) {
        return frame.fail(EFAULT);
    }
    if !with_config || crate::usr::accounts::validate_install_config(&mut config).is_err() {
        config.password.fill(0);
        return frame.fail(EINVAL);
    }
    let result = installer::install_primary_master(&config);
    config.password.fill(0);
    match result {
        Ok(()) => frame.ok(0),
        Err(installer::InstallError::NoDisk) => frame.fail(ENODEV),
        Err(installer::InstallError::DiskTooSmall) => frame.fail(-28),
        Err(installer::InstallError::MissingPayload) => frame.fail(ENOENT),
        Err(installer::InstallError::PayloadTooLarge) => frame.fail(-28),
        Err(installer::InstallError::DfsFormat) => frame.fail(-5),
        Err(installer::InstallError::InvalidAccountConfig) => frame.fail(EINVAL),
        Err(installer::InstallError::EntropyUnavailable) => frame.fail(-95),
        Err(installer::InstallError::Block(_)) => frame.fail(-5),
    }
}

/// `shutdown_request()`: só root pode pedir desligamento.
pub fn request_shutdown(frame: &mut TrapFrame) -> *mut TrapFrame {
    if process::current_credentials().0 != 0 {
        return frame.fail(EPERM);
    }
    crate::acpi::request_shutdown();
    frame.ok(0)
}

/// `shutdown_poll()`: o supervisor (PID 1) consulta o pedido pendente.
pub fn poll_shutdown(frame: &mut TrapFrame) -> *mut TrapFrame {
    if process::current_pid() != 1 {
        return frame.fail(EPERM);
    }
    frame.ok(u64::from(crate::acpi::take_shutdown_request()))
}

/// `power_off()`: desliga a máquina (somente PID 1).
pub fn power_off(frame: &mut TrapFrame) -> *mut TrapFrame {
    if process::current_pid() != 1 {
        return frame.fail(EPERM);
    }
    match crate::acpi::power_off() {
        Ok(()) => unreachable!("ACPI poweroff returned"),
        Err(_) => {
            crate::console_write("ERROR: storage sync or ACPI poweroff failed; shutdown aborted\n");
            frame.fail(-5) // EIO
        }
    }
}
