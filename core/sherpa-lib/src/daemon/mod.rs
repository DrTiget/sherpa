//! Демон ядра (CORE_API §4): терминальные сессии, соединения рабочего стола,
//! утилиты. Тот же бинарник (`sherpa-core daemon run`), поднимается по
//! требованию (`daemon ensure`), сам выходит при простое.

pub mod client;
pub mod connect;
pub mod methods;
pub mod proto;
pub mod registry;
pub mod server;
pub mod session;
pub mod state;
pub mod transport;

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::config::Settings;
use crate::error::{CoreError, CoreResult};

/// Файлы демона для данного хранилища.
#[derive(Debug, Clone)]
pub struct Paths {
    pub endpoint: PathBuf,
    pub lock: PathBuf,
    pub log: PathBuf,
}

/// Суффикс для нестандартной БД: у разных хранилищ — разные демоны.
fn suffix(db: &Path) -> String {
    let default_db = Settings::load(&Default::default())
        .map(|s| s.db_path)
        .unwrap_or_default();
    if db == default_db {
        return String::new();
    }
    use sha2::{Digest, Sha256};
    let h = Sha256::digest(db.display().to_string().as_bytes());
    format!("-{}", &hex::encode(h)[..8])
}

pub fn paths(settings: &Settings) -> Paths {
    let sfx = suffix(&settings.db_path);
    #[cfg(windows)]
    {
        let user: String = crate::ssh::target::local_user()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Sherpa");
        Paths {
            endpoint: PathBuf::from(format!(r"\\.\pipe\sherpa-core-{user}{sfx}")),
            lock: dir.join(format!("daemon{sfx}.lock")),
            log: dir.join(format!("daemon{sfx}.log")),
        }
    }
    #[cfg(not(windows))]
    {
        let base = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/state"))
            .join("sherpa");
        Paths {
            endpoint: base.join(format!("daemon{sfx}.sock")),
            lock: base.join(format!("daemon{sfx}.lock")),
            log: base.join(format!("daemon{sfx}.log")),
        }
    }
}

/// Эксклюзивный lock-файл: один демон на хранилище. Держится до выхода.
fn acquire_lock(path: &Path) -> CoreResult<File> {
    if let Some(dir) = path.parent() {
        crate::store::crypto::create_private_dir(dir)?;
    }
    let mut f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|e| CoreError::fs("daemon.start_failed", path, &e))?;
    match f.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => {
            let pid = std::fs::read_to_string(path)
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok());
            return Err(CoreError::new("daemon.already_running", "Daemon is already running")
                .with("pid", pid.map(Value::from).unwrap_or(Value::Null)));
        }
        Err(std::fs::TryLockError::Error(e)) => return Err(CoreError::fs("daemon.start_failed", path, &e)),
    }
    let _ = f.set_len(0);
    let _ = write!(f, "{}", std::process::id());
    let _ = f.flush();
    Ok(f)
}

/// Тело процесса демона: `sherpa-core daemon run`.
pub fn run(settings: &Settings) -> CoreResult<Value> {
    let p = paths(settings);
    let _lock = acquire_lock(&p.lock)?;
    let rt = crate::commands::runtime()?;
    let reason = rt.block_on(async {
        let listener = transport::Listener::bind(&p.endpoint)?;
        tracing::info!("daemon listening on {}", p.endpoint.display());
        let state = state::DaemonState::new(settings.clone(), p.clone());
        Ok::<_, CoreError>(server::serve(state, listener).await)
    })?;
    rt.shutdown_timeout(Duration::from_millis(500));
    Ok(json!({ "stopped": reason }))
}

/// Статус работающего демона или `None`.
pub fn query_status(settings: &Settings) -> Option<Value> {
    let p = paths(settings);
    let rt = crate::commands::runtime().ok()?;
    rt.block_on(async {
        let mut c = tokio::time::timeout(Duration::from_secs(2), client::Client::connect(&p.endpoint))
            .await
            .ok()?
            .ok()?;
        c.request("daemon.status", json!({})).await.ok()
    })
}

pub fn status(settings: &Settings) -> CoreResult<Value> {
    Ok(query_status(settings)
        .unwrap_or_else(|| json!({ "running": false, "endpoint": paths(settings).endpoint.display().to_string() })))
}

pub fn stop(settings: &Settings, force: bool) -> CoreResult<Value> {
    let p = paths(settings);
    let rt = crate::commands::runtime()?;
    rt.block_on(async {
        let mut c = match client::Client::connect(&p.endpoint).await {
            Ok(c) => c,
            Err(_) => return Err(CoreError::new("daemon.not_running", "Daemon is not running")),
        };
        let r = c.request("daemon.stop", json!({ "force": force })).await?;
        // Дождаться выхода.
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            if client::Client::connect(&p.endpoint).await.is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Ok(json!({ "stopped": true, "sessions_closed": r["sessions"] }))
    })
}

/// Запустить демон, если он не работает (`daemon ensure`).
pub fn ensure(settings: &Settings) -> CoreResult<Value> {
    let p = paths(settings);
    let ours = env!("CARGO_PKG_VERSION");
    if let Some(st) = query_status(settings) {
        let same = st["core_version"] == ours;
        let sessions = st["sessions"].as_u64().unwrap_or(0);
        if same || sessions > 0 {
            return Ok(json!({
                "endpoint": p.endpoint.display().to_string(),
                "pid": st["pid"],
                "started": false,
                "core_version": st["core_version"],
                "protocol": st["protocol"],
            }));
        }
        // Другая версия и нет сессий — перезапускаем.
        let _ = stop(settings, false);
    }
    spawn_daemon(settings, &p)?;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        if let Some(st) = query_status(settings) {
            return Ok(json!({
                "endpoint": p.endpoint.display().to_string(),
                "pid": st["pid"],
                "started": true,
                "core_version": st["core_version"],
                "protocol": st["protocol"],
            }));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let tail = std::fs::read_to_string(&p.log)
        .ok()
        .and_then(|s| s.lines().rev().find(|l| !l.trim().is_empty()).map(str::to_string))
        .unwrap_or_default();
    Err(CoreError::new("daemon.start_failed", "Daemon did not start in time").with("reason", tail))
}

fn spawn_daemon(settings: &Settings, p: &Paths) -> CoreResult<()> {
    let exe = std::env::current_exe()
        .map_err(|e| CoreError::new("daemon.start_failed", e.to_string()).with("reason", e.to_string()))?;
    if let Some(dir) = p.log.parent() {
        crate::store::crypto::create_private_dir(dir)?;
    }
    let log = File::create(&p.log).map_err(|e| CoreError::fs("daemon.start_failed", &p.log, &e))?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("--config")
        .arg(&settings.config_path)
        .arg("--db")
        .arg(&settings.db_path)
        .arg("--key-file")
        .arg(&settings.key_file_path)
        .args(["--log-level", "info", "daemon", "run"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(log);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }
    cmd.spawn().map_err(|e| {
        CoreError::new("daemon.start_failed", format!("Cannot start daemon: {e}")).with("reason", e.to_string())
    })?;
    Ok(())
}
