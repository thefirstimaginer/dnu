//! Hub de drivers: traits genéricas e registro central de dispositivos.
//!
//! O ImagineOS não usa daemons de drivers em userspace (o ecossistema herdado
//! do Redox foi removido). Os drivers são módulos nativos do kernel, cada um
//! atrás de uma trait genérica ([`Device`]) e publicados num registro central
//! ([`registry`]) consultável por nome, classe e por quem for usar (`/dev`).
//!
//! A árvore `src/` é plana: cada driver é um arquivo com a implementação
//! concreta (ata, keyboard, mouse, timer, framebuffer) e este hub declara as
//! folhas com `#[path]` relativo a ele.

#[path = "drv_registry.rs"]
pub mod registry;

pub use registry::{Device, DeviceClass};
#[path = "drv_block.rs"]
pub mod block;
#[path = "drv_input.rs"]
pub mod input;
#[path = "drv_video.rs"]
pub mod video;
#[path = "drv_platform.rs"]
pub mod platform;
