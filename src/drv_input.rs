//! Drivers de entrada: teclado e mouse no registro genérico.
//!
//! O consumo dos eventos (syscall de display, `abi_display.rs`) continua
//! lendo os drivers físicos diretamente; este módulo os publica no registro
//! central com a trait [`InputDevice`], o contrato que o kernel espera de
//! *qualquer* fonte de entrada futura (USB HID, touchpad PS/2 etc.). Assim
//! uma nova fonte de entrada só precisa implementar a trait e se registrar —
//! sem tocar no caminho de eventos do display server.
//!
//! A exclusão mútua do teclado é feita desabilitando interrupções durante a
//! leitura (`disable_interrupts`, como em `abi_console_input.rs`); a mesma
//! convenção vale para `poll` aqui.

use crate::mouse::MouseReport;
use crate::drivers::{Device, DeviceClass};

/// Próximo evento de um dispositivo de entrada, em formato neutro.
pub enum Event {
    /// Caractere decodificado pelo driver do teclado.
    Char(char),
    /// Relatório de movimento/botões do apontador.
    Pointer(MouseReport),
}

/// Contrato de um dispositivo que produz eventos de entrada.
pub trait InputDevice: Device {
    /// Lê o próximo evento pendente, se houver.
    fn poll(&self) -> Option<Event>;
}

/// Wrapper sem estado em volta do teclado global (`SharedKeyboard`).
struct KeyboardDevice;

impl Device for KeyboardDevice {
    fn name(&self) -> &'static str {
        "keyboard"
    }

    fn class(&self) -> DeviceClass {
        DeviceClass::Input
    }

    fn describe(&self) -> &'static str {
        "PS/2 keyboard (polled)"
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

impl InputDevice for KeyboardDevice {
    fn poll(&self) -> Option<Event> {
        let interrupts_were_enabled =
            crate::abi::input::disable_interrupts();
        let polled = unsafe {
            (&mut *crate::abi::display::KEYBOARD.get()).poll_char()
        };
        crate::abi::input::restore_interrupts(interrupts_were_enabled);
        polled.map(Event::Char)
    }
}

/// Wrapper em volta do driver de mouse (polling puro, sem estado próprio).
struct MouseDevice;

impl Device for MouseDevice {
    fn name(&self) -> &'static str {
        "mouse"
    }

    fn class(&self) -> DeviceClass {
        DeviceClass::Input
    }

    fn describe(&self) -> &'static str {
        "PS/2 mouse (polled)"
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

impl InputDevice for MouseDevice {
    fn poll(&self) -> Option<Event> {
        crate::mouse::poll().map(Event::Pointer)
    }
}

static KEYBOARD_DEVICE: KeyboardDevice = KeyboardDevice;
static MOUSE_DEVICE: MouseDevice = MouseDevice;

/// Publica teclado e mouse no registro genérico.
pub fn register_all() {
    crate::drivers::registry::register(&KEYBOARD_DEVICE);
    crate::drivers::registry::register(&MOUSE_DEVICE);
}
