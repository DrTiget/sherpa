//! Подключение к серверу из демона: в пул, с вопросами пользователю о host key
//! и пароле (события `hostkey_prompt` / `auth_prompt`, ответы — методами).

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use super::state::{ClientHandle, DaemonState, PROMPT_TIMEOUT};
use crate::error::{CoreError, CoreResult};
use crate::repo::conn::{self, AuthMethod, Status};
use crate::repo::hostkey::{self, Presented};
use crate::secret::Secret;
use crate::ssh::target::OneTimeSecrets;
use crate::ssh::{self, ConnectOptions, PooledConn, Target};

/// Куда отправлять вопросы (сессия или клиент `desktop.open`).
#[async_trait]
pub trait EventSink: Send + Sync {
    async fn emit_prompt(&self, name: &str, fields: Value);
    async fn prompt_done(&self) {}
}

#[async_trait]
impl EventSink for ClientHandle {
    async fn emit_prompt(&self, name: &str, fields: Value) {
        self.send(&super::proto::event(name, fields)).await;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HostKeyDecision {
    Accept,
    AcceptOnce,
    Reject,
    Cancel,
}

#[async_trait]
pub trait Prompter: Send + Sync {
    async fn hostkey(&self, connection_id: &str, kind: &str, err: &CoreError) -> HostKeyDecision;
    /// `None` — пользователь отменил. `(секрет, сохранить)`.
    async fn auth(&self, connection_id: &str, kind: &str, attempt: u32) -> Option<(Secret, bool)>;
    /// false — вопросов нет, ошибки возвращаются как есть.
    fn interactive(&self) -> bool {
        true
    }
}

/// Без вопросов: ошибки host key и авторизации возвращаются как есть.
pub struct NoPrompt;

#[async_trait]
impl Prompter for NoPrompt {
    async fn hostkey(&self, _: &str, _: &str, _: &CoreError) -> HostKeyDecision {
        HostKeyDecision::Cancel
    }
    async fn auth(&self, _: &str, _: &str, _: u32) -> Option<(Secret, bool)> {
        None
    }
    fn interactive(&self) -> bool {
        false
    }
}

/// Вопросы через события; ответы — `session.hostkey_answer` / `session.auth_answer`.
pub struct EventPrompter<'a> {
    pub state: &'a Arc<DaemonState>,
    pub sink: &'a dyn EventSink,
    pub session: Option<String>,
}

impl EventPrompter<'_> {
    async fn ask(&self, name: &str, mut fields: Value) -> Option<Value> {
        let (id, rx) = self.state.new_prompt(self.session.clone());
        fields["prompt_id"] = id.clone().into();
        self.sink.emit_prompt(name, fields).await;
        let r = tokio::time::timeout(PROMPT_TIMEOUT, rx).await;
        self.state.drop_prompt(&id);
        self.sink.prompt_done().await;
        r.ok().and_then(Result::ok)
    }
}

#[async_trait]
impl Prompter for EventPrompter<'_> {
    async fn hostkey(&self, connection_id: &str, kind: &str, err: &CoreError) -> HostKeyDecision {
        let mut f = json!({ "connection_id": connection_id, "kind": kind });
        for k in ["host", "port", "key_type", "fingerprint", "known_fingerprint"] {
            if let Some(v) = err.params.get(k) {
                f[k] = v.clone();
            }
        }
        let answer = self.ask("hostkey_prompt", f).await;
        match answer.as_ref().and_then(|a| a.get("decision")).and_then(Value::as_str) {
            Some("accept") => HostKeyDecision::Accept,
            Some("accept_once") => HostKeyDecision::AcceptOnce,
            Some("reject") => HostKeyDecision::Reject,
            _ => HostKeyDecision::Cancel,
        }
    }

    async fn auth(&self, connection_id: &str, kind: &str, attempt: u32) -> Option<(Secret, bool)> {
        let text = if kind == "passphrase" {
            "Key passphrase:"
        } else {
            "Password:"
        };
        let f = json!({
            "connection_id": connection_id,
            "kind": kind,
            "prompts": [{ "text": text, "echo": false }],
            "attempt": attempt,
        });
        let a = self.ask("auth_prompt", f).await?;
        let resp = a.get("responses")?.as_array()?.first()?.as_str()?.to_string();
        let save = a.get("save").and_then(Value::as_bool).unwrap_or(false);
        Some((Secret::new(resp), save))
    }
}

pub const MAX_AUTH_ATTEMPTS: u32 = 3;

/// Какой секрет спросить по ошибке авторизации (или `None` — спрашивать нечего).
fn auth_prompt_kind(e: &CoreError, method: AuthMethod) -> Option<&'static str> {
    match e.code.as_str() {
        "auth.secret_required" => match e.params.get("kind").and_then(Value::as_str) {
            Some("passphrase") => Some("passphrase"),
            _ => Some("password"),
        },
        "auth.passphrase_wrong" => Some("passphrase"),
        "auth.rejected" if method == AuthMethod::Password => Some("password"),
        _ => None,
    }
}

fn save_status(state: &DaemonState, id: &str, err: Option<&CoreError>) {
    let st = Status {
        state: crate::commands::check::state_for(err).into(),
        checked_at: Some(crate::util::now()),
        latency_ms: None,
        error: err.map(|e| serde_json::to_value(e).unwrap_or(Value::Null)),
    };
    let _ = state.with_store(|s| conn::save_status(s, id, &st));
}

/// Подключиться (с вопросами через `prompter`). Возвращает сессию и баннер.
pub async fn connect_interactive(
    state: &Arc<DaemonState>,
    connection_id: &str,
    mut once: OneTimeSecrets,
    prompter: &dyn Prompter,
) -> CoreResult<(ssh::Session, Option<String>)> {
    let timeout = std::time::Duration::from_secs(state.settings.connect_timeout_sec);
    let mut accept_once: Option<String> = None;
    let mut attempts = 0u32;
    let mut save_secret: Option<(&'static str, Secret)> = None;
    loop {
        let (target, known, method, host, port) = state.with_store(|s| {
            let c = conn::get(s, connection_id)?;
            let t = Target::from_conn(s, &c, Some(&once))?;
            let k = hostkey::for_addr(s, &c.host, c.port)?;
            Ok((t, k, c.auth_method, c.host.clone(), c.port))
        })?;
        let mut opts = ConnectOptions::new(timeout, known);
        opts.accept_once_fingerprint = accept_once.clone();
        let out = ssh::connect(&target, &opts).await;
        let err = match out.result {
            Ok(session) => {
                save_status(state, connection_id, None);
                if let Some((kind, secret)) = save_secret.take() {
                    let patch = json!({ "auth": { kind: secret.expose() } });
                    let r = state.with_store(|s| conn::update(s, connection_id, patch.as_object().expect("obj")));
                    if let Err(e) = r {
                        tracing::warn!("cannot save secret: {e}");
                    }
                }
                return Ok((session, out.server_banner));
            }
            Err(e) => e,
        };
        if err.code == "hostkey.unknown" || err.code == "hostkey.mismatch" {
            let Some(p) = out.presented.clone() else {
                return Err(err);
            };
            let kind = if err.code == "hostkey.unknown" {
                "unknown"
            } else {
                "mismatch"
            };
            match prompter.hostkey(connection_id, kind, &err).await {
                HostKeyDecision::Accept => {
                    let replace = kind == "mismatch";
                    state.with_store(|s| hostkey::trust(s, &host, port, &p, replace).map(|_| ()))?;
                }
                HostKeyDecision::AcceptOnce => accept_once = Some(p.fingerprint.clone()),
                HostKeyDecision::Reject => {
                    let Presented { key_type, fingerprint } = p;
                    state.with_store(|s| hostkey::reject(s, &host, port, &fingerprint, Some(&key_type)).map(|_| ()))?;
                    return Err(err);
                }
                HostKeyDecision::Cancel => return Err(err),
            }
            continue;
        }
        if let Some(kind) = auth_prompt_kind(&err, method) {
            if attempts < MAX_AUTH_ATTEMPTS {
                attempts += 1;
                match prompter.auth(connection_id, kind, attempts).await {
                    Some((secret, save)) => {
                        if kind == "passphrase" {
                            once.passphrase = Some(secret.clone());
                        } else {
                            once.password = Some(secret.clone());
                        }
                        save_secret = save.then_some((kind, secret));
                        continue;
                    }
                    None if prompter.interactive() => {
                        return Err(CoreError::new("auth.cancelled", "Authentication cancelled"));
                    }
                    None => {}
                }
            }
        }
        save_status(state, connection_id, Some(&err));
        return Err(err);
    }
}

/// Соединение из пула или новое (одно на подключение).
pub async fn pooled(
    state: &Arc<DaemonState>,
    connection_id: &str,
    once: OneTimeSecrets,
    prompter: &dyn Prompter,
) -> CoreResult<Arc<PooledConn>> {
    state
        .pool
        .get_or_connect(connection_id, || {
            connect_interactive(state, connection_id, once, prompter)
        })
        .await
}
