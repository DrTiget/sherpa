//! `priv.*` — sudo и режим «только чтение» для соединения рабочего стола.

use async_trait::async_trait;
use serde_json::Value;

use crate::daemon::proto::{opt_bool, req_str};
use crate::daemon::registry::{unknown, Cx, MethodGroup};
use crate::error::CoreResult;
use crate::secret::Secret;

pub struct PrivMethods;

#[async_trait]
impl MethodGroup for PrivMethods {
    fn prefix(&self) -> &'static str {
        "priv"
    }

    fn methods(&self) -> &'static [&'static str] {
        &[
            "priv.sudo.check",
            "priv.sudo.auth",
            "priv.sudo.forget",
            "priv.readonly.set",
        ]
    }

    async fn call(&self, cx: &Cx, method: &str, p: Value) -> CoreResult<Value> {
        let conn = cx.conn(&req_str(&p, "connection_id")?).await?;
        match method {
            "priv.sudo.check" => conn.privilege_status().await,
            "priv.sudo.auth" => conn.sudo_auth(Secret::new(req_str(&p, "password")?)).await,
            "priv.sudo.forget" => {
                conn.sudo_forget();
                conn.privilege_status().await
            }
            "priv.readonly.set" => {
                conn.set_readonly(opt_bool(&p, "readonly")?);
                conn.privilege_status().await
            }
            _ => Err(unknown(method)),
        }
    }
}
