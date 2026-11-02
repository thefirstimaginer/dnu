//! Drivers de bloco: registro do disco no registro genérico.
//!
//! A implementação física (ATA PIO primary-master) vive em `ata.rs` com a
//! trait `BlockDevice` de `block.rs`. Este módulo é a ponte entre o driver
//! concreto e o registro central: publica `hda` como [`Device`] de classe
//! `Block` e expõe o disco via [`disk()`] para quem não quer downcast.
//!
//! Com o registro, adicionar um segundo disco (SATA, NVMe) é implementar a
//! trait e registrar outro nome — consumidores como GPT e DFS continuam
//! iguais, pedindo dispositivos pelo nome.

use crate::drivers::{Device, DeviceClass};
use crate::{ata, block};

/// Wrapper que publica o disco ATA no registro genérico.
struct AtaDisk;

impl Device for AtaDisk {
    fn name(&self) -> &'static str {
        "hda"
    }

    fn class(&self) -> DeviceClass {
        DeviceClass::Block
    }

    fn describe(&self) -> &'static str {
        "ATA primary master (PIO)"
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

static ATA_DISK: AtaDisk = AtaDisk;

/// Inicializa o disco e o publica no registro.
///
/// Devolve o número de setores quando o disco responde; `None` quando não há
/// disco utilizável (o boot continua, só sem raiz persistente).
pub fn init() -> Option<u64> {
    match ata::init() {
        Ok(sectors) => {
            crate::drivers::registry::register(&ATA_DISK);
            Some(sectors)
        }
        Err(_) => None,
    }
}

/// Disco primário como trait de bloco, para GPT/DFS/installer.
pub fn disk() -> Option<&'static dyn block::BlockDevice> {
    ata::is_ready().then(|| &ata::PrimaryMaster as &'static dyn block::BlockDevice)
}

/// Verdadeiro quando o disco registrado está pronto para E/S.
pub fn disk_ready() -> bool {
    ata::is_ready()
}
