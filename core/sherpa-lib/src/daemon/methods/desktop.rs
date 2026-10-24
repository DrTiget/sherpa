//! `desktop.*` — соединение рабочего стола и возможности сервера.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::daemon::connect::{self, EventPrompter};
use crate::daemon::proto::{opt_bool, req_str};
use crate::daemon::registry::{unknown, Cx, MethodGroup};
use crate::error::{CoreError, CoreResult};
use crate::ssh::target::OneTimeSecrets;
use crate::utilities::capabilities;

pub struct DesktopMethods;

#[async_trait]
impl MethodGroup for DesktopMethods {
    fn prefix(&self) -> &'static str {
        "desktop"
    }

    fn methods(&self) -> &'static [&'static str] {
        &[
            "desktop.open",
            "desktop.capabilities",
            "desktop.list",
            "desktop.close",
            "desktop.state.get",
            "desktop.state.set",
        ]
    }

    async fn call(&self, cx: &Cx, method: &str, p: Value) -> CoreResult<Value> {
        match method {
            "desktop.state.get" | "desktop.state.set" => {
                let id = req_str(&p, "connection_id")?;
                cx.state.with_store(|s| {
                    crate::repo::conn::get(s, &id)?;
                    let key = format!("desktop.state.{id}");
                    if method == "desktop.state.set" {
                        let state = p
                            .get("state")
                            .filter(|v| v.is_object())
                            .ok_or_else(|| CoreError::invalid("state", "expected object"))?;
                        let text = state.to_string();
                        if text.len() > 65536 {
                            return Err(CoreError::invalid("state", "maximum 64 KiB"));
                        }
                        s.meta_set(&key, &text)?;
                    }
                    let state: Value = s
                        .meta_get(&key)?
                        .map(|v| serde_json::from_str(&v))
                        .transpose()
                        .map_err(|_| CoreError::internal("Invalid desktop state"))?
                        .unwrap_or(json!({}));
                    Ok(json!({"connection_id": id, "state": state}))
                })
            }
            "desktop.open" => {
                let id = req_str(&p, "connection_id")?;
                cx.state.with_store(|s| crate::repo::conn::get(s, &id).map(|_| ()))?;
                let once: OneTimeSecrets = match p.get("secrets") {
                    None | Some(Value::Null) => OneTimeSecrets::default(),
                    Some(v) => serde_json::from_value(v.clone())
                        .map_err(|_| CoreError::invalid("secrets", "expected {password, passphrase}"))?,
                };
                let prompter = EventPrompter {
                    state: &cx.state,
                    sink: &cx.client,
                    session: None,
                };
                let conn = connect::pooled(&cx.state, &id, once, &prompter).await?;
                let caps = capabilities::get(&conn, false).await?;
                Ok(json!({ "connection": conn.info(), "capabilities": caps }))
            }
            "desktop.capabilities" => {
                let id = req_str(&p, "connection_id")?;
                let conn = cx.conn(&id).await?;
                capabilities::get(&conn, opt_bool(&p, "refresh")?).await
            }
            "desktop.list" => Ok(json!({ "items": cx.state.pool.list().await })),
            "desktop.close" => {
                let id = req_str(&p, "connection_id")?;
                cx.state.jobs.cancel_connection(&id);
                Ok(json!({ "closed": cx.state.pool.close(&id).await }))
            }
            _ => Err(unknown(method)),
        }
    }
}
