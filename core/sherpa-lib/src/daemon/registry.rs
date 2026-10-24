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
        Arc::new(super::methods::privs::PrivMethods),
        Arc::new(crate::utilities::monitor::MonitorMethods),
    ]
}

pub fn unknown(method: &str) -> CoreError {
    CoreError::new("protocol.unknown_method", format!("Unknown method: {method}")).with("method", method)
}

pub async fn dispatch(cx: &Cx, method: &str, params: Value) -> CoreResult<Value> {
    let prefix = method.split('.').next().unwrap_or("");
    let group = cx.state.registry.iter().find(|g| g.prefix() == prefix).cloned();
    match group {
        Some(g) if g.methods().contains(&method) => g.call(cx, method, params).await,
        _ => Err(unknown(method)),
    }
}
