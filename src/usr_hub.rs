//! Estado de espaço de usuário do kernel: contas, configuração global e
//! primitivas criptográficas usadas pela autenticação.
//!
//! Estes módulos não são drivers nem subsistemas de arquitetura; agrupá-los aqui
//! deixa explícito que servem ao contrato com o userspace (login, `global.conf`,
//! hashing de senha) e não ao núcleo de boot/memória/interrupções.
#[path = "accounts.rs"]
pub mod accounts;
#[path = "config.rs"]
pub mod config;
#[path = "crypto.rs"]
pub mod crypto;
