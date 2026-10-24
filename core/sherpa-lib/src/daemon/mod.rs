//! Демон терминальных сессий (CORE_API §4) — следующий этап.
//!
//! Задел архитектуры:
//! - транспорт: Unix socket / named pipe (`endpoint()`), JSON Lines;
//! - сообщения: запрос `{id, method, params}`, ответ `{id, ok, data|error}`,
//!   событие `{event, session, …}`; ошибки — тот же `CoreError`;
//! - SSH — общий слой `crate::ssh` (connect с TOFU, `accept_once_fingerprint`
//!   для ответа `accept_once`, авторизация), каналы PTY поверх `Session.handle`;
//! - утилиты рабочего стола будут регистрировать методы демона так же, как
//!   группы CLI в `commands::registry()`.

use std::path::PathBuf;

/// Адрес сокета демона для текущего пользователя (§4.1).
pub fn endpoint(db_path: &std::path::Path) -> PathBuf {
    let default_db = crate::config::Settings::load(&Default::default())
        .map(|s| s.db_path)
        .unwrap_or_default();
    let suffix = if db_path == default_db {
        String::new()
    } else {
        use sha2::{Digest, Sha256};
        let h = Sha256::digest(db_path.display().to_string().as_bytes());
        format!("-{}", &hex::encode(h)[..8])
    };
    #[cfg(windows)]
    {
        let user = crate::ssh::target::local_user();
        PathBuf::from(format!(r"\\.\pipe\sherpa-core-{user}{suffix}"))
    }
    #[cfg(not(windows))]
    {
        let base = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/state"));
        base.join("sherpa").join(format!("daemon{suffix}.sock"))
    }
}
