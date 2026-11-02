//! Ponto de entrada das syscalls: preserva FPU, roteia e entrega sinais.
//!
//! Cada número de syscall tem exatamente um handler, agrupado por área. O
//! `match` abaixo é a única tabela de roteamento — os handlers não se chamam
//! entre si, então um teste do crate pode cobrir uma área sem tocar nas outras.

use imagineos_abi::{
    SYS_ABI_VERSION, SYS_AUTHENTICATE, SYS_CLEAR, SYS_CLOCK_GETTIME, SYS_CLOSE, SYS_CONSOLE_RELEASE,
    SYS_DISK_COUNT, SYS_DISK_SECTORS, SYS_DMESG, SYS_DUP, SYS_DUP2, SYS_EXEC, SYS_EXECVE, SYS_EXIT,
    SYS_FB_INFO, SYS_FB_MAP, SYS_FORK, SYS_FSTAT, SYS_FUTEX, SYS_GETIDENTITY, SYS_GETPID,
    SYS_GETTIMEOFDAY, SYS_GETTID, SYS_INPUT_POLL, SYS_INSTALL_DISK, SYS_INSTALL_DISK_CONFIG,
    SYS_ISDIR, SYS_ISFILE, SYS_KILL, SYS_LSEEK, SYS_MKDIR, SYS_MMAP, SYS_MUNMAP, SYS_NANOSLEEP,
    SYS_OPEN, SYS_PIPE, SYS_PIVOT_ROOT, SYS_POINTER_POSITION, SYS_POWER_OFF, SYS_PROCESS_LIST,
    SYS_READ, SYS_READ_FD, SYS_READ_FILE, SYS_READDIR, SYS_REMOVE, SYS_SBRK, SYS_SHUTDOWN_POLL,
    SYS_SHUTDOWN_REQUEST, SYS_SIGACTION, SYS_SIGRETURN, SYS_SPAWN, SYS_STAT, SYS_SURFACE_CREATE,
    SYS_SURFACE_DESTROY, SYS_SURFACE_FLUSH, SYS_SURFACE_MOVE, SYS_SURFACE_POLL, SYS_SURFACE_PRESENT,
    SYS_SURFACE_SEND_EVENT, SYS_SURFACE_SNAPSHOT, SYS_SYSTEM_INFO, SYS_SET_TLS,
    SYS_THREAD_CREATE, SYS_THREAD_EXIT, SYS_TLS_INFO, SYS_TOUCH, SYS_WAITPID, SYS_WRITE,
    SYS_WRITE_FD, SYS_WRITE_FILE, SYS_YIELD,
};

// `crate::process` é o escalonador (reexportado por `exec`); o módulo
// `abi::process` só trata da decodificação das syscalls de processo.
use super::process as process_calls;
use super::thread as thread_calls;
use super::trap::{TrapFrame, ENOSYS};
use crate::abi::{display, fs, system};
use crate::process;

/// Entrada chamada pelo stub `int 0x80`.
#[unsafe(no_mangle)]
extern "C" fn dreamcore_syscall_dispatch(frame: *mut TrapFrame) -> *mut TrapFrame {
    // Preserva os registradores FPU/SSE do processo que fez a syscall: o kernel
    // Rust pode usar XMM entre aqui e o retorno para o espaço de usuário.
    process::save_current_fpu();
    let next = dispatch(frame);
    let next = process::deliver_pending_signal(next);
    process::restore_fpu_for(next);
    next
}

fn dispatch(frame: *mut TrapFrame) -> *mut TrapFrame {
    let Some(frame) = (unsafe { frame.as_mut() }) else {
        return frame;
    };
    match frame.rax {
        // --- processo, sinais e exec --------------------------------------
        SYS_YIELD => {
            frame.rax = 0;
            process::yield_current(frame)
        }
        SYS_EXIT => process::exit_current(frame, frame.rdi as i32),
        SYS_GETPID => frame.ok(process::current_pid() as u64),
        SYS_GETTID => thread_calls::gettid(frame),
        SYS_FORK => process::fork_current(frame),
        SYS_WAITPID => {
            process::waitpid_current(frame, frame.rdi as i64, frame.rsi, frame.rdx as i64)
        }
        SYS_EXEC | SYS_EXECVE => process_calls::exec_handler(frame),
        SYS_SPAWN => process_calls::spawn_handler(frame),
        SYS_KILL => frame.ok(process::send_signal(frame.rdi as usize, frame.rsi) as u64),
        SYS_SIGACTION => {
            frame.ok(process::set_signal_action(frame.rdi, frame.rsi, frame.rdx) as u64)
        }
        SYS_SIGRETURN => {
            if !process::restore_signal_context(frame) {
                frame.fail(super::trap::EINVAL);
            }
            frame
        }
        SYS_PROCESS_LIST => process_calls::process_list(frame),
        SYS_AUTHENTICATE => process_calls::authenticate(frame),
        SYS_PIVOT_ROOT => fs::pivot_root(frame),
        SYS_SBRK => frame.ok(process::grow_heap(frame.rdi as i64)
            .map_or_else(|error| error as u64, |address| address)),
        SYS_THREAD_CREATE => thread_calls::thread_create(frame),
        SYS_THREAD_EXIT => thread_calls::thread_exit(frame),

        // --- espera, relógio e memória mapeada ----------------------------
        //
        // Base de `std::thread`, `std::sync`, `std::time::Instant`,
        // `std::thread::sleep` e do `GlobalAlloc` da biblioteca padrão. Sem
        // estes handlers um runtime Rust com `std` não inicia.
        SYS_FUTEX => thread_calls::futex(frame),
        SYS_NANOSLEEP => thread_calls::nanosleep(frame),
        SYS_CLOCK_GETTIME => thread_calls::clock_gettime(frame),
        SYS_MMAP => thread_calls::mmap(frame),
        SYS_MUNMAP => thread_calls::munmap(frame),
        SYS_SET_TLS => thread_calls::set_tls(frame),
        SYS_TLS_INFO => thread_calls::tls_info(frame),

        // --- descritores, pipes e arquivos --------------------------------
        SYS_OPEN => fs::open(frame),
        SYS_READ_FD => fs::read_fd(frame),
        SYS_WRITE_FD => fs::write_fd(frame),
        SYS_LSEEK => fs::seek(frame),
        SYS_FSTAT => fs::fstat(frame),
        SYS_CLOSE => fs::close(frame),
        SYS_DUP => frame.ok(process::duplicate_descriptor(frame.rdi as usize)
            .map_or_else(|error| error as u64, |fd| fd as u64)),
        SYS_DUP2 => frame.ok(process::duplicate_descriptor_to(
            frame.rdi as usize,
            frame.rsi as usize,
        )
        .map_or_else(|error| error as u64, |fd| fd as u64)),
        SYS_PIPE => process_calls::create_pipe(frame),
        SYS_ISDIR => fs::isdir(frame),
        SYS_ISFILE => fs::isfile(frame),
        SYS_READ_FILE => fs::read_file(frame),
        SYS_READDIR => fs::readdir(frame),
        SYS_STAT => fs::stat(frame),
        SYS_WRITE_FILE => fs::write_file(frame),
        SYS_MKDIR => fs::make_node(frame, fs::NodeKind::Directory),
        SYS_TOUCH => fs::make_node(frame, fs::NodeKind::File),
        SYS_REMOVE => fs::make_node(frame, fs::NodeKind::Remove),

        // --- console e entrada de caractere -------------------------------
        SYS_WRITE => system::console_write(frame),
        SYS_READ => frame.ok(super::input::read_character() as u64),
        SYS_CLEAR => {
            crate::framebuffer::clear();
            frame.ok(0)
        }

        // --- display: framebuffer, superfícies e entrada -------------------
        SYS_FB_INFO => display::framebuffer_info(frame),
        SYS_FB_MAP => display::framebuffer_map(frame),
        SYS_INPUT_POLL => display::input_poll(frame),
        SYS_POINTER_POSITION => display::pointer_position(frame),
        SYS_CONSOLE_RELEASE => display::console_release(frame),
        SYS_SURFACE_CREATE => display::surface_create(frame),
        SYS_SURFACE_PRESENT => display::surface_present(frame),
        SYS_SURFACE_FLUSH => display::surface_flush(frame),
        SYS_SURFACE_DESTROY => display::surface_destroy(frame),
        SYS_SURFACE_POLL => display::surface_poll(frame),
        SYS_SURFACE_SNAPSHOT => display::surface_snapshot(frame),
        SYS_SURFACE_SEND_EVENT => display::surface_send_event(frame),
        SYS_SURFACE_MOVE => display::surface_move(frame),

        // --- sistema: identidade, tempo, disco, energia, instalação -------
        SYS_ABI_VERSION => frame.ok(imagineos_abi::ABI_VERSION),
        SYS_GETIDENTITY => system::identity(frame),
        SYS_SYSTEM_INFO => system::system_info(frame),
        SYS_GETTIMEOFDAY => system::gettimeofday(frame),
        SYS_DMESG => system::dmesg(frame),
        SYS_DISK_COUNT => frame.ok(u64::from(crate::ata::is_ready())),
        SYS_DISK_SECTORS => system::disk_sectors(frame),
        SYS_INSTALL_DISK | SYS_INSTALL_DISK_CONFIG => system::install_disk(frame),
        SYS_SHUTDOWN_REQUEST => system::request_shutdown(frame),
        SYS_SHUTDOWN_POLL => system::poll_shutdown(frame),
        SYS_POWER_OFF => system::power_off(frame),

        _ => frame.fail(ENOSYS),
    }
}
