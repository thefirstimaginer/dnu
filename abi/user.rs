//! Cópia de dados entre o kernel e o espaço de usuário, com validação.
//!
//! Todo ponteiro que vem de `rdi`/`rsi`/... é não confiável: pode apontar para
//! memória não mapeada, atravessar uma borda de página ou ter um comprimento
//! absurdo. Os helpers abaixo falham com o errno correspondente em vez de
//! deixar o kernel tocar memória arbitrária.

use imagineos_abi::{UserArg, MAX_MUTABLE_PATH, MAX_PATH_QUERY};

use super::trap::{E2BIG, EFAULT, EINVAL};
use crate::process;

/// Copia uma estrutura `#[repr(C)]` do usuário para o kernel.
///
/// Só deve ser usada com tipos `Copy` e sem campos de padding não inicializados
/// lidos pelo kernel; aqui todos os tipos da ABI são POD.
pub fn copy_struct_from_user<T: Copy>(address: u64, value: &mut T) -> bool {
    if address == 0 {
        return false;
    }
    let bytes = unsafe {
        core::slice::from_raw_parts_mut(
            (value as *mut T).cast::<u8>(),
            core::mem::size_of::<T>(),
        )
    };
    process::copy_from_current_user(address, bytes)
}

/// Copia uma estrutura `#[repr(C)]` do kernel para o usuário.
pub fn copy_struct_to_user<T>(address: u64, value: &T) -> bool {
    if address == 0 {
        return false;
    }
    let bytes = unsafe {
        core::slice::from_raw_parts(
            (value as *const T).cast::<u8>(),
            core::mem::size_of::<T>(),
        )
    };
    process::copy_to_current_user(address, bytes)
}

/// Lê um caminho em bytes com `length` explícito, sem NUL, para um buffer fixo.
///
/// O erro devolvido já é o errno que o handler deve gravar em `rax`.
pub fn path_from_user<'a>(
    address: u64,
    length: u64,
    buffer: &'a mut [u8; MAX_MUTABLE_PATH],
) -> Result<&'a str, i64> {
    copy_path(address, length, buffer)
}

/// Lê um caminho de consulta (128 bytes, sem NUL) para um buffer próprio.
pub fn query_path_from_user(
    address: u64,
    length: u64,
    buffer: &mut [u8; MAX_PATH_QUERY],
) -> Result<&str, i64> {
    copy_path(address, length, buffer)
}

/// Valida comprimento, copia e decodifica um caminho já lido do usuário.
fn copy_path<'a>(address: u64, length: u64, buffer: &'a mut [u8]) -> Result<&'a str, i64> {
    let length = usize::try_from(length).map_err(|_| EINVAL)?;
    if length == 0 || length > buffer.len() {
        return Err(EINVAL);
    }
    if !process::copy_from_current_user(address, &mut buffer[..length]) {
        return Err(EFAULT);
    }
    if buffer[..length].contains(&0) {
        return Err(EINVAL);
    }
    core::str::from_utf8(&buffer[..length]).map_err(|_| EINVAL)
}

/// Lê um caminho terminado em NUL (até ao tamanho do buffer) e devolve a fatia.
///
/// Usado por `pivot_root`, onde a ABI não envia o comprimento.
pub fn c_string_from_user(address: u64, buffer: &mut [u8]) -> Option<&str> {
    if address == 0 || !process::copy_from_current_user(address, buffer) {
        return None;
    }
    let end = buffer.iter().position(|&byte| byte == 0).unwrap_or(0);
    core::str::from_utf8(&buffer[..end]).ok()
}

/// Copia um vetor de `UserArg` (`argv`/`envp`) para armazenamento próprio.
///
/// Cada entrada aponta para `length` bytes no espaço de usuário; `storage[index]`
/// guarda esses bytes e `lengths[index]` o comprimento real. É tudo ou nada: se
/// qualquer item falhar, o vetor de destino não é usado.
pub fn load_user_vector<const STORAGE: usize, const ITEM: usize>(
    pointer: u64,
    count: usize,
    storage: &mut [[u8; ITEM]; STORAGE],
    lengths: &mut [usize; STORAGE],
) -> Result<usize, i64> {
    if count > STORAGE {
        return Err(E2BIG);
    }
    if count == 0 {
        return Ok(0);
    }
    let mut records = [UserArg::EMPTY; STORAGE];
    let metadata = unsafe {
        core::slice::from_raw_parts_mut(
            records.as_mut_ptr().cast::<u8>(),
            count * core::mem::size_of::<UserArg>(),
        )
    };
    if !process::copy_from_current_user(pointer, metadata) {
        return Err(EFAULT);
    }
    for index in 0..count {
        let Ok(length) = usize::try_from(records[index].length) else {
            return Err(E2BIG);
        };
        if length > storage[index].len() {
            return Err(E2BIG);
        }
        if length != 0
            && !process::copy_from_current_user(
                records[index].address,
                &mut storage[index][..length],
            )
        {
            return Err(EFAULT);
        }
        if storage[index][..length].contains(&0) {
            return Err(EINVAL);
        }
        lengths[index] = length;
    }
    Ok(count)
}

/// Constrói as fatias `&[u8]` de um vetor já carregado.
///
/// `count <= STORAGE` é garantido pelo carregamento, que também valida que cada
/// item cabe em `ITEM` bytes.
pub fn vector_slices<'a, const STORAGE: usize, const ITEM: usize>(
    storage: &'a [[u8; ITEM]; STORAGE],
    lengths: &[usize; STORAGE],
    count: usize,
) -> Vector<'a, STORAGE> {
    debug_assert!(count <= STORAGE);
    let mut slices: [&[u8]; STORAGE] = [&[]; STORAGE];
    for index in 0..count {
        slices[index] = &storage[index][..lengths[index]];
    }
    Vector { slices, count }
}

/// Coleção de fatias devolvida por [`vector_slices`].
///
/// Um tipo próprio evita alocar: `Deref` dá a visão de `&[&[u8]]` que os
/// loaders de processo esperam.
pub struct Vector<'a, const STORAGE: usize> {
    slices: [&'a [u8]; STORAGE],
    count: usize,
}

impl<'a, const STORAGE: usize> core::ops::Deref for Vector<'a, STORAGE> {
    type Target = [&'a [u8]];

    fn deref(&self) -> &Self::Target {
        &self.slices[..self.count]
    }
}

// Verificações de tamanho dos tipos que atravessam a fronteira de privilégio.
// Se um deles mudar de forma incompatível, o build (e os testes) quebra em vez
// de corromper a memória em tempo de execução.
#[cfg(test)]
mod layout {
    use imagineos_abi::{
        FramebufferInfo, InputEvent, SurfaceEvent, SurfaceInfo, SurfaceSnapshot, UserArg, UserIdentity,
        UserStat,
    };

    #[test]
    fn abi_structs_keep_their_shared_layout() {
        assert_eq!(core::mem::size_of::<UserArg>(), 16);
        // `u64` + 4 x `u32`: 24 bytes, sem padding.
        assert_eq!(core::mem::size_of::<UserStat>(), 24);
        assert_eq!(core::mem::size_of::<FramebufferInfo>(), 32);
        assert_eq!(core::mem::size_of::<InputEvent>(), 24);
        assert_eq!(core::mem::size_of::<SurfaceInfo>(), 24);
        assert_eq!(core::mem::size_of::<SurfaceEvent>(), 28);
        assert_eq!(core::mem::size_of::<SurfaceSnapshot>(), 40);
        // `UserIdentity` é `[u32; 4]` + `[u8; 32]` + `u32` + `[u8; 64]`, com
        // padding de 4 bytes depois de `hostname_length` (alinhamento 4).
        assert_eq!(core::mem::size_of::<UserIdentity>(), 108);
    }
}
