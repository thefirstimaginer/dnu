//! Leitura de caracteres do console do kernel (teclado PS/2 + serial).
//!
//! Usada por `SYS_READ` e por `read_fd` no descritor `stdin`. As interrupções
//! ficam desligadas durante a consulta à porta porque o estado do teclado
//! (shift, dead keys, prefixo E0) é global e a preempção no meio da sequência
//! "ler status, ler dado, atualizar estado" embaralharia a digitação — foi
//! assim que caracteres do login sumiram.

use core::arch::asm;
use core::cell::UnsafeCell;

use super::display::KEYBOARD;
use crate::framebuffer;
use crate::process;

pub(super) struct SharedDecoder(UnsafeCell<crate::utf8::Decoder>);
unsafe impl Sync for SharedDecoder {}
pub(super) static SERIAL_UTF8: SharedDecoder =
    SharedDecoder(UnsafeCell::new(crate::utf8::Decoder::new()));

impl SharedDecoder {
    /// Decodificador UTF-8 da serial; a serial é global e só o console a lê.
    pub fn get(&self) -> *mut crate::utf8::Decoder {
        self.0.get()
    }
}

/// Intervalo do cursor piscando, em ciclos de TSC.
const BLINK_INTERVAL: u64 = 750_000_000;

/// Lê um caractere do teclado (preferência) ou da serial, bloqueando até haver.
pub fn read_character() -> u32 {
    let mut last_blink = read_tsc();
    framebuffer::set_cursor_visible(true);
    let mut cursor_visible = true;
    loop {
        let character = {
            let interrupts_were_enabled = disable_interrupts();
            let polled = unsafe { (&mut *KEYBOARD.get()).poll_char() };
            restore_interrupts(interrupts_were_enabled);
            polled
        };
        if let Some(character) = character {
            // Com o servidor de display no comando, a tecla vai para a fila de
            // eventos dele em vez da linha de comando do kernel.
            if crate::input::is_captured() {
                crate::input::push_key(character);
                continue;
            }
            framebuffer::set_cursor_visible(false);
            if character == crate::keyboard::KEY_INTERRUPT {
                let _ = process::send_signal(process::current_pid(), imagineos_abi::SIGNAL_INT);
                return b'\n' as u32;
            }
            return character as u32;
        }
        if let Some(character) = serial_character() {
            framebuffer::set_cursor_visible(false);
            if character == '\u{3}' {
                let _ = process::send_signal(process::current_pid(), imagineos_abi::SIGNAL_INT);
                return b'\n' as u32;
            }
            return character as u32;
        }
        let now = read_tsc();
        if now.wrapping_sub(last_blink) >= BLINK_INTERVAL {
            cursor_visible = !cursor_visible;
            framebuffer::set_cursor_visible(cursor_visible);
            last_blink = now;
        }
        // As interrupções seguem ligadas, então o timer pode preemptar.
        core::hint::spin_loop();
    }
}

fn serial_character() -> Option<char> {
    unsafe {
        let decoder = &mut *SERIAL_UTF8.get();
        if let Some(character) = decoder.push_pending() {
            return Some(character);
        }
        decoder.push(crate::uart::read_byte()?)
    }
}

/// Desabilita interrupções e devolve o estado anterior de `RFLAGS.IF`.
pub fn disable_interrupts() -> bool {
    let flags: u64;
    unsafe {
        asm!("pushfq", "pop {}", out(reg) flags, options(nomem, preserves_flags));
        asm!("cli", options(nomem, nostack, preserves_flags));
    }
    flags & (1 << 9) != 0
}

/// Restaura `RFLAGS.IF` conforme o valor devolvido por [`disable_interrupts`].
/// O bit `IF` só é gravado quando o chamador tinha interrupções ligadas.
pub fn restore_interrupts(was_enabled: bool) {
    if was_enabled {
        unsafe {
            asm!("sti", options(nomem, nostack, preserves_flags));
        }
    }
}

fn read_tsc() -> u64 {
    let low: u32;
    let high: u32;
    unsafe {
        asm!(
            "rdtsc",
            out("eax") low,
            out("edx") high,
            options(nomem, nostack, preserves_flags)
        );
    }
    ((high as u64) << 32) | low as u64
}
