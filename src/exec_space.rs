//! Cópia entre o espaço de endereço do processo atual e o kernel.
//!
//! O mapa de páginas é consultado página a página (`AddressSpace::translate`),
//! então uma cópia que atravessa regiões não mapeadas falha sem tocar memória
//! inválida. Também concentra os helpers usados pelo loader (`zero_user_range`
//! e `copy_to_user` recebem o espaço explicitamente, porque o do processo novo
//! ainda não está ativo).

use core::ptr;

use crate::paging::AddressSpace;

use super::loader::LoadError;
use super::process::{PAGE_SIZE, SCHEDULER};

/// Executa `operation` com o espaço de endereço do processo atual.
pub fn with_current<T>(operation: impl FnOnce(AddressSpace) -> T) -> T {
    let scheduler = unsafe { &*SCHEDULER.0.get() };
    operation(
        scheduler.processes[scheduler.current]
            .address_space
            .expect("process address space"),
    )
}

/// Converte uma cópia explícita para o erro do loader.
pub fn copy_to_space_result(
    space: &AddressSpace,
    address: u64,
    input: &[u8],
) -> Result<(), LoadError> {
    if copy_to_space(space, address, input) {
        Ok(())
    } else {
        Err(LoadError::InvalidMapping)
    }
}

/// Copia `output.len()` bytes do usuário para o kernel.
pub fn copy_from_current(address: u64, output: &mut [u8]) -> bool {
    with_current(|space| copy_from_space(&space, address, output))
}

/// Copia `input` do kernel para o usuário.
pub fn copy_to_current(address: u64, input: &[u8]) -> bool {
    with_current(|space| copy_to_space(&space, address, input))
}

/// Versão explícita do `copy_from_current` (espaço do processo novo).
pub fn copy_from_space(space: &AddressSpace, address: u64, output: &mut [u8]) -> bool {
    let mut copied = 0usize;
    while copied < output.len() {
        let Some(current) = address.checked_add(copied as u64) else {
            return false;
        };
        let Some(source) = space.translate(current) else {
            return false;
        };
        let available =
            ((PAGE_SIZE - (current & (PAGE_SIZE - 1))) as usize).min(output.len() - copied);
        unsafe {
            ptr::copy_nonoverlapping(source, output.as_mut_ptr().add(copied), available);
        }
        copied += available;
    }
    true
}

/// Versão explícita do `copy_to_current` (espaço do processo novo).
pub fn copy_to_space(space: &AddressSpace, address: u64, input: &[u8]) -> bool {
    let mut copied = 0usize;
    while copied < input.len() {
        let Some(current) = address.checked_add(copied as u64) else {
            return false;
        };
        let Some(destination) = space.translate(current) else {
            return false;
        };
        let available =
            ((PAGE_SIZE - (current & (PAGE_SIZE - 1))) as usize).min(input.len() - copied);
        unsafe {
            ptr::copy_nonoverlapping(input.as_ptr().add(copied), destination, available);
        }
        copied += available;
    }
    true
}
