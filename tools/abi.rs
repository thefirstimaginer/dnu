//! Include da ABI: agrupa `abi/imagineos_abi` no crate que o usa.
//!
//! As ferramentas de host (`dnu-pack`, `dnulink`) e os harnesses de teste são
//! compilados por `rustc` direto, sem cargo. O `.rlib` que o `build.rs` da ABI
//! publica contém só metadados (o crate não gera código) e o `rustc` não o
//! aceita como `--extern` de outro crate. O `tools/abi-bundle.sh` reúne
//! `lib.rs` e seus módulos em `.build/tools/imagineos_abi_bundle.rs`; os
//! consumidores declaravam `mod abi` com `#[path = "abi.rs"]` e o bundle entra
//! no crate deles. Depois disso o caminho é `crate::abi::imagineos_abi::...`.
//!
//! O bundle é regenerado pelo `make abi` antes de cada ferramenta.

include!(concat!(env!("OUT_DIR"), "/imagineos_abi_bundle.rs"));
