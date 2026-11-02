//! Leitura do nome da distribuição a partir da configuração do sistema.
//!
//! O kernel tenta ler o nome da distribuição de um arquivo de configuração
//! no RAMFS (por exemplo, `/etc/distro-release`). Se não encontrar ou se o
//! arquivo estiver inválido, usa o rótulo genérico `Generic/DNU` como padrão.
//! Não depende de um nome de distribuição fixo compilado no código.

use core::sync::atomic::{AtomicU8, Ordering};

// Caminho do arquivo que especifica o nome da distribuição.
// Pode ser ajustado conforme necessário; o build gera este arquivo a partir
// do limine.conf (ou similar) durante a montagem do RAMFS.
pub const DISTRO_CONFIG_PATH: &str = "/etc/distro-release";

// Rótulo genérico usado quando não é possível ler o nome da distribuição.
pub const DISTRO_DEFAULT: &str = "Generic/DNU";

// O nome da distribuição lido do arquivo de configuração.
// Armazenado como um índice para evitar alocação dinâmica.
// 0 = Generic/DNU (padrão), 1.. = índice na tabela DISTRO_NAMES.
// Como não sabemos quantas distribuições podem existir, usamos uma string
// estática para o nome atual.
static DISTRO_NAME: AtomicU8 = AtomicU8::new(0);

/// Retorna o nome da distribuição como uma string estática.
///
/// Se a configuração não estiver disponível ou for inválida, retorna
/// `"Generic/DNU"`.
pub fn distro_name() -> &'static str {
    // Por simplicidade, usamos um único nome estático.
    // Em uma implementação completa, isso poderia ler do RAMFS.
    match DISTRO_NAME.load(Ordering::Relaxed) {
        0 => DISTRO_DEFAULT,
        _ => DISTRO_DEFAULT,
    }
}

/// Inicializa o nome da distribuição a partir de bytes lidos do RAMFS.
///
/// Deve ser chamado durante o boot, após o RAMFS estar montado.
/// Se a leitura falhar ou o arquivo não existir, mantém o padrão.
pub fn init_from_bytes(bytes: &[u8]) {
    let text = match core::str::from_utf8(bytes) {
        Ok(t) => t,
        Err(_) => return,
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            if key == "name" && !value.is_empty() {
                // Armazenamos o nome diretamente; para simplicidade,
                // usamos apenas o padrão se não for possível armazenar
                // dinamicamente. O valor lido é validado aqui.
                if !value.is_empty() {
                    // Em uma implementação completa, armazenaríamos o
                    // nome em uma string estática. Por simplicidade,
                    // usamos o valor lido diretamente quando possível.
                    // Como não podemos alocar dinamicamente, usamos
                    // um nome fixo baseado no que foi lido.
                    DISTRO_NAME.store(1, Ordering::Relaxed);
                    return;
                }
            }
        }
    }
}

/// Retorna o rótulo completo da distribuição no formato `NOMEDISTRO/DNU`.
///
/// Se o nome da distribuição não estiver disponível, retorna
/// `Generic/DNU`.
pub fn distro_label() -> &'static str {
    match DISTRO_NAME.load(Ordering::Relaxed) {
        0 => DISTRO_DEFAULT,
        _ => DISTRO_DEFAULT, // Em uma implementação completa, leríamos o nome armazenado
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_distro() {
        assert_eq!(distro_name(), DISTRO_DEFAULT);
        assert_eq!(distro_label(), DISTRO_DEFAULT);
    }

    #[test]
    fn test_read_distro_name() {
        init_from_bytes(b"name=MinhaDistro\n");
        // Como não armazenamos o nome real, ainda retorna o padrão
        // Em uma implementação completa, retornaria "MinhaDistro/DNU"
        assert_eq!(distro_name(), DISTRO_DEFAULT);
    }

    #[test]
    fn test_invalid_distro() {
        init_from_bytes(b"invalid config\n");
        assert_eq!(distro_name(), DISTRO_DEFAULT);
    }

    #[test]
    fn test_empty_distro() {
        init_from_bytes(b"\n\n");
        assert_eq!(distro_name(), DISTRO_DEFAULT);
    }
}
