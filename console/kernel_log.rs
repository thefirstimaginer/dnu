//! Log circular do kernel (`dmesg`): 4 KiB, sobrescrevendo o mais antigo.
//!
//! Toda saída passa por `append`, que também espelha os bytes na serial para
//! diagnóstico no QEMU.

use core::cell::UnsafeCell;

pub const CAPACITY: usize = 4096;

pub struct LogBuffer {
    bytes: [u8; CAPACITY],
    next: usize,
    length: usize,
}

impl LogBuffer {
    pub const fn new() -> Self {
        Self {
            bytes: [0; CAPACITY],
            next: 0,
            length: 0,
        }
    }

    pub fn append(&mut self, mut bytes: &[u8]) {
        if bytes.len() > CAPACITY {
            bytes = &bytes[bytes.len() - CAPACITY..];
        }
        for &byte in bytes {
            self.bytes[self.next] = byte;
            self.next = (self.next + 1) % CAPACITY;
            self.length = (self.length + 1).min(CAPACITY);
        }
    }

    pub fn read(&self, output: &mut [u8]) -> usize {
        let length = self.length.min(output.len());
        let skipped = self.length - length;
        let start = (self.next + CAPACITY - self.length + skipped) % CAPACITY;
        let first = length.min(CAPACITY - start);
        output[..first].copy_from_slice(&self.bytes[start..start + first]);
        if first < length {
            output[first..length].copy_from_slice(&self.bytes[..length - first]);
        }
        length
    }
}

/// Log global do kernel; só o console (single-core, sem preempção dentro de
/// `append`) o acessa, então não há lock.
pub struct SharedLog(UnsafeCell<LogBuffer>);
unsafe impl Sync for SharedLog {}

pub static KERNEL_LOG: SharedLog = SharedLog(UnsafeCell::new(LogBuffer::new()));

pub fn append(bytes: &[u8]) {
    unsafe { (*KERNEL_LOG.0.get()).append(bytes) };
    // Espelha na porta serial (debug no QEMU com -serial file).
    crate::serial_write(bytes);
}

pub fn read(output: &mut [u8]) -> usize {
    unsafe { (*KERNEL_LOG.0.get()).read(output) }
}
