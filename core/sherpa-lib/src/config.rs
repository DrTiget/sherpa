//! Конфиг ядра и пути по умолчанию (CORE_API §1.2).

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{CoreError, CoreResult};

pub const DEFAULT_CONNECT_TIMEOUT_SEC: u64 = 10;
pub const DEFAULT_CHECK_CONCURRENCY: usize = 32;
pub const DEFAULT_DAEMON_IDLE_TIMEOUT_SEC: u64 = 300;

/// Содержимое `core.toml`; все поля необязательны.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigFile {
    pub db: Option<PathBuf>,
    pub key_file: Option<PathBuf>,
    pub connect_timeout_sec: Option<u64>,
    pub check_concurrency: Option<usize>,
    pub daemon_idle_timeout_sec: Option<u64>,
}

/// Явные переопределения из флагов/окружения.
#[derive(Debug, Default, Clone)]
pub struct Overrides {
    pub config: Option<PathBuf>,
    pub db: Option<PathBuf>,
    pub key_file: Option<PathBuf>,
}

/// Итоговые настройки, с которыми работает ядро.
#[derive(Debug, Clone)]
pub struct Settings {
    pub config_path: PathBuf,
    pub config_exists: bool,
    pub db_path: PathBuf,
    pub key_file_path: PathBuf,
    pub connect_timeout_sec: u64,
    pub check_concurrency: usize,
    pub daemon_idle_timeout_sec: u64,
}

fn config_dir() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    if cfg!(windows) {
        base.join("Sherpa")
    } else {
        base.join("sherpa")
    }
}

fn data_dir() -> PathBuf {
    if cfg!(windows) {
        dirs::data_local_dir().unwrap_or_else(|| PathBuf::from(".")).join("Sherpa")
    } else {
        dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("sherpa")
    }
}

pub fn default_config_path() -> PathBuf {
    config_dir().join("core.toml")
}

impl Settings {
    pub fn load(ov: &Overrides) -> CoreResult<Settings> {
        let config_path = ov.config.clone().unwrap_or_else(default_config_path);
        let (file, exists) = read_config(&config_path)?;
        Ok(Settings {
            db_path: ov
                .db
                .clone()
                .or(file.db)
                .unwrap_or_else(|| data_dir().join("sherpa.db")),
            key_file_path: ov
                .key_file
                .clone()
                .or(file.key_file)
                .unwrap_or_else(|| config_dir().join("sherpa.key")),
            connect_timeout_sec: file
                .connect_timeout_sec
                .unwrap_or(DEFAULT_CONNECT_TIMEOUT_SEC)
                .clamp(1, 600),
            check_concurrency: file
                .check_concurrency
                .unwrap_or(DEFAULT_CHECK_CONCURRENCY)
                .clamp(1, 100),
            daemon_idle_timeout_sec: file
                .daemon_idle_timeout_sec
                .unwrap_or(DEFAULT_DAEMON_IDLE_TIMEOUT_SEC),
            config_path,
            config_exists: exists,
        })
    }
}

fn read_config(path: &Path) -> CoreResult<(ConfigFile, bool)> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok((ConfigFile::default(), false))
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            return Err(CoreError::fs("store.permission_denied", path, &e))
        }
        Err(e) => return Err(CoreError::fs("fs.read_failed", path, &e)),
    };
    let cfg: ConfigFile = toml::from_str(&text).map_err(|e| {
        let line = e
            .span()
            .map(|s| text[..s.start.min(text.len())].lines().count().max(1))
            .unwrap_or(0);
        CoreError::new("config.invalid", format!("Invalid config: {}", e.message()))
            .with("path", path.display().to_string())
            .with("line", line)
            .with("reason", e.message().to_string())
    })?;
    Ok((cfg, true))
}

/// Шаблон конфига, который пишет `init`.
pub fn config_template(db: &Path, key_file: &Path) -> String {
    format!(
        "# Sherpa core config\n\
         db = {db:?}\n\
         key_file = {key:?}\n\
         connect_timeout_sec = {DEFAULT_CONNECT_TIMEOUT_SEC}\n\
         check_concurrency = {DEFAULT_CHECK_CONCURRENCY}\n\
         daemon_idle_timeout_sec = {DEFAULT_DAEMON_IDLE_TIMEOUT_SEC}\n",
        db = db.display().to_string(),
        key = key_file.display().to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_win_and_defaults_apply() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("core.toml");
        std::fs::write(&cfg, "db = \"/x/a.db\"\nconnect_timeout_sec = 5\n").unwrap();
        let s = Settings::load(&Overrides {
            config: Some(cfg),
            db: None,
            key_file: Some("/k".into()),
        })
        .unwrap();
        assert_eq!(s.db_path, PathBuf::from("/x/a.db"));
        assert_eq!(s.key_file_path, PathBuf::from("/k"));
        assert_eq!(s.connect_timeout_sec, 5);
        assert_eq!(s.check_concurrency, DEFAULT_CHECK_CONCURRENCY);
    }

    #[test]
    fn invalid_config_reports_line() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("core.toml");
        std::fs::write(&cfg, "db = \"a\"\nbogus = 1\n").unwrap();
        let e = Settings::load(&Overrides { config: Some(cfg), ..Default::default() }).unwrap_err();
        assert_eq!(e.code, "config.invalid");
    }
}
