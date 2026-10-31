//! Formato e execução de imagens: parser DEF, loader e escalonador.
//!
//! `process` é o módulo do escalonador; `descriptors`, `loader` e `space` são
//! seus blocos de apoio. O DEF é o único formato executável de userspace; o
//! kernel em si continua sendo ELF apenas porque é o Limine quem o carrega.
//!
//! O DEF é definido neste repositório e produzido pelo `dnu-pack`/`dnulink`
//! (`tools/`); este carregador valida com a mesma [`def::validate_image`] que
//! os empacotadores usam.
//!
//! `src/` é uma árvore plana: cada módulo-hub declara suas folhas com
//! `#[path]` relativo a ele.

#[path = "exec_def.rs"]
pub mod def;
#[path = "exec_descriptors.rs"]
pub mod descriptors;
#[path = "exec_loader.rs"]
pub mod loader;
#[path = "exec_process.rs"]
pub mod process;
#[path = "exec_space.rs"]
pub mod space;
