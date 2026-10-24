//! Куда и с чем подключаться: адрес и расшифрованные данные входа.

use std::path::PathBuf;

use crate::error::CoreResult;
use crate::repo::conn::{AuthMethod, Conn};
use crate::secret::Secret;
use crate::store::Store;

#[derive(Debug, Clone)]
pub enum KeySource {
    File(PathBuf),
    Text(Secret),
}

#[derive(Debug, Clone)]
pub enum AuthMaterial {
    Password(Option<Secret>),
    Key { source: KeySource, passphrase: Option<Secret> },
}

#[derive(Debug, Clone)]
pub struct Target {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: AuthMaterial,
}

/// Разовые секреты от фронта (не сохраняются).
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OneTimeSecrets {
    #[serde(default)]
    pub password: Option<Secret>,
    #[serde(default)]
    pub passphrase: Option<Secret>,
}

pub fn local_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "root".to_string())
}

impl Target {
    /// Собрать цель из подключения, расшифровав сохранённые секреты.
    pub fn from_conn(store: &mut Store, c: &Conn, once: Option<&OneTimeSecrets>) -> CoreResult<Target> {
        let mut dec = |field: &str, blob: &Option<Vec<u8>>| -> CoreResult<Option<Secret>> {
            match blob {
                Some(b) => Ok(Some(store.decrypt_string(&Conn::aad(&c.id, field), b)?)),
                None => Ok(None),
            }
        };
        let passphrase = match once.and_then(|o| o.passphrase.clone()) {
            Some(p) => Some(p),
            None => dec("passphrase", &c.secret_passphrase)?,
        };
        let auth = match c.auth_method {
            AuthMethod::Password => AuthMaterial::Password(match once.and_then(|o| o.password.clone()) {
                Some(p) => Some(p),
                None => dec("password", &c.secret_password)?,
            }),
            AuthMethod::KeyFile => AuthMaterial::Key {
                source: KeySource::File(expand_tilde(c.key_path.as_deref().unwrap_or(""))),
                passphrase,
            },
            AuthMethod::KeyText => AuthMaterial::Key {
                source: KeySource::Text(dec("key_text", &c.secret_key_text)?.unwrap_or_default()),
                passphrase,
            },
        };
        Ok(Target {
            host: c.host.clone(),
            port: c.port,
            user: c.user.clone().unwrap_or_else(local_user),
            auth,
        })
    }
}

pub fn expand_tilde(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/").or_else(|| p.strip_prefix("~\\")) {
        if let Some(h) = dirs::home_dir() {
            return h.join(rest);
        }
    }
    if p == "~" {
        if let Some(h) = dirs::home_dir() {
            return h;
        }
    }
    PathBuf::from(p)
}
