use imagineos_abi::{InstallConfig, ACCOUNT_NAME_SIZE, ACCOUNT_PASSWORD_SIZE, HOSTNAME_SIZE};

use crate::dfs;

const DATABASE_MAGIC: &[u8; 8] = b"IMUSER01";
// Fixed 100-byte record: header, UID/GID, salt, password hash, then a 32-byte name.
const DATABASE_SIZE: usize = 100;
const PBKDF2_ITERATIONS: u32 = 10_000;
// Chave usada para "envolver" a base de dados na RAMFS (evita que seja
// acidentalmente sobrescrita por outro módulo).
const DB_FILENAME: &str = "/etc/users.db";

#[derive(Clone, Copy)]
pub struct Account {
    /// Nome de usuário (NUL-terminado, comprimento real em `username_length`).
    pub username: [u8; ACCOUNT_NAME_SIZE],
    pub username_length: usize,
    /// ID do usuário (0 = root).
    pub uid: u32,
    /// ID do grupo.
    pub gid: u32,
    /// Se o usuário tem privilégios de administrador.
    pub administrator: bool,
    /// Semente para derivação de chave (16 bytes, aleatória).
    pub salt: [u8; 16],
    /// Hash da senha (32 bytes, PBKDF2-SHA256 com `PBKDF2_ITERATIONS` iterações).
    pub password_hash: [u8; 32],
}

impl Account {
    /// Constrói uma conta root com hash vazio (não autenticável até que uma
    /// senha real seja definida pelo instalador ou via syscall de troca).
    #[allow(dead_code)]
    fn root() -> Self {
        let mut username = [0; ACCOUNT_NAME_SIZE];
        username[..4].copy_from_slice(b"root");
        Self {
            username,
            username_length: 4,
            uid: 0,
            gid: 0,
            administrator: true,
            salt: [0; 16],
            password_hash: [0; 32], // Hash vazio: nenhuma senha é aceita até ser definida.
        }
    }

    /// Verifica se a senha corresponde ao hash armazenado (constante-tempo).
    ///
    /// Não existe fallback de senha literal: apenas o hash PBKDF2-SHA256 armazenado
    /// no banco é considerado.
    fn verify_password(&self, password: &[u8]) -> bool {
        crate::usr::crypto::pbkdf2_sha256(password, &self.salt, PBKDF2_ITERATIONS)
            .is_some_and(|candidate| constant_time_eq(&candidate, &self.password_hash))
    }

    /// Tenta mudar a senha da conta, desde que a senha atual seja conhecida.
    ///
    /// # Arguments
    ///
    /// * `current_password` - A senha atual (em texto puro, será hasheada internamente).
    /// * `new_password` - A nova senha em texto puro.
    ///
    /// Returns `Ok(())` se a senha foi alterada com sucesso, ou `Err(())` se a
    /// senha atual estiver errada ou os parâmetros forem inválidos.
#[allow(dead_code)] // syscall de troca de senha ainda não exposta na ABI.
    fn change_password(&mut self, current_password: &[u8], new_password: &[u8]) -> Result<(), ()> {
        // Verifica a senha atual.
        if !self.verify_password(current_password) {
            return Err(());
        }
        // Valida a nova senha (não vazia, sem NULs internos).
        if new_password.is_empty()
            || new_password.contains(&0)
            || new_password.len() > ACCOUNT_PASSWORD_SIZE
        {
            return Err(());
        }
        // Gera uma nova semente e computed o novo hash.
        let mut new_salt = [0u8; 16];
        fill_random(&mut new_salt).map_err(|_| ())?;
        let new_hash = crate::usr::crypto::pbkdf2_sha256(new_password, &new_salt, PBKDF2_ITERATIONS).ok_or(())?;
        // Atualiza a conta.
        self.salt = new_salt;
        self.password_hash = new_hash;
        Ok(())
    }
}

pub fn validate_install_config(config: &InstallConfig) -> Result<(), ()> {
    if config.hostname_length as usize > HOSTNAME_SIZE
        || config.username_length as usize > ACCOUNT_NAME_SIZE
        || config.password_length as usize > ACCOUNT_PASSWORD_SIZE
        || config.add_user > 1
        || config.administrator > 1
    {
        return Err(());
    }
    let hostname = &config.hostname[..config.hostname_length as usize];
    if !hostname.is_empty()
        && (!hostname
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'.'))
            || hostname[0] == b'-'
            || hostname[0] == b'.')
    {
        return Err(());
    }
    if config.add_user == 0 {
        return if config.username_length == 0
            && config.password_length == 0
            && config.administrator == 0
        {
            Ok(())
        } else {
            Err(())
        };
    }
    let username = &config.username[..config.username_length as usize];
    let password = &config.password[..config.password_length as usize];
    if username.is_empty()
        || username == b"root"
        || !username.iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(*byte, b'-' | b'_')
        })
        || password.is_empty()
        || password.contains(&0)
    {
        return Err(());
    }
    Ok(())
}

pub fn make_database(config: &InstallConfig) -> Result<[u8; DATABASE_SIZE], ()> {
    validate_install_config(config)?;
    if config.add_user == 0 {
        return Err(());
    }
    // Para a conta root, permite uid/gid = 0 (primeiro boot).
    let is_root = config.username[..config.username_length as usize] == *b"root";
    let uid: u32 = if is_root { 0 } else { 1000 };
    let gid: u32 = if is_root { 0 } else { 1000 };
    let mut salt = [0u8; 16];
    fill_random(&mut salt)?;
    let password = &config.password[..config.password_length as usize];
    let hash = crate::usr::crypto::pbkdf2_sha256(password, &salt, PBKDF2_ITERATIONS).ok_or(())?;
    let mut output = [0u8; DATABASE_SIZE];
    output[..8].copy_from_slice(DATABASE_MAGIC);
    output[8] = config.username_length;
    output[9] = config.administrator;
    output[12..16].copy_from_slice(&uid.to_le_bytes());
    output[16..20].copy_from_slice(&gid.to_le_bytes());
    output[20..36].copy_from_slice(&salt);
    output[36..68].copy_from_slice(&hash);
    output[68..68 + config.username_length as usize]
        .copy_from_slice(&config.username[..config.username_length as usize]);
    Ok(output)
}

pub fn load_database() -> Option<[u8; DATABASE_SIZE]> {
    // Try RAMFS first (fast, already set up during boot).
    let from_ramfs = crate::ramfs::read(DB_FILENAME).and_then(|bytes| {
        let bytes = bytes.get(..DATABASE_SIZE)?;
        let mut output = [0u8; DATABASE_SIZE];
        output.copy_from_slice(bytes);
        Some(output)
    });
    if let Some(bytes) = from_ramfs {
        // Also persist to DFS if not already there (lazy init).
        if !crate::vfs::has_persistent_root() || dfs::read_static(DB_FILENAME).is_none() {
            let _ = save_database(&bytes);
        }
        return Some(bytes);
    }
    // Fall back to DFS if RAMFS doesn't have it (e.g., after pivot_root).
    if crate::vfs::has_persistent_root() {
        if let Some(dfs_data) = dfs::read_static(DB_FILENAME) {
            let bytes = dfs_data.get(..DATABASE_SIZE)?;
            let mut output = [0u8; DATABASE_SIZE];
            output.copy_from_slice(bytes);
            // Persist to RAMFS for faster future access.
            let _ = save_database(&output);
            return Some(output);
        }
    }
    None
}

#[allow(dead_code)]
pub fn init_accounts() {
    // Tenta carregar a base de usuários de DFS para RAMFS.
    // Isso garante que as credenciais estejam disponíveis após pivot_root
    // ou em bootes subsequentes.
    let _ = load_database();
}

pub fn save_database(data: &[u8; DATABASE_SIZE]) -> Result<(), ()> {
    // Save to RAMFS (for current session). O modo é aplicado pelo VFS no
    // primeiro create; a RAMFS grava no overlay por uid/gid.
    crate::ramfs::write_file_as(DB_FILENAME, data, 0, 0).map_err(|_| ())?;
    // Also try to save to DFS (for persistence across reboots).
    if crate::vfs::has_persistent_root() {
        dfs::write_file_as(DB_FILENAME, data, 0o600, 0, 0).map_err(|_| ())?;
    }
    Ok(())
}

pub fn authenticate(username: &[u8], password: &[u8]) -> Option<Account> {
    // Autenticação exclusivamente contra o banco de usuários persistente.
    // Não existe backdoor de senha literal: a primeira conta deve ser criada
    // pelo instalador (`distroinstall`) ou via syscall de troca de senha.
    let bytes = load_database()?;
    let account = decode_account(&bytes)?;
    (username == &account.username[..account.username_length] && account.verify_password(password))
        .then_some(account)
}

fn decode_account(bytes: &[u8]) -> Option<Account> {
    if bytes.len() != DATABASE_SIZE || &bytes[..8] != DATABASE_MAGIC {
        return None;
    }
    let username_length = bytes[8] as usize;
    if username_length == 0 || username_length > ACCOUNT_NAME_SIZE {
        return None;
    }
    // Permitir uid/gid = 0 apenas para a conta root (primeiro boot).
    let uid = u32::from_le_bytes(bytes[12..16].try_into().ok()?);
    let gid = u32::from_le_bytes(bytes[16..20].try_into().ok()?);
    // Se não for root (uid == 0), exigir uid/gid >= 1000.
    if uid != 0 && (uid < 1000 || gid < 1000) {
        return None;
    }
    let mut username = [0; ACCOUNT_NAME_SIZE];
    username.copy_from_slice(&bytes[68..100]);
    if username[username_length..].iter().any(|byte| *byte != 0) {
        return None;
    }
    let mut salt = [0; 16];
    salt.copy_from_slice(&bytes[20..36]);
    let mut password_hash = [0; 32];
    password_hash.copy_from_slice(&bytes[36..68]);
    Some(Account {
        username,
        username_length,
        uid,
        gid,
        administrator: bytes[9] != 0,
        salt,
        password_hash,
    })
}

fn fill_random(output: &mut [u8]) -> Result<(), ()> {
    if core::arch::x86_64::__cpuid(1).ecx & (1 << 30) == 0 {
        return Err(());
    }
    for chunk in output.chunks_mut(8) {
        let mut random = 0u64;
        let mut available: u8;
        unsafe {
            core::arch::asm!(
                "rdrand {random}",
                "setc {available}",
                random = out(reg) random,
                available = out(reg_byte) available,
                options(nomem, nostack)
            );
        }
        if available == 0 {
            return Err(());
        }
        let bytes = random.to_le_bytes();
        chunk.copy_from_slice(&bytes[..chunk.len()]);
    }
    Ok(())
}

fn constant_time_eq(left: &[u8; 32], right: &[u8; 32]) -> bool {
    left.iter()
        .zip(right)
        .fold(0u8, |difference, (left, right)| difference | (left ^ right))
        == 0
}