//! Drivers de vídeo: framebuffer no registro genérico.
//!
//! O framebuffer configurado pelo Limine é gerido por `console/framebuffer.rs`
//! (texto e superfícies). Este módulo publica o dispositivo de vídeo no
//! registro central para que o display server (userspace) e as syscalls de
//! superfície encontrem o hardware de saída pelo registro, não por caminho
//! direto ao módulo do console.

use crate::drivers::{Device, DeviceClass};

struct Framebuffer;

impl Device for Framebuffer {
    fn name(&self) -> &'static str {
        "fb0"
    }

    fn class(&self) -> DeviceClass {
        DeviceClass::Video
    }

    fn describe(&self) -> &'static str {
        "Limine GOP linear framebuffer"
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

static FB0: Framebuffer = Framebuffer;

/// Publica o framebuffer no registro (após `framebuffer::init`).
pub fn register() {
    crate::drivers::registry::register(&FB0);
}
