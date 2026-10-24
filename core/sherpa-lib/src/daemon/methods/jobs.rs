//! Долгие операции принадлежат демону, состояние доступно после переподключения.
use crate::{
    daemon::{proto::event, state::DaemonState},
    error::{CoreError, CoreResult},
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::sync::{watch, Semaphore};
pub struct Job {
    pub id: String,
    pub kind: &'static str,
    pub connection: String,
    pub state: Mutex<Value>,
    pub cancel: watch::Sender<bool>,
}
impl Job {
    pub async fn update(&self, daemon: &DaemonState, value: Value) {
        *self.state.lock().expect("job") = value.clone();
        let msg = event(
            self.kind,
            json!({"id":self.id,"connection_id":self.connection,"data":value}),
        );
        let clients: Vec<_> = daemon.clients.lock().expect("clients").values().cloned().collect();
        // Уведомления можно восстановить через list; медленный GUI не блокирует туннель.
        for c in clients {
            let _ = c.tx.try_send(msg.to_string());
        }
        daemon.touch();
    }
}
pub struct Jobs {
    pub entries: Mutex<HashMap<String, Arc<Job>>>,
    pub transfers: Arc<Semaphore>,
}
impl Default for Jobs {
    fn default() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            transfers: Arc::new(Semaphore::new(2)),
        }
    }
}
impl Jobs {
    pub fn insert(&self, id: String, kind: &'static str, connection: String, data: Value) -> CoreResult<Arc<Job>> {
        let mut entries = self.entries.lock().expect("jobs");
        if entries.len() >= 256 {
            entries.retain(|_, j| !terminal(&j.state.lock().expect("job")));
        }
        if entries.len() >= 256 {
            return Err(CoreError::new("job.limit", "Too many background jobs"));
        }
        let j = Arc::new(Job {
            id: id.clone(),
            kind,
            connection,
            state: Mutex::new(data),
            cancel: watch::channel(false).0,
        });
        entries.insert(id, j.clone());
        Ok(j)
    }
    pub fn get(&self, id: &str, kind: &str) -> CoreResult<Arc<Job>> {
        self.entries
            .lock()
            .expect("jobs")
            .get(id)
            .filter(|j| j.kind == kind)
            .cloned()
            .ok_or_else(|| CoreError::new("job.not_found", "Background job not found"))
    }
    pub fn list(&self, kind: &str) -> Value {
        let entries = self.entries.lock().expect("jobs");
        json!({"items": entries.values().filter(|j| j.kind == kind).map(|j| json!({"id":j.id,"connection_id":j.connection,"data":j.state.lock().expect("job").clone()})).collect::<Vec<_>>()})
    }
    pub fn active(&self) -> bool {
        self.entries
            .lock()
            .expect("jobs")
            .values()
            .any(|j| !terminal(&j.state.lock().expect("job")))
    }
    pub fn cancel_connection(&self, connection: &str) {
        for j in self
            .entries
            .lock()
            .expect("jobs")
            .values()
            .filter(|j| j.connection == connection)
        {
            j.cancel.send_replace(true);
        }
    }
    pub fn cancel_all(&self) {
        for j in self.entries.lock().expect("jobs").values() {
            j.cancel.send_replace(true);
        }
    }
}
fn terminal(v: &Value) -> bool {
    matches!(
        v["status"].as_str(),
        Some("completed" | "failed" | "cancelled" | "stopped")
    )
}
