//! Syscalls de display e entrada.
//!
//! O servidor de display roda em ring 3: ele recebe a descrição do framebuffer,
//! mapeia a memória de vídeo, captura o console e passa a receber teclado e
//! mouse pela fila de eventos ([`crate::input`]). Superfícies permitem que
//! processos clientes registrem buffers que o servidor compõe na tela.

use core::cell::UnsafeCell;

use imagineos_abi::{
    FramebufferInfo, InputEvent, SurfaceEvent, SurfaceInfo, SurfaceSnapshot, MAX_SURFACES,
};

use super::trap::{TrapFrame, EFAULT, EINVAL, ENODEV, ENOENT, ENOMEM};
use super::user::{copy_struct_from_user, copy_struct_to_user};
use crate::process;
use crate::keyboard::Keyboard;

// A porta PS/2 não tem IRQ própria e é lida em modo polling; o estado do
// teclado (shift, dead keys, prefixo E0) é um único objeto global. A exclusão
// mútua é feita desabilitando interrupções durante a leitura (ver `input`).
pub(super) struct SharedKeyboard(UnsafeCell<Keyboard>);
unsafe impl Sync for SharedKeyboard {}
pub(super) static KEYBOARD: SharedKeyboard =
    SharedKeyboard(UnsafeCell::new(Keyboard::new()));

impl SharedKeyboard {
    /// Estado global do teclado; o chamador deve desabilitar interrupções.
    pub fn get(&self) -> *mut Keyboard {
        self.0.get()
    }
}

/// Base virtual onde o framebuffer é mapeado; fica na área alta de usuário,
/// livre entre a pilha e o fim do espaço.
const FB_BASE: u64 = 0x0000_4000_0000_0000;

/// `fb_info(&FramebufferInfo)`: descreve o framebuffer sem mapeá-lo.
pub fn framebuffer_info(frame: &mut TrapFrame) -> *mut TrapFrame {
    let Some(description) = crate::framebuffer::description() else {
        return frame.fail(ENODEV);
    };
    let info = FramebufferInfo {
        address: 0,
        ..describe_framebuffer(&description)
    };
    if copy_struct_to_user(frame.rdi, &info) {
        frame.ok(0)
    } else {
        frame.fail(EFAULT)
    }
}

/// `fb_map(&FramebufferInfo)`: mapeia a memória de vídeo e preenche `address`.
pub fn framebuffer_map(frame: &mut TrapFrame) -> *mut TrapFrame {
    let Some(description) = crate::framebuffer::description() else {
        return frame.fail(ENODEV);
    };
    let length = description.pitch.saturating_mul(description.height);
    let mapped = process::with_current_space(|space| {
        space.map_framebuffer_range(FB_BASE, description.physical_address, length)
    });
    let Some(address) = mapped else {
        return frame.fail(ENOMEM);
    };
    let info = FramebufferInfo {
        address,
        ..describe_framebuffer(&description)
    };
    if copy_struct_to_user(frame.rdi, &info) {
        // Restringe o ponteiro à área visível: movimentos acumulados antes do
        // mapeamento poderiam deixar o cursor fora da tela.
        crate::input::clamp_pointer(description.width as i32, description.height as i32);
        frame.ok(0)
    } else {
        frame.fail(EFAULT)
    }
}

fn describe_framebuffer(description: &crate::framebuffer::Description) -> FramebufferInfo {
    FramebufferInfo {
        address: 0,
        physical_address: description.physical_address,
        width: description.width as u32,
        height: description.height as u32,
        pitch: description.pitch as u32,
        red_shift: description.red_shift,
        green_shift: description.green_shift,
        blue_shift: description.blue_shift,
        reserved: 0,
    }
}

/// `input_poll(&InputEvent)`: devolve 1 com evento, 0 com a fila vazia.
///
/// Quando o servidor captura o console, esta chamada também drena o teclado
/// PS/2 e o mouse; fora do modo display, o teclado é lido pelo caminho
/// `SYS_READ`.
pub fn input_poll(frame: &mut TrapFrame) -> *mut TrapFrame {
    if crate::input::is_captured() {
        drain_keyboard();
    }
    drain_mouse();
    let Some(event) = crate::input::poll() else {
        return frame.ok(0);
    };
    if process::copy_to_current_user(
        frame.rdi,
        unsafe {
            core::slice::from_raw_parts(
                (&event as *const InputEvent).cast::<u8>(),
                core::mem::size_of::<InputEvent>(),
            )
        },
    ) {
        frame.ok(1)
    } else {
        frame.fail(EFAULT)
    }
}

fn drain_keyboard() {
    for _ in 0..16 {
        let interrupts = super::input::disable_interrupts();
        let key = unsafe { (&mut *KEYBOARD.get()).poll_char() };
        super::input::restore_interrupts(interrupts);
        let Some(key) = key else { break };
        crate::input::push_key(key);
    }
}

fn drain_mouse() {
    // `mouse::poll` acumula todos os pacotes e `push_pointer` coalesce o
    // movimento, de modo que o WM recebe a posição mais recente.
    while let Some(report) = crate::mouse::poll() {
        if report.delta_x != 0 || report.delta_y != 0 {
            crate::input::push_pointer(report.delta_x, report.delta_y);
        }
        crate::input::push_pointer_button(1, report.left);
        crate::input::push_pointer_button(2, report.right);
    }
}

/// `pointer_position()`: devolve `(x << 32) | y`.
pub fn pointer_position(frame: &mut TrapFrame) -> *mut TrapFrame {
    let (x, y) = crate::input::pointer();
    frame.ok(((x as u32 as u64) << 32) | (y as u32 as u64))
}

/// `console_release()`: o servidor de display assume a tela e a entrada.
pub fn console_release(frame: &mut TrapFrame) -> *mut TrapFrame {
    crate::framebuffer::set_active(false);
    crate::input::capture();
    crate::surface::set_server();
    frame.ok(0)
}

// ---------------------------------------------------------------------------
// Superfícies
// ---------------------------------------------------------------------------

/// `surface_create(&SurfaceInfo)`: registra a superfície e mapeia o buffer.
pub fn surface_create(frame: &mut TrapFrame) -> *mut TrapFrame {
    let mut info = SurfaceInfo::default();
    if !copy_struct_from_user(frame.rdi, &mut info) {
        return frame.fail(EFAULT);
    }
    match crate::surface::create(&mut info) {
        Ok(()) => {
            if copy_struct_to_user(frame.rdi, &info) {
                frame.ok(0)
            } else {
                frame.fail(EFAULT)
            }
        }
        Err(code) => frame.fail(code),
    }
}

/// `surface_present(id, shown)`.
pub fn surface_present(frame: &mut TrapFrame) -> *mut TrapFrame {
    surface_result(frame, crate::surface::present(frame.rdi as u32, frame.rsi != 0))
}

/// `surface_flush(id)`: publica o frame escrito no buffer.
pub fn surface_flush(frame: &mut TrapFrame) -> *mut TrapFrame {
    surface_result(frame, crate::surface::flush(frame.rdi as u32))
}

/// `surface_destroy(id)`.
pub fn surface_destroy(frame: &mut TrapFrame) -> *mut TrapFrame {
    surface_result(frame, crate::surface::destroy(frame.rdi as u32))
}

/// `surface_move(id, x, y)`: o dono reposiciona a própria superfície.
pub fn surface_move(frame: &mut TrapFrame) -> *mut TrapFrame {
    surface_result(
        frame,
        crate::surface::move_surface(frame.rdi as u32, frame.rsi as i32, frame.rdx as i32),
    )
}

fn surface_result(frame: &mut TrapFrame, result: Result<(), i64>) -> *mut TrapFrame {
    match result {
        Ok(()) => frame.ok(0),
        Err(code) => frame.fail(code),
    }
}

/// `surface_poll(&SurfaceEvent)`: devolve 1 com evento, 0 sem eventos.
pub fn surface_poll(frame: &mut TrapFrame) -> *mut TrapFrame {
    let Some(event) = crate::surface::pop_event() else {
        return frame.ok(0);
    };
    if process::copy_to_current_user(
        frame.rdi,
        unsafe {
            core::slice::from_raw_parts(
                (&event as *const SurfaceEvent).cast::<u8>(),
                core::mem::size_of::<SurfaceEvent>(),
            )
        },
    ) {
        frame.ok(1)
    } else {
        frame.fail(EFAULT)
    }
}

/// `surface_snapshot(&[SurfaceSnapshot], max)`: lista as superfícies visíveis e
/// mapeia os buffers no processo do servidor.
pub fn surface_snapshot(frame: &mut TrapFrame) -> *mut TrapFrame {
    let capacity = (frame.rsi as usize).min(MAX_SURFACES);
    if frame.rdi == 0 || capacity == 0 {
        return frame.fail(EINVAL);
    }
    let mut output = [SurfaceSnapshot::default(); MAX_SURFACES];
    // Consome a fila de frames novos: sem isto o servidor redesenha
    // superfícies antigas a cada quadro e a fila satura.
    while crate::surface::take_dirty().is_some() {}
    let count = crate::surface::snapshot_mapped(&mut output[..capacity]);
    let bytes = unsafe {
        core::slice::from_raw_parts(
            output.as_ptr().cast::<u8>(),
            count * core::mem::size_of::<SurfaceSnapshot>(),
        )
    };
    if process::copy_to_current_user(frame.rdi, bytes) {
        frame.ok(count as u64)
    } else {
        frame.fail(EFAULT)
    }
}

/// `surface_send_event(&SurfaceEvent)`: entrega um evento ao dono.
pub fn surface_send_event(frame: &mut TrapFrame) -> *mut TrapFrame {
    let mut event = SurfaceEvent::default();
    if !copy_struct_from_user(frame.rdi, &mut event) {
        return frame.fail(EFAULT);
    }
    if crate::surface::push_event_to_surface(event.surface, event) {
        frame.ok(0)
    } else {
        frame.fail(ENOENT)
    }
}
