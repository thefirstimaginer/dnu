//! Registro central de dispositivos do kernel.
//!
//! Cada driver, ao concluir a inicialização, publica seu dispositivo aqui
//! com um nome único (`/dev/<nome>`), uma classe ([`DeviceClass`]) e um
//! ponteiro para a interface genérica ([`Device`]). O resto do kernel
//! (ramfs/VFS para os nós de `/dev`, syscalls de entrada, display server)
//! consulta este registro em vez de referenciar drivers diretamente.
//!
//! Boot é single-thread e o registro é escrito só durante o bring-up; depois
//! ele é somente leitura. O tamanho é fixo (sem heap) e um registro cheio é
//! tratado como erro de configuração do kernel, não pânico.
//!
//! Antes deste módulo, cada consumidor referenciava o driver concreto
//! (`crate::ata`, `crate::mouse`), o que impedia adicionar um segundo disco
//! ou um segundo teclado sem tocar em cada ponto de uso.

use core::sync::atomic::{AtomicUsize, Ordering};

/// Classe funcional de um dispositivo — o que o consumidor pode fazer com ele.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DeviceClass {
    /// Armazenamento em blocos (discos).
    Block,
    /// Entrada de usuário (teclado, mouse, apontadores).
    Input,
    /// Saída de vídeo (framebuffers).
    Video,
    /// Serviços de plataforma (ACPI, relógio, desligamento).
    Platform,
}

impl DeviceClass {
    /// Nome da classe como aparece no registro e nos logs de boot.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::Input => "input",
            Self::Video => "video",
            Self::Platform => "platform",
        }
    }
}

/// Interface genérica que todo driver publica no registro.
///
/// Cada classe especializa [`Device`] por meio de downcast seguro
/// (`as_any`), porque o registro é uma lista homogênea sem heap.
pub trait Device {
    /// Nome único do dispositivo (o mesmo usado em `/dev`).
    fn name(&self) -> &'static str;
    /// Classe funcional.
    fn class(&self) -> DeviceClass;
    /// Linha descritiva para o relatório de boot.
    fn describe(&self) -> &'static str;
    /// Downcast para `Any` — permite recuperar a trait específica da classe.
    fn as_any(&self) -> &dyn core::any::Any;
}

/// Capacidade máxima do registro: mais dispositivos do que isso indicaria
/// dispositivos dinâmicos (hot-plug), que ainda não existem no bring-up.
pub const MAX_DEVICES: usize = 16;

struct Entry {
    device: Option<&'static (dyn Device + Sync)>,
}

static ENTRIES: [Entry; MAX_DEVICES] = [const { Entry { device: None } }; MAX_DEVICES];
static COUNT: AtomicUsize = AtomicUsize::new(0);

/// Registra um dispositivo. Falha (com `false`) quando o nome já existe ou o
/// registro está cheio — o chamador decide se isso é fatal no boot.
pub fn register(device: &'static (dyn Device + Sync)) -> bool {
    // Nome duplicado é erro de configuração: primeiro verifica.
    let name = device.name();
    for index in 0..COUNT.load(Ordering::Relaxed) {
        if let Some(existing) = ENTRIES[index].device {
            if existing.name() == name {
                return false;
            }
        }
    }
    let slot = COUNT.fetch_add(1, Ordering::Relaxed);
    if slot >= MAX_DEVICES {
        // Registro cheio: desfaz o incremento lógico. `COUNT` fica além do
        // limite, mas `iter()` só lê slots válidos.
        return false;
    }
    // Escrita exclusiva: o boot é single-thread e o registro só é escrito
    // durante o bring-up (ver doc do módulo).
    unsafe {
        let entries = &mut *(core::ptr::addr_of!(ENTRIES)
            as *const [Entry; MAX_DEVICES] as *mut [Entry; MAX_DEVICES]);
        entries[slot].device = Some(device);
    }
    true
}

/// Número de dispositivos registrados.
pub fn count() -> usize {
    COUNT.load(Ordering::Relaxed).min(MAX_DEVICES)
}

/// Itera sobre os dispositivos registrados.
pub fn iter() -> impl Iterator<Item = &'static (dyn Device + Sync)> {
    let total = count();
    ENTRIES
        .iter()
        .take(total)
        .filter_map(|entry| entry.device)
}

/// Procura um dispositivo pelo nome (o mesmo de `/dev/<nome>`).
pub fn find(name: &str) -> Option<&'static (dyn Device + Sync)> {
    iter().find(|device| device.name() == name)
}

/// Primeiro dispositivo de uma classe — o padrão para consumidores que querem
/// "o" disco ou "o" teclado (sem hot-plug há no máximo um de cada).
pub fn first_of_class(class: DeviceClass) -> Option<&'static (dyn Device + Sync)> {
    iter().find(|device| device.class() == class)
}

/// Escreve a tabela de dispositivos no console de boot.
pub fn report() {
    crate::console_write("drivers: device registry\n");
    let mut any = false;
    for device in iter() {
        any = true;
        crate::console_write("  /dev/");
        crate::console_write(device.name());
        crate::console_write("  [");
        crate::console_write(device.class().name());
        crate::console_write("] ");
        crate::console_write(device.describe());
        crate::console_write("\n");
    }
    if !any {
        crate::console_write("  (none)\n");
    }
}
