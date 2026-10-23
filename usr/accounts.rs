use imagineos_abi::{InstallConfig, ACCOUNT_NAME_SIZE, ACCOUNT_PASSWORD_SIZE, HOSTNAME_SIZE};

const DATABASE_MAGIC: &[u8; 8] = b"IMUSER01";
// Fixed 100-byte record: header, UID/GID, salt, password hash, then a 32-byte name.
const DATABASE_SIZE: usize = 100;
const PBKDF2_ITERATIONS: u32 = 10_000;

#[derive(Clone, Copy)]
pub struct Account {
    pub username: [u8; ACCOUNT_NAME_SIZE],
    pub username_length: usize,
    pub uid: u32,
    pub gid: u32,
    pub administrator: bool,
    salt: [u8; 16],
    password_hash: [u8; 32],
}

impl Account {
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
            password_hash: [0; 32],
        }
    }

    fn verify_password(&self, password: &[u8]) -> bool {
        if self.uid == 0 {
            return password == b"root";
        }
        crate::usr::crypto::pbkdf2_sha256(password, &self.salt, PBKDF2_ITERATIONS)
            .is_some_and(|candidate| constant_time_eq(&candidate, &self.password_hash))
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

pub fn authenticate(username: &[u8], password: &[u8]) -> Option<Account> {
    if username == b"root" {
        return (password == b"root").then(Account::root);
    }
    let bytes = crate::ramfs::read("/etc/users.db")?;
    let account = decode_account(bytes)?;
    (username == &account.username[..account.username_length] && account.verify_password(password))
        .then_some(account)
}

pub fn make_database(config: &InstallConfig) -> Result<[u8; DATABASE_SIZE], ()> {
    validate_install_config(config)?;
    if config.add_user == 0 {
        return Err(());
    }
    let mut salt = [0u8; 16];
    fill_random(&mut salt)?;
    let password = &config.password[..config.password_length as usize];
    let hash = crate::usr::crypto::pbkdf2_sha256(password, &salt, PBKDF2_ITERATIONS).ok_or(())?;
    let mut output = [0u8; DATABASE_SIZE];
    output[..8].copy_from_slice(DATABASE_MAGIC);
    output[8] = config.username_length;
    output[9] = config.administrator;
    output[12..16].copy_from_slice(&1000u32.to_le_bytes());
    output[16..20].copy_from_slice(&1000u32.to_le_bytes());
    output[20..36].copy_from_slice(&salt);
    output[36..68].copy_from_slice(&hash);
    output[68..68 + config.username_length as usize]
        .copy_from_slice(&config.username[..config.username_length as usize]);
    Ok(output)
}

pub fn hostname(config: &InstallConfig) -> &[u8] {
    let length = config.hostname_length as usize;
    if length == 0 {
        b"imagineos"
    } else {
        &config.hostname[..length]
    }
}

fn decode_account(bytes: &[u8]) -> Option<Account> {
    if bytes.len() != DATABASE_SIZE || &bytes[..8] != DATABASE_MAGIC {
        return None;
    }
    let username_length = bytes[8] as usize;
    if username_length == 0 || username_length > ACCOUNT_NAME_SIZE || bytes[9] > 1 {
        return None;
    }
    let uid = u32::from_le_bytes(bytes[12..16].try_into().ok()?);
    let gid = u32::from_le_bytes(bytes[16..20].try_into().ok()?);
    if uid < 1000 || gid < 1000 {
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
