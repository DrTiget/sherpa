//! `daemon.status`, `daemon.stop`, `daemon.methods`.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::daemon::proto::opt_bool;
use crate::daemon::registry::{unknown, Cx, MethodGroup};
use crate::error::{CoreError, CoreResult};
use crate::protocol::PROTOCOL;

pub struct DaemonMethods;

#[async_trait]
impl MethodGroup for DaemonMethods {
    fn prefix(&self) -> &'static str {
        "daemon"
    }

    fn methods(&self) -> &'static [&'static str] {
        &["daemon.status", "daemon.stop", "daemon.methods"]
    }

    async fn call(&self, cx: &Cx, method: &str, p: Value) -> CoreResult<Value> {
        let st = &cx.state;
        match method {
            "daemon.status" => Ok(json!({
                "running": true,
                "pid": std::process::id(),
                "endpoint": st.paths.endpoint.display().to_string(),
                "core_version": env!("CARGO_PKG_VERSION"),
                "protocol": PROTOCOL,
                "sessions": st.sessions.lock().expect("sessions").len(),
                "clients": st.clients.lock().expect("clients").len(),
                "connections": st.pool.list().await.len(),
                "uptime_sec": st.started.elapsed().as_secs(),
                "idle_timeout_sec": st.settings.daemon_idle_timeout_sec,
            })),
            "daemon.stop" => {
                let n = st.sessions.lock().expect("sessions").len();
                if n > 0 && !opt_bool(&p, "force")? {
                    return Err(CoreError::new("daemon.sessions_active", "There are active sessions").with("count", n));
                }
                let state = st.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    state.request_shutdown("stop");
                });
                Ok(json!({ "stopping": true, "sessions": n }))
            }
            "daemon.methods" => {
                let mut all: Vec<&str> = vec!["hello", "ping"];
                for g in &st.registry {
                    all.extend_from_slice(g.methods());
                }
                Ok(json!({ "items": all }))
            }
            _ => Err(unknown(method)),
        }
    }
}
