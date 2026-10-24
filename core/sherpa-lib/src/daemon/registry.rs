//! Реестр методов демона — по образцу `commands::registry()` для CLI.
//! Группа методов (`session`, `desktop`, `monitor`…) отвечает за все методы
//! с её префиксом: `monitor.snapshot` → группа `monitor`.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use super::state::{ClientHandle, DaemonState};
use crate::error::{CoreError, CoreResult};
use crate::ssh::PooledConn;

/// Контекст вызова: состояние демона и клиент, приславший запрос.
#[derive(Clone)]
pub struct Cx {
    pub state: Arc<DaemonState>,
    pub client: ClientHandle,
}

impl Cx {
    /// Соединение из пула; если его нет — подключиться без вопросов к
    /// пользователю (сохранённые ключи и секреты). Интерактивно соединение
    /// открывает `desktop.open` или `session.open`.
    pub async fn conn(&self, connection_id: &str) -> CoreResult<Arc<PooledConn>> {
        super::connect::pooled(
            &self.state,
            connection_id,
            Default::default(),
            &super::connect::NoPrompt,
        )
        .await
    }
}

#[async_trait]
pub trait MethodGroup: Send + Sync {
    /// Префикс методов группы (`session` для `session.open`).
    fn prefix(&self) -> &'static str;
    /// Имена методов (для `daemon.methods` и проверки).
    fn methods(&self) -> &'static [&'static str];
    async fn call(&self, cx: &Cx, method: &str, params: Value) -> CoreResult<Value>;
}

pub fn registry() -> Vec<Arc<dyn MethodGroup>> {
    vec![
        Arc::new(super::methods::daemon::DaemonMethods),
        Arc::new(super::methods::session::SessionMethods),
        Arc::new(super::methods::desktop::DesktopMethods),
        Arc::new(super::methods::danger::DangerMethods),
        Arc::new(super::methods::files::FilesMethods),
        Arc::new(super::methods::transfer::TransferMethods),
        Arc::new(super::methods::tunnel::TunnelMethods),
        Arc::new(super::methods::privs::PrivMethods),
        Arc::new(crate::utilities::monitor::MonitorMethods),
        Arc::new(crate::utilities::disk::DiskMethods),
        Arc::new(crate::utilities::log::LogMethods),
        Arc::new(crate::utilities::cron::CronMethods),
        Arc::new(crate::utilities::net::NetMethods),
        Arc::new(crate::utilities::nettools::NettoolsMethods),
        Arc::new(crate::utilities::svc::SvcMethods),
        Arc::new(crate::utilities::pkg::PkgMethods),
        Arc::new(crate::utilities::sys::SysMethods),
        Arc::new(crate::utilities::docker::DockerMethods),
        Arc::new(crate::utilities::web::WebMethods),
        Arc::new(crate::utilities::db::DbMethods),
        Arc::new(crate::utilities::audit::AuditMethods),
        Arc::new(crate::utilities::notes::NotesMethods),
        Arc::new(crate::utilities::notes::VaultMethods),
        Arc::new(crate::utilities::backup::BackupMethods),
        Arc::new(crate::utilities::diff::DiffMethods),
        Arc::new(crate::utilities::snippet::SnippetMethods),
        Arc::new(crate::utilities::snippet::MultiMethods),
        Arc::new(crate::utilities::sec::SecMethods),
    ]
}

pub fn unknown(method: &str) -> CoreError {
    CoreError::new("protocol.unknown_method", format!("Unknown method: {method}")).with("method", method)
}

pub async fn dispatch(cx: &Cx, method: &str, params: Value) -> CoreResult<Value> {
    let prefix = method.split('.').next().unwrap_or("");
    let group = cx.state.registry.iter().find(|g| g.prefix() == prefix).cloned();
    match group {
        Some(g) if g.methods().contains(&method) => {
            let modifying = crate::utilities::audit::is_mutating(method);
            let connection = params.get("connection_id").and_then(Value::as_str).map(str::to_owned);
            // Сначала проверяем доступность журнала; ошибка записи не замалчивается.
            if modifying {
                cx.state.with_store(|_| Ok(()))?;
            }
            let result = g.call(cx, method, params).await;
            if modifying {
                let outcome = result.as_ref().map(|_| "ok").unwrap_or_else(|e| e.code.as_str());
                cx.state.with_store(|s| {
                    crate::utilities::audit::record(
                        s,
                        &cx.client.id.to_string(),
                        connection.as_deref(),
                        method,
                        outcome,
                    )
                })?;
            }
            result
        }
        _ => Err(unknown(method)),
    }
}
