//! Syscalls de arquivos e descritores.
//!
//! Convenção de argumentos (ver `imagineos_abi::syscall`):
//!
//! * `open(path, len, flags, mode)` — `rdi/rsi/rdx/r10`;
//! * `read_fd(fd, buf, count)` / `write_fd(fd, buf, count)` — `rdi/rsi/rdx`;
//! * `lseek(fd, offset, whence)`, `fstat(fd, &stat)` e as consultas por caminho
//!   (`isdir`, `isfile`, `read_file`, `readdir`, `stat`);
//! * `mkdir(path, len, recursive)`, `touch(path, len)`, `remove(path, len, recursive)`.
//!
//! Cada handler valida limites, confere permissões e só então toca o VFS.

use imagineos_abi::{
    OPEN_APPEND, OPEN_CREATE, OPEN_EXCLUSIVE, OPEN_READ, OPEN_TRUNCATE, OPEN_WRITE, SYS_MKDIR,
};
use imagineos_abi::{MAX_IO_SIZE, MAX_MUTABLE_PATH, MAX_PATH_QUERY, UserStat};

use super::access;
use super::trap::{
    TrapFrame, EACCES, EFAULT, EINVAL, EISDIR, ENOENT, ENOSPC, ENOTDIR, EOVERFLOW,
};
use super::user::{copy_struct_to_user, path_from_user, query_path_from_user};
use crate::process;

#[derive(Clone, Copy)]
pub enum NodeKind {
    Directory,
    File,
    Remove,
}

// ---------------------------------------------------------------------------
// Descritores
// ---------------------------------------------------------------------------

/// `open`: devolve um descritor novo para `path`.
pub fn open(frame: &mut TrapFrame) -> *mut TrapFrame {
    let mut path_buffer = [0u8; MAX_MUTABLE_PATH];
    let path = match path_from_user(frame.rdi, frame.rsi, &mut path_buffer) {
        Ok(path) => path,
        Err(error) => return frame.fail(error),
    };
    let flags = frame.rdx;
    if !flags_are_valid(flags) {
        return frame.fail(EINVAL);
    }
    if crate::ramfs::is_directory(path) {
        return frame.fail(EISDIR);
    }
    let exists = crate::ramfs::is_file(path);
    if exists && flags & OPEN_CREATE != 0 && flags & OPEN_EXCLUSIVE != 0 {
        return frame.fail(-17); // EEXIST
    }
    if exists
        && ((flags & OPEN_READ != 0 && !access::allowed(path, 4))
            || (flags & OPEN_WRITE != 0 && !access::allowed(path, 2)))
    {
        return frame.fail(EACCES);
    }
    if !exists && flags & OPEN_CREATE != 0 && !access::parent_allowed(path, 3) {
        return frame.fail(EACCES);
    }
    if !exists {
        if flags & OPEN_CREATE == 0 {
            return frame.fail(ENOENT);
        }
        let (uid, gid, _) = process::current_credentials();
        if let Err(error) =
            crate::ramfs::create_file_with_mode_as(path, (frame.r10 as u16) & 0o777, uid, gid)
        {
            return frame.fail(fs_error_code(error));
        }
    }
    if flags & OPEN_TRUNCATE != 0 {
        let (uid, gid, _) = process::current_credentials();
        if let Err(error) = crate::ramfs::write_file_as(path, &[], uid, gid) {
            return frame.fail(fs_error_code(error));
        }
    }
    let offset = if flags & OPEN_APPEND != 0 {
        match crate::ramfs::file_len(path) {
            Ok(length) => length,
            Err(error) => return frame.fail(fs_error_code(error)),
        }
    } else {
        0
    };
    let descriptor = process::Descriptor::file(path, flags, offset);
    match process::allocate_descriptor(descriptor) {
        Some(fd) => frame.ok(fd as u64),
        None => frame.fail(-24), // EMFILE
    }
}

fn flags_are_valid(flags: u64) -> bool {
    let known =
        OPEN_READ | OPEN_WRITE | OPEN_CREATE | OPEN_TRUNCATE | OPEN_APPEND | OPEN_EXCLUSIVE;
    flags & !known == 0
        && flags & (OPEN_READ | OPEN_WRITE) != 0
        && (flags & (OPEN_TRUNCATE | OPEN_APPEND) == 0 || flags & OPEN_WRITE != 0)
        && !(flags & OPEN_TRUNCATE != 0 && flags & OPEN_APPEND != 0)
        && (flags & OPEN_EXCLUSIVE == 0 || flags & OPEN_CREATE != 0)
}

/// `read_fd`: lê de um descritor (arquivo, stdin ou pipe).
pub fn read_fd(frame: &mut TrapFrame) -> *mut TrapFrame {
    let (Ok(fd), Ok(capacity)) = (usize::try_from(frame.rdi), usize::try_from(frame.rdx)) else {
        return frame.fail(EINVAL);
    };
    if capacity > MAX_IO_SIZE {
        return frame.fail(EINVAL);
    }
    if capacity == 0 {
        return frame.ok(0);
    }
    let Some(mut descriptor) = process::descriptor(fd) else {
        return frame.fail(-9); // EBADF
    };
    if descriptor.kind == process::DescriptorKind::File && !can_read_file(&descriptor) {
        return frame.fail(EACCES);
    }
    let mut buffer = [0u8; MAX_IO_SIZE];
    let length = match descriptor.kind {
        process::DescriptorKind::Stdin => read_stdin(&mut descriptor, &mut buffer, capacity),
        process::DescriptorKind::File if descriptor.flags & OPEN_READ != 0 => {
            let path = descriptor.path();
            match crate::ramfs::read_at(path, descriptor.offset, &mut buffer[..capacity]) {
                Ok(length) => {
                    descriptor.offset += length;
                    length
                }
                Err(error) => return frame.fail(fs_error_code(error)),
            }
        }
        process::DescriptorKind::File => return frame.fail(-9),
        process::DescriptorKind::PipeRead => {
            return match crate::ipc::read(descriptor.pipe, &mut buffer[..capacity]) {
                // Pipes não são posicionáveis: o descritor não muda.
                Ok(length) => finish_io(frame, frame.rsi, &buffer[..length]),
                Err(error) => frame.fail(error.to_errno()),
            };
        }
        _ => return frame.fail(-9),
    };
    if !process::copy_to_current_user(frame.rsi, &buffer[..length]) {
        return frame.fail(EFAULT);
    }
    if !process::update_descriptor(fd, descriptor) {
        return frame.fail(-9);
    }
    frame.ok(length as u64)
}

/// `write_fd`: escreve em stdout/stderr, pipe ou arquivo.
pub fn write_fd(frame: &mut TrapFrame) -> *mut TrapFrame {
    let (Ok(fd), Ok(length)) = (usize::try_from(frame.rdi), usize::try_from(frame.rdx)) else {
        return frame.fail(EINVAL);
    };
    if length > MAX_IO_SIZE {
        return frame.fail(EINVAL);
    }
    let Some(mut descriptor) = process::descriptor(fd) else {
        return frame.fail(-9); // EBADF
    };
    if descriptor.kind == process::DescriptorKind::File && !can_write_file(&descriptor) {
        return frame.fail(EACCES);
    }
    let mut buffer = [0u8; MAX_IO_SIZE];
    if !process::copy_from_current_user(frame.rsi, &mut buffer[..length]) {
        return frame.fail(EFAULT);
    }
    match descriptor.kind {
        process::DescriptorKind::Stdout | process::DescriptorKind::Stderr => {
            crate::console_write_bytes(&buffer[..length]);
            frame.ok(length as u64)
        }
        process::DescriptorKind::PipeWrite => {
            match crate::ipc::write(descriptor.pipe, &buffer[..length]) {
                Ok(written) => frame.ok(written as u64),
                Err(error) => frame.fail(error.to_errno()),
            }
        }
        process::DescriptorKind::File if descriptor.flags & OPEN_WRITE != 0 => {
            let path = descriptor.path();
            let offset = if descriptor.flags & OPEN_APPEND != 0 {
                match crate::ramfs::file_len(path) {
                    Ok(offset) => offset,
                    Err(error) => return frame.fail(fs_error_code(error)),
                }
            } else {
                descriptor.offset
            };
            match crate::ramfs::write_at(path, offset, &buffer[..length]) {
                Ok(written) => {
                    descriptor.offset = offset + written;
                    if !process::update_descriptor(fd, descriptor) {
                        return frame.fail(-9);
                    }
                    frame.ok(written as u64)
                }
                Err(error) => frame.fail(fs_error_code(error)),
            }
        }
        _ => frame.fail(-9),
    }
}

/// `lseek(fd, offset, whence)`.
pub fn seek(frame: &mut TrapFrame) -> *mut TrapFrame {
    let Ok(fd) = usize::try_from(frame.rdi) else {
        return frame.fail(-9);
    };
    let Some(mut descriptor) = process::descriptor(fd) else {
        return frame.fail(-9);
    };
    if descriptor.kind != process::DescriptorKind::File {
        return frame.fail(-29); // ESPIPE
    }
    let base = match frame.rdx {
        0 => 0i128,
        1 => descriptor.offset as i128,
        2 => match crate::ramfs::file_len(descriptor.path()) {
            Ok(length) => length as i128,
            Err(error) => return frame.fail(fs_error_code(error)),
        },
        _ => return frame.fail(EINVAL),
    };
    let Ok(offset) = usize::try_from(base + (frame.rsi as i64) as i128) else {
        return frame.fail(EINVAL);
    };
    descriptor.offset = offset;
    if !process::update_descriptor(fd, descriptor) {
        return frame.fail(-9);
    }
    frame.ok(offset as u64)
}

/// `fstat(fd, &stat)`.
pub fn fstat(frame: &mut TrapFrame) -> *mut TrapFrame {
    let Ok(fd) = usize::try_from(frame.rdi) else {
        return frame.fail(-9);
    };
    let Some(descriptor) = process::descriptor(fd) else {
        return frame.fail(-9);
    };
    if descriptor.kind != process::DescriptorKind::File {
        return frame.fail(-9);
    }
    let Some(metadata) = crate::ramfs::metadata(descriptor.path()) else {
        return frame.fail(ENOENT);
    };
    write_stat(frame, frame.rsi, metadata)
}

/// `close(fd)`.
pub fn close(frame: &mut TrapFrame) -> *mut TrapFrame {
    let Ok(fd) = usize::try_from(frame.rdi) else {
        return frame.fail(-9);
    };
    if process::close_descriptor(fd) {
        frame.ok(0)
    } else {
        frame.fail(-9)
    }
}

// ---------------------------------------------------------------------------
// Consultas por caminho
// ---------------------------------------------------------------------------

/// `isdir(path, len)`.
pub fn isdir(frame: &mut TrapFrame) -> *mut TrapFrame {
    answer_path_query(frame, 4, |path| crate::ramfs::is_directory(path))
}

/// `isfile(path, len)`.
pub fn isfile(frame: &mut TrapFrame) -> *mut TrapFrame {
    answer_path_query(frame, 4, |path| crate::ramfs::is_file(path))
}

/// `read_file(path, len, buf, capacity)`.
pub fn read_file(frame: &mut TrapFrame) -> *mut TrapFrame {
    let mut path_buffer = [0u8; MAX_PATH_QUERY];
    let path = match query_path_from_user(frame.rdi, frame.rsi, &mut path_buffer) {
        Ok(path) => path,
        Err(error) => return frame.fail(error),
    };
    let Ok(capacity) = usize::try_from(frame.r10) else {
        return frame.fail(EINVAL);
    };
    if capacity > MAX_IO_SIZE {
        return frame.fail(EINVAL);
    }
    if !access::allowed(path, 4) {
        return frame.fail(EACCES);
    }
    if crate::ramfs::is_directory(path) {
        return frame.fail(EISDIR);
    }
    let Some(bytes) = crate::ramfs::read(path) else {
        return frame.fail(ENOENT);
    };
    if bytes.len() > capacity {
        return frame.fail(EOVERFLOW);
    }
    if process::copy_to_current_user(frame.rdx, bytes) {
        frame.ok(bytes.len() as u64)
    } else {
        frame.fail(EFAULT)
    }
}

/// `readdir(path, len, buf, capacity, show_hidden)`.
pub fn readdir(frame: &mut TrapFrame) -> *mut TrapFrame {
    let mut path_buffer = [0u8; MAX_PATH_QUERY];
    let path = match query_path_from_user(frame.rdi, frame.rsi, &mut path_buffer) {
        Ok(path) => path,
        Err(error) => return frame.fail(error),
    };
    let Ok(capacity) = usize::try_from(frame.r10) else {
        return frame.fail(EINVAL);
    };
    if capacity > MAX_IO_SIZE {
        return frame.fail(EINVAL);
    }
    if !access::allowed(path, 5) {
        return frame.fail(EACCES);
    }
    if !crate::ramfs::is_directory(path) {
        return frame.fail(if crate::ramfs::is_file(path) {
            ENOTDIR
        } else {
            ENOENT
        });
    }
    let mut buffer = [0u8; MAX_IO_SIZE];
    let Some(length) =
        crate::ramfs::list_directory_with_hidden(path, frame.r9 != 0, &mut buffer[..capacity])
    else {
        return frame.fail(EOVERFLOW);
    };
    if process::copy_to_current_user(frame.rdx, &buffer[..length]) {
        frame.ok(length as u64)
    } else {
        frame.fail(EFAULT)
    }
}

/// `stat(path, len, &stat)`.
pub fn stat(frame: &mut TrapFrame) -> *mut TrapFrame {
    let mut path_buffer = [0u8; 128];
    let path = match query_path_from_user(frame.rdi, frame.rsi, &mut path_buffer) {
        Ok(path) => path,
        Err(error) => return frame.fail(error),
    };
    let Some(metadata) = crate::ramfs::metadata(path) else {
        return frame.fail(ENOENT);
    };
    write_stat(frame, frame.rdx, metadata)
}

// ---------------------------------------------------------------------------
// Mutações
// ---------------------------------------------------------------------------

/// `write_file(path, len, buf, size)`: substitui o conteúdo inteiro.
///
/// O tamanho viaja no **quarto** argumento, que a ABI entrega em `r10` (o `rcx`
/// é reservado pelo `int 0x80` para o endereço de retorno). A libc (`__dc_write_file`)
/// e o runtime Rust usam o mesmo registrador, por isso a leitura é fixa aqui.
pub fn write_file(frame: &mut TrapFrame) -> *mut TrapFrame {
    let mut path_buffer = [0u8; MAX_MUTABLE_PATH];
    let path = match path_from_user(frame.rdi, frame.rsi, &mut path_buffer) {
        Ok(path) => path,
        Err(error) => return frame.fail(error),
    };
    let length = match usize::try_from(frame.r10) {
        Ok(length) if length <= crate::ramfs::MAX_WRITE_FILE_SIZE => length,
        _ => return frame.fail(ENOSPC),
    };
    let allowed = if crate::ramfs::is_file(path) {
        access::allowed(path, 2)
    } else {
        access::parent_allowed(path, 3)
    };
    if !allowed {
        return frame.fail(EACCES);
    }
    let mut contents = [0u8; imagineos_abi::MAX_WRITE_FILE_SIZE];
    if !process::copy_from_current_user(frame.rdx, &mut contents[..length]) {
        return frame.fail(EFAULT);
    }
    if crate::usr::config::is_global_config_path(path)
        && crate::usr::config::validate(&contents[..length]).is_err()
    {
        return frame.fail(EINVAL);
    }
    let (uid, gid, _) = process::current_credentials();
    match crate::ramfs::write_file_as(path, &contents[..length], uid, gid) {
        Ok(()) => {
            if crate::usr::config::is_global_config_path(path) {
                let _ = crate::usr::config::load(&contents[..length]);
            }
            frame.ok(length as u64)
        }
        Err(error) => frame.fail(fs_error_code(error)),
    }
}

/// `mkdir`/`touch`/`remove`, que compartilham a decodificação do caminho.
pub fn make_node(frame: &mut TrapFrame, kind: NodeKind) -> *mut TrapFrame {
    let mut path_buffer = [0u8; MAX_MUTABLE_PATH];
    let path = match path_from_user(frame.rdi, frame.rsi, &mut path_buffer) {
        Ok(path) => path,
        Err(error) => return frame.fail(error),
    };
    // Em `mkdir -p` o bit `recursive` está em `rdx`, assim como o `remove`
    // recursivo; para `touch` o registrador é ignorado.
    let recursive = frame.rdx != 0;
    let mkdir_recursive = matches!(kind, NodeKind::Directory) && frame.rax == SYS_MKDIR && recursive;
    let permitted = if mkdir_recursive {
        access::recursive_parent_allowed(path, 3)
    } else {
        access::parent_allowed(path, 3)
    };
    if !permitted {
        return frame.fail(EACCES);
    }
    let (uid, gid, _) = process::current_credentials();
    let result = match kind {
        NodeKind::Directory if mkdir_recursive => {
            crate::ramfs::create_directory_with_parents_as(path, true, uid, gid)
        }
        NodeKind::Directory => crate::ramfs::create_directory_as(path, uid, gid),
        NodeKind::File => crate::ramfs::create_file_as(path, uid, gid),
        NodeKind::Remove => crate::ramfs::remove(path, recursive),
    };
    match result {
        Ok(()) => frame.ok(0),
        Err(error) => frame.fail(fs_error_code(error)),
    }
}

/// `pivot_root(new_root, put_old)`: troca a raiz do sistema de arquivos.
pub fn pivot_root(frame: &mut TrapFrame) -> *mut TrapFrame {
    let mut new_root = [0u8; MAX_MUTABLE_PATH];
    let mut put_old = [0u8; MAX_MUTABLE_PATH];
    let Some(new_root) = super::user::c_string_from_user(frame.rdi, &mut new_root) else {
        return frame.fail(EFAULT);
    };
    let Some(put_old) = super::user::c_string_from_user(frame.rsi, &mut put_old) else {
        return frame.fail(EFAULT);
    };
    match crate::vfs::pivot_root_to(new_root, put_old) {
        Ok(()) => frame.ok(0),
        Err(error) => frame.fail(error.to_errno()),
    }
}

// ---------------------------------------------------------------------------
// Auxiliares
// ---------------------------------------------------------------------------

/// Executa uma consulta booleana de caminho (`isdir`/`isfile`).
fn answer_path_query(
    frame: &mut TrapFrame,
    requested: u16,
    predicate: impl FnOnce(&str) -> bool,
) -> *mut TrapFrame {
    let mut path_buffer = [0u8; 128];
    let path = match query_path_from_user(frame.rdi, frame.rsi, &mut path_buffer) {
        Ok(path) => path,
        Err(error) => return frame.fail(error),
    };
    if !access::allowed(path, requested) {
        return frame.fail(EACCES);
    }
    frame.ok(u64::from(predicate(path)))
}

fn can_read_file(descriptor: &process::Descriptor) -> bool {
    access::allowed(descriptor.path(), 4)
}

fn can_write_file(descriptor: &process::Descriptor) -> bool {
    access::allowed(descriptor.path(), 2)
}

/// Atende stdin: consome o caractere pendente no descritor, se houver.
fn read_stdin(
    descriptor: &mut process::Descriptor,
    buffer: &mut [u8; MAX_IO_SIZE],
    capacity: usize,
) -> usize {
    if descriptor.input_offset == descriptor.input_length {
        let mut encoded = [0u8; 4];
        let character = super::input::read_character();
        let value = char::from_u32(character).unwrap_or('\u{fffd}');
        let bytes = value.encode_utf8(&mut encoded).as_bytes();
        descriptor.input[..bytes.len()].copy_from_slice(bytes);
        descriptor.input_length = bytes.len();
        descriptor.input_offset = 0;
    }
    let length = capacity.min(descriptor.input_length - descriptor.input_offset);
    buffer[..length].copy_from_slice(
        &descriptor.input[descriptor.input_offset..descriptor.input_offset + length],
    );
    descriptor.input_offset += length;
    length
}

fn finish_io(frame: &mut TrapFrame, address: u64, bytes: &[u8]) -> *mut TrapFrame {
    if process::copy_to_current_user(address, bytes) {
        frame.ok(bytes.len() as u64)
    } else {
        frame.fail(EFAULT)
    }
}

fn write_stat(frame: &mut TrapFrame, address: u64, metadata: crate::dfs::Metadata) -> *mut TrapFrame {
    let stat = UserStat {
        size: metadata.size,
        mode: metadata.mode as u32,
        uid: metadata.uid,
        gid: metadata.gid,
        kind: u32::from(metadata.is_directory),
    };
    if copy_struct_to_user(address, &stat) {
        frame.ok(0)
    } else {
        frame.fail(EFAULT)
    }
}

/// Traduz um erro de sistema de arquivos para o errno negativo da ABI.
///
/// A tabela vive em um único lugar (`VfsError::to_errno`), então os backends do
/// VFS e a camada de syscall não podem divergir.
fn fs_error_code(error: crate::ramfs::FsError) -> i64 {
    crate::vfs::VfsError::from(error).to_errno()
}
