//! Одноразовые подтверждения, связанные с клиентом, действием и параметрами.
use crate::{
    daemon::{
        proto::req_str,
        registry::{Cx, MethodGroup},
    },
    error::{CoreError, CoreResult},
};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

struct Ticket {
    client: u64,
    action: String,
    params: Value,
    expires: Instant,
}
#[derive(Default)]
pub struct Confirmations(Mutex<HashMap<String, Ticket>>);
fn bound(p: &Value) -> Value {
    let mut p = p.clone();
    if let Some(o) = p.as_object_mut() {
        o.remove("confirm_token");
    }
    p
}
impl Confirmations {
    pub fn prepare(&self, client: u64, action: &str, params: &Value) -> CoreResult<String> {
        if !params.is_object() {
            return Err(CoreError::invalid("params", "expected object"));
        }
        let mut tickets = self.0.lock().expect("confirmations");
        tickets.retain(|_, t| t.expires > Instant::now());
        if tickets.len() >= 1024 {
            return Err(CoreError::new("danger.limit", "Too many pending confirmations"));
        }
        let token = hex::encode(rand::random::<[u8; 32]>());
        tickets.insert(
            token.clone(),
            Ticket {
                client,
                action: action.into(),
                params: bound(params),
                expires: Instant::now() + Duration::from_secs(60),
            },
        );
        Ok(token)
    }
    /// Вызывать непосредственно перед изменением, после валидации и ensure_writable.
    pub fn consume(&self, client: u64, action: &str, params: &Value) -> CoreResult<()> {
        let token = params.get("confirm_token").and_then(Value::as_str).unwrap_or("");
        let mut tickets = self.0.lock().expect("confirmations");
        let valid = tickets.get(token).is_some_and(|t| {
            t.client == client && t.action == action && t.params == bound(params) && t.expires > Instant::now()
        });
        if !valid {
            return Err(CoreError::new(
                "danger.confirmation_required",
                "A matching, unexpired confirmation is required",
            ));
        }
        tickets.remove(token);
        Ok(())
    }
    pub fn client_gone(&self, client: u64) {
        self.0.lock().expect("confirmations").retain(|_, t| t.client != client);
    }
}
pub struct DangerMethods;
#[async_trait]
impl MethodGroup for DangerMethods {
    fn prefix(&self) -> &'static str {
        "danger"
    }
    fn methods(&self) -> &'static [&'static str] {
        &["danger.prepare"]
    }
    async fn call(&self, cx: &Cx, _: &str, p: Value) -> CoreResult<Value> {
        let action = req_str(&p, if p.get("method").is_some() { "method" } else { "action" })?;
        if action.starts_with("danger.") || !cx.state.registry.iter().any(|g| g.methods().contains(&action.as_str())) {
            return Err(CoreError::invalid("action", "unknown action"));
        }
        let token = cx.state.confirmations.prepare(cx.client.id, &action, &p["params"])?;
        Ok(json!({"confirm_token": token, "expires_in_sec": 60, "action": action, "params": bound(&p["params"])}))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn binding_replay_expiry() {
        let c = Confirmations::default();
        let p = json!({"connection_id":"c", "path":"/tmp/a", "recursive":true});
        let token = c.prepare(1, "files.delete", &p).unwrap();
        let mut confirmed = p.clone();
        confirmed["confirm_token"] = token.clone().into();
        assert!(c.consume(2, "files.delete", &confirmed).is_err());
        assert!(c.consume(1, "files.write", &confirmed).is_err());
        let mut changed = confirmed.clone();
        changed["path"] = "/".into();
        assert!(c.consume(1, "files.delete", &changed).is_err());
        c.consume(1, "files.delete", &confirmed).unwrap();
        assert!(c.consume(1, "files.delete", &confirmed).is_err());
        let token = c.prepare(1, "files.delete", &p).unwrap();
        c.0.lock().unwrap().get_mut(&token).unwrap().expires = Instant::now();
        confirmed["confirm_token"] = token.into();
        assert!(c.consume(1, "files.delete", &confirmed).is_err());
    }
}
