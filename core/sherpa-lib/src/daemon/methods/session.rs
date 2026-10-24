//! `session.*` — терминальные сессии (CORE_API §4.3).

use std::collections::HashMap;

use async_trait::async_trait;
use base64::Engine;
use serde_json::{json, Value};

use crate::daemon::proto::{event, opt_bool, opt_str, opt_u64, req_str};
use crate::daemon::registry::{unknown, Cx, MethodGroup};
use crate::daemon::session::{self, not_found, Ctrl, OpenParams, TermSession};
use crate::error::{CoreError, CoreResult};
use crate::ssh::target::OneTimeSecrets;

/// Максимум сессий на демон.
pub const MAX_SESSIONS: usize = 256;

pub struct SessionMethods;

fn size(p: &Value, key: &str, default: u64) -> CoreResult<u32> {
    let v = opt_u64(p, key)?.unwrap_or(default);
    if !(1..=10_000).contains(&v) {
        return Err(
            CoreError::new("validation.out_of_range", format!("{key} is out of range"))
                .with("field", key)
                .with("min", 1)
                .with("max", 10_000),
        );
    }
    Ok(v as u32)
}

/// Сессия, принадлежащая клиенту запроса.
fn owned(cx: &Cx, p: &Value) -> CoreResult<std::sync::Arc<TermSession>> {
    let id = req_str(p, "session")?;
    match cx.state.session(&id) {
        Some(s) if s.owned_by(cx.client.id) => Ok(s),
        _ => Err(not_found(&id)),
    }
}

fn send(s: &TermSession, c: Ctrl) -> CoreResult<Value> {
    s.ctrl.send(c).map_err(|_| {
        CoreError::new("session.not_connected", "Session is closed")
            .with("session", s.id.clone())
            .with("state", "closed")
    })?;
    Ok(Value::Null)
}

#[async_trait]
impl MethodGroup for SessionMethods {
    fn prefix(&self) -> &'static str {
        "session"
    }

    fn methods(&self) -> &'static [&'static str] {
        &[
            "session.open",
            "session.input",
            "session.resize",
            "session.close",
            "session.list",
            "session.attach",
            "session.hostkey_answer",
            "session.auth_answer",
        ]
    }

    async fn call(&self, cx: &Cx, method: &str, p: Value) -> CoreResult<Value> {
        match method {
            "session.open" => {
                if cx.state.sessions.lock().expect("sessions").len() >= MAX_SESSIONS {
                    return Err(CoreError::new("session.limit_reached", "Too many sessions").with("max", MAX_SESSIONS));
                }
                let connection_id = req_str(&p, "connection_id")?;
                // Подключение должно существовать — ошибка сразу, а не событием.
                cx.state
                    .with_store(|s| crate::repo::conn::get(s, &connection_id).map(|_| ()))?;
                let env: HashMap<String, String> = match p.get("env") {
                    None | Some(Value::Null) => HashMap::new(),
                    Some(v) => serde_json::from_value(v.clone())
                        .map_err(|_| CoreError::invalid("env", "expected object of strings"))?,
                };
                let secrets: OneTimeSecrets = match p.get("secrets") {
                    None | Some(Value::Null) => OneTimeSecrets::default(),
                    Some(v) => serde_json::from_value(v.clone())
                        .map_err(|_| CoreError::invalid("secrets", "expected {password, passphrase}"))?,
                };
                let params = OpenParams {
                    connection_id,
                    cols: size(&p, "cols", 80)?,
                    rows: size(&p, "rows", 24)?,
                    term: opt_str(&p, "term")?.unwrap_or_else(|| "xterm-256color".into()),
                    env,
                    secrets,
                };
                let s = session::spawn(&cx.state, cx.client.clone(), params);
                Ok(json!({ "session": s.id }))
            }
            "session.input" => {
                let s = owned(cx, &p)?;
                let data = base64::engine::general_purpose::STANDARD
                    .decode(req_str(&p, "data_b64")?)
                    .map_err(|_| CoreError::invalid("data_b64", "invalid base64"))?;
                send(&s, Ctrl::Input(data))
            }
            "session.resize" => {
                let s = owned(cx, &p)?;
                let (c, r) = (size(&p, "cols", 80)?, size(&p, "rows", 24)?);
                s.set_size(c, r);
                send(&s, Ctrl::Resize(c, r))
            }
            "session.close" => {
                let s = owned(cx, &p)?;
                send(&s, Ctrl::Close("client"))
            }
            "session.list" => {
                let items: Vec<Value> = cx
                    .state
                    .sessions
                    .lock()
                    .expect("sessions")
                    .values()
                    .map(|s| s.info())
                    .collect();
                Ok(json!({ "items": items }))
            }
            "session.attach" => {
                let id = req_str(&p, "session")?;
                let s = cx.state.session(&id).ok_or_else(|| not_found(&id))?;
                let (tail, prompt) = s.attach(cx.client.clone())?;
                let replay_bytes = tail.len();
                // Досылаем хвост и неотвеченный вопрос после ответа на attach.
                let client = cx.client.clone();
                let sid = s.id.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                    if !tail.is_empty() {
                        let ev = event(
                            "output",
                            json!({ "session": sid, "data_b64": base64::engine::general_purpose::STANDARD.encode(&tail), "replay": true }),
                        );
                        client.send(&ev).await;
                    }
                    if let Some(pr) = prompt {
                        client.send(&pr).await;
                    }
                });
                Ok(json!({ "session": s.info(), "replay_bytes": replay_bytes }))
            }
            "session.hostkey_answer" => {
                let prompt = req_str(&p, "prompt_id")?;
                let decision = req_str(&p, "decision")?;
                if !["accept", "accept_once", "reject", "cancel"].contains(&decision.as_str()) {
                    return Err(CoreError::invalid("decision", "accept | accept_once | reject | cancel"));
                }
                let session = opt_str(&p, "session")?;
                cx.state
                    .answer(&prompt, session.as_deref(), json!({ "decision": decision }))?;
                Ok(Value::Null)
            }
            "session.auth_answer" => {
                let prompt = req_str(&p, "prompt_id")?;
                let session = opt_str(&p, "session")?;
                let cancel = opt_bool(&p, "cancel")?;
                let answer = if cancel {
                    json!({ "cancel": true })
                } else {
                    let responses = p
                        .get("responses")
                        .and_then(Value::as_array)
                        .filter(|a| a.iter().all(Value::is_string) && !a.is_empty())
                        .ok_or_else(|| CoreError::required("responses"))?;
                    json!({ "responses": responses, "save": opt_bool(&p, "save")? })
                };
                cx.state.answer(&prompt, session.as_deref(), answer)?;
                Ok(Value::Null)
            }
            _ => Err(unknown(method)),
        }
    }
}
