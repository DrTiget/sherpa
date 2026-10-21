use serde::Serialize;

/// Ошибка, сериализуемая во фронт: `kind` — машинный код, `message` — текст для пользователя.
#[derive(Debug, Clone, Serialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CoreError {
    #[error("{message}")]
    Message { message: String },
    #[error("Сервер не найден")]
    NotFound,
    /// Первый контакт с хостом: нужно подтверждение отпечатка (TOFU).
    #[error("Неизвестный хост {host}:{port}, отпечаток {fingerprint}")]
    UnknownHost { host: String, port: u16, fingerprint: String, algorithm: String },
    /// Ключ хоста не совпал с сохранённым — возможна подмена (MITM).
    #[error("Ключ хоста {host}:{port} изменился (ожидался {expected}, получен {actual}). Подключение отклонено: возможна атака «человек посередине». Если ключ сменён намеренно, удалите запись хоста из known_hosts приложения.")]
    HostKeyMismatch { host: String, port: u16, expected: String, actual: String },
    #[error("Ошибка аутентификации: {message}")]
    AuthFailed { message: String },
}

impl CoreError {
    pub fn msg(m: impl Into<String>) -> Self {
        CoreError::Message { message: m.into() }
    }
}

pub type CoreResult<T> = Result<T, CoreError>;

macro_rules! from_err {
    ($($t:ty),*) => {$(impl From<$t> for CoreError { fn from(e: $t) -> Self { CoreError::msg(e.to_string()) } })*};
}
from_err!(std::io::Error, serde_json::Error, russh::Error, russh::keys::Error, keyring::Error);
