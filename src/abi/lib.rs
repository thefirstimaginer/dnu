#![no_std]

pub mod def;
pub mod syscall;
#[path = "abi_trap.rs"]
pub mod trap;
#[path = "input.rs"]
pub mod input;
#[path = "abi_display.rs"]
pub mod display;
#[path = "abi_access.rs"]
pub mod access;
#[path = "abi_console_input.rs"]
pub mod console_input;
#[path = "abi_dispatch.rs"]
pub mod dispatch;
#[path = "abi_fs.rs"]
pub mod fs;
#[path = "abi_process.rs"]
pub mod process;
#[path = "abi_system.rs"]
pub mod system;
#[path = "abi_thread.rs"]
pub mod thread;
#[path = "abi_user.rs"]
pub mod user;

// Reexporta o contrato canônico de `dnu-abi` (constantes SYS_*, structs
// #[repr(C)] e limites compartilhados). O kernel e o userspace importam do
// mesmo crate, evitando desalinhamento de números de syscall e layouts.
pub use dnu_abi::*;
pub use syscall::*;
