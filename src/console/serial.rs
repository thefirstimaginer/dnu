//! Porta serial COM1 (16550) em modo de diagnóstico.
//!
//! Toda saída de log do console espelha aqui para que o QEMU (`-serial stdio`)
//! mostre o boot mesmo antes de o framebuffer estar inicializado.

use core::arch::asm;

const COM1_BASE: u16 = 0x3f8;

/// Configura o COM1 em 115200 8N1 sem FIFO de IRQ: o kernel só escreve.
pub fn init() {
    unsafe {
        out(COM1_BASE + 1, 0x00);
        out(COM1_BASE + 3, 0x80);
        out(COM1_BASE, 0x03);
        out(COM1_BASE + 1, 0x00);
        out(COM1_BASE + 3, 0x03);
        out(COM1_BASE + 2, 0xc7);
        out(COM1_BASE + 4, 0x0b);
    }
}

/// Compatibilidade: o resto do kernel chama a serial por `crate::serial_write`.
pub fn serial_write(bytes: &[u8]) {
    write(bytes);
}

/// Escreve bytes na serial, convertendo `\n` em `\r\n`.
pub fn write(bytes: &[u8]) {
    let mut previous_was_cr = false;
    for &byte in bytes {
        if byte == b'\n' && !previous_was_cr {
            write_byte(b'\r');
        }
        write_byte(byte);
        previous_was_cr = byte == b'\r';
    }
}

/// Escreve um número hexadecimal (diagnóstico).
#[cfg(feature = "kernel-debug")]
#[allow(dead_code)]
pub fn write_hex(mut value: u64) {
    if value == 0 {
        write(b"0");
        return;
    }
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
    write(&digits[cursor..]);
}

/// Escreve um número decimal (diagnóstico).
#[cfg(feature = "kernel-debug")]
pub fn write_number(mut value: u64) {
    let mut digits = [0u8; 20];
    let mut cursor = digits.len();
    if value == 0 {
        write(b"0");
        return;
    }
    while value != 0 {
        cursor -= 1;
        digits[cursor] = b'0' + (value % 10) as u8;
        value /= 10;
    }
    write(&digits[cursor..]);
}

/// Lê um byte da serial, se houver (usado pela entrada do console).
pub fn read_byte() -> Option<u8> {
    if unsafe { in_port(COM1_BASE + 5) } & 1 != 0 {
        Some(unsafe { in_port(COM1_BASE) })
    } else {
        None
    }
}

fn write_byte(byte: u8) {
    unsafe {
        while in_port(COM1_BASE + 5) & 0x20 == 0 {
            asm!("pause", options(nomem, nostack, preserves_flags));
        }
        out(COM1_BASE, byte);
    }
}

unsafe fn out(port: u16, value: u8) {
    asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
}

unsafe fn in_port(port: u16) -> u8 {
    let value: u8;
    asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags));
    value
}
