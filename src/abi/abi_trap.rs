//! O frame de trap compartilhado entre o stub `int 0x80`, o escalonador e os
//! handlers de syscall.
//!
//! O layout é exatamente o dos `push` do stub em `arch/x86_64/idt.rs` seguido
//! do frame que o hardware empilha no `int`. Não reordene os campos sem
//! reordenar o assembly: é um contrato binário.

#[repr(C)]
#[derive(Clone, Copy)]
pub struct TrapFrame {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

/// Códigos de erro devolvidos em `rax` (negativos, estilo errno).
///
/// Ficam reunidos aqui para que toda a ABI negocie os mesmos números. Nem todos
/// são usados hoje; manter a tabela completa evita números mágicos nos handlers.
pub const EPERM: i64 = -1;
pub const ENOENT: i64 = -2;
#[allow(dead_code)]
pub const ESRCH: i64 = -3;
#[allow(dead_code)]
pub const EINTR: i64 = -4;
#[allow(dead_code)]
pub const EIO: i64 = -5;
pub const E2BIG: i64 = -7;
#[allow(dead_code)]
pub const ENOEXEC: i64 = -8;
#[allow(dead_code)]
pub const EBADF: i64 = -9;
#[allow(dead_code)]
pub const ECHILD: i64 = -10;
#[allow(dead_code)]
pub const EAGAIN: i64 = -11;
pub const ENOMEM: i64 = -12;
pub const EACCES: i64 = -13;
pub const EFAULT: i64 = -14;
#[allow(dead_code)]
pub const EEXIST: i64 = -17;
pub const ENODEV: i64 = -19;
pub const ENOTDIR: i64 = -20;
pub const EISDIR: i64 = -21;
pub const EINVAL: i64 = -22;
#[allow(dead_code)]
pub const EMFILE: i64 = -24;
pub const ENOSPC: i64 = -28;
#[allow(dead_code)]
pub const ESPIPE: i64 = -29;
/// `futex`/`nanosleep` com prazo: o tempo acabou antes do evento.
pub const ETIMEDOUT: i64 = -110;
#[allow(dead_code)]
pub const ERANGE: i64 = -34;
pub const ENOSYS: i64 = -38;
pub const EOVERFLOW: i64 = -75;
#[allow(dead_code)]
pub const EOPNOTSUPP: i64 = -95;

impl TrapFrame {
    /// Registra `error` como o resultado da syscall.
    pub fn fail(&mut self, error: i64) -> *mut TrapFrame {
        self.rax = error as u64;
        self
    }

    /// Registra `value` como o resultado da syscall.
    pub fn ok(&mut self, value: u64) -> *mut TrapFrame {
        self.rax = value;
        self
    }
}
