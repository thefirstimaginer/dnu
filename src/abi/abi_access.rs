//! Avaliação de permissões POSIX para caminhos vindos do usuário.
//!
//! Bitmap usado pelos chamadores (`requested`):
//! `1` = executar/atravessar, `2` = escrever, `4` = ler, `5` = ler+executar
//! (listar diretório).
//!
//! As regras seguem o UNIX clássico: `root` passa por cima de tudo; um caminho
//! só está acessível se **todos** os diretórios ancestrais permitirem
//! travessia; e o nó final é avaliado com o próprio modo.

use imagineos_abi::MAX_MUTABLE_PATH;

use crate::process;

/// Verifica se um caminho contém componentes `..` que escapariam da raiz do
/// sistema de arquivos (path traversal). Barras duplas e `.` são tolerados.
fn has_path_traversal(path: &str) -> bool {
    let mut depth: isize = 0;
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                depth -= 1;
                if depth < 0 {
                    return true;
                }
            }
            _ => depth += 1,
        }
    }
    false
}

/// Remove componentes `.` e barras duplas de um caminho, copiando para `output`.
/// Devolve `None` se o caminho exceder `output.len()` ou tentar escapar da raiz.
fn normalize_path(path: &str, output: &mut [u8]) -> Option<usize> {
    if has_path_traversal(path) {
        return None;
    }
    let mut length = 0usize;
    for component in path.split('/') {
        if component.is_empty() || component == "." {
            continue;
        }
        if length != 0 {
            output.get_mut(length)?;
            output[length] = b'/';
            length += 1;
        }
        let end = length.checked_add(component.len())?;
        output.get_mut(length..end)?.copy_from_slice(component.as_bytes());
        length = end;
    }
    Some(length)
}

/// Verdadeiro quando o processo atual pode acessar `path` com `requested`.
pub fn allowed(path: &str, requested: u16) -> bool {
    let (uid, gid, _) = process::current_credentials();
    if uid == 0 {
        return true;
    }
    let mut normalized = [0u8; MAX_MUTABLE_PATH];
    let Some(normalized_len) = normalize_path(path, &mut normalized) else {
        return false;
    };
    let Ok(normalized) = core::str::from_utf8(&normalized[..normalized_len]) else {
        return false;
    };
    if !ancestors_searchable(normalized, uid, gid) {
        return false;
    }
    let Some(metadata) = crate::ramfs::metadata(normalized) else {
        return false;
    };
    crate::permissions::allows(
        metadata.mode,
        metadata.uid,
        metadata.gid,
        uid,
        gid,
        requested,
    )
}

/// Permissão sobre o diretório-pai de `path` (criação/remoção de entradas).
pub fn parent_allowed(path: &str, requested: u16) -> bool {
    let parent = path.rsplit_once('/').map_or("", |(parent, _)| parent);
    allowed(parent, requested)
}

/// Permissão sobre o diretório-pai de `path`, criando ancestrais ausentes.
///
/// É o que `mkdir -p` precisa: os ancestrais que já existem precisam permitir
/// travessia; os que serão criados assumem o dono atual, e o último existente
/// precisa permitir o bitmap pedido (normalmente escrita).
pub fn recursive_parent_allowed(path: &str, requested: u16) -> bool {
    if has_path_traversal(path) {
        return false;
    }
    let (uid, gid, _) = process::current_credentials();
    if uid == 0 {
        return true;
    }
    let parent = path.rsplit_once('/').map_or("", |(parent, _)| parent);
    let mut prefix = [0u8; MAX_MUTABLE_PATH];
    let mut length = 0usize;
    let mut last_existing = crate::ramfs::metadata("");
    for component in parent.trim_matches('/').split('/') {
        if component.is_empty() || component == "." {
            continue;
        }
        length = match push_component(&mut prefix, length, component.as_bytes()) {
            Some(length) => length,
            None => return false,
        };
        let current = core::str::from_utf8(&prefix[..length]).unwrap_or("");
        match crate::ramfs::metadata(current) {
            Some(metadata) if metadata.is_directory => {
                if !mode_allows(metadata, uid, gid, 1) {
                    return false;
                }
                last_existing = Some(metadata);
            }
            Some(_) => return false,
            None => break,
        }
    }
    last_existing.is_some_and(|metadata| mode_allows(metadata, uid, gid, requested))
}

fn ancestors_searchable(path: &str, uid: u32, gid: u32) -> bool {
    if has_path_traversal(path) {
        return false;
    }
    let mut prefix = [0u8; MAX_MUTABLE_PATH];
    let mut length = 0usize;
    let parent = path.rsplit_once('/').map_or("", |(parent, _)| parent);
    for component in parent
        .trim_matches('/')
        .split('/')
        .take_while(|part| !part.is_empty() && *part != ".")
    {
        length = match push_component(&mut prefix, length, component.as_bytes()) {
            Some(length) => length,
            None => return false,
        };
        let directory = core::str::from_utf8(&prefix[..length]).unwrap_or("");
        let searchable = crate::ramfs::metadata(directory)
            .is_some_and(|metadata| metadata.is_directory && mode_allows(metadata, uid, gid, 1));
        if !searchable {
            return false;
        }
    }
    true
}

/// Acrescenta `/component` a `prefix`, devolvendo o novo comprimento.
fn push_component(prefix: &mut [u8], length: usize, component: &[u8]) -> Option<usize> {
    let mut length = length;
    if length != 0 {
        *prefix.get_mut(length)? = b'/';
        length += 1;
    }
    let end = length.checked_add(component.len())?;
    prefix.get_mut(length..end)?.copy_from_slice(component);
    Some(end)
}

fn mode_allows(metadata: crate::dfs::Metadata, uid: u32, gid: u32, requested: u16) -> bool {
    crate::permissions::allows(
        metadata.mode,
        metadata.uid,
        metadata.gid,
        uid,
        gid,
        requested,
    )
}
