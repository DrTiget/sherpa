//! Утилиты рабочего стола: группы методов демона поверх `ssh::RemoteExec`.
//! Как добавить новую — `docs/UTILITY_GUIDE.md`.

pub mod capabilities;
pub mod cron;
pub mod disk;
pub mod log;
pub mod monitor;
pub mod net;
pub mod nettools;

use crate::error::CoreError;
use crate::ssh::ExecOutput;

/// Ошибка «команда на сервере завершилась неудачно».
pub fn command_failed(command: &str, out: &ExecOutput) -> CoreError {
    let stderr: String = out.stderr_lossy().chars().take(500).collect();
    CoreError::new("remote.command_failed", format!("Remote command failed: {command}"))
        .with("command", command)
        .with(
            "exit_status",
            out.exit_status
                .map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null),
        )
        .with("stderr", stderr)
}

pub mod db;
pub mod docker;
pub mod web;
