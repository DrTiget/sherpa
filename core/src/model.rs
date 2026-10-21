use serde::{Deserialize, Serialize};

/// Способ аутентификации. Сами секреты тут не хранятся — только ссылки на запись в системном хранилище.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthConfig {
    Password {
        /// Имя записи в keyring; None — пароль не задан.
        #[serde(default)]
        password_ref: Option<String>,
    },
    Key {
        /// Путь к приватному ключу на диске ПК пользователя.
        #[serde(default)]
        key_path: Option<String>,
        /// Ссылка на импортированный ключ (PEM) в keyring.
        #[serde(default)]
        key_ref: Option<String>,
        #[serde(default)]
        passphrase_ref: Option<String>,
    },
    Agent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub username: String,
    #[serde(default)]
    pub group: Option<String>,
    pub auth: AuthConfig,
}

fn default_port() -> u16 {
    22
}

/// Тип аутентификации + открытые секреты — приходит из формы и сразу уходит в keyring.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthInput {
    Password { password: Option<String> },
    Key { key_path: Option<String>, key_pem: Option<String>, passphrase: Option<String> },
    Agent,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerInput {
    pub name: String,
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub username: String,
    #[serde(default)]
    pub group: Option<String>,
    pub auth: AuthInput,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_roundtrip_has_no_secrets() {
        let s = Server {
            id: "1".into(), name: "web".into(), host: "h".into(), port: 2222, username: "root".into(),
            group: Some("prod".into()),
            auth: AuthConfig::Key { key_path: Some("/k".into()), key_ref: None, passphrase_ref: Some("server:1:passphrase".into()) },
        };
        let j = serde_json::to_string(&s).unwrap();
        assert!(j.contains(r#""type":"key""#));
        assert_eq!(serde_json::from_str::<Server>(&j).unwrap(), s);
    }

    #[test]
    fn defaults_apply() {
        let s: Server = serde_json::from_str(r#"{"id":"1","name":"a","host":"h","username":"u","auth":{"type":"agent"}}"#).unwrap();
        assert_eq!(s.port, 22);
        assert_eq!(s.group, None);
    }
}
