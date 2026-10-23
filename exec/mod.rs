//! Formato e execução de imagens: parser DEF, loader e escalonador.
//!
//! `process` é o módulo do escalonador; `descriptors`, `loader` e `space` são
//! seus blocos de apoio. O DEF é o único formato executável de userspace; o
//! kernel em si continua sendo ELF apenas porque é o Limine quem o carrega.

#[path = "def.rs"]
pub mod def;
pub mod descriptors;
pub mod loader;
pub mod process;
pub mod space;
