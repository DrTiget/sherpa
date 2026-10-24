//! Общий журнал: только метаданные операции, никогда её параметры.
pub mod parse;
use crate::{
    daemon::{
        proto::{opt_str, opt_u64},
        registry::{Cx, MethodGroup},
    },
    error::CoreResult,
    store::Store,
    util,
};
use async_trait::async_trait;
use rusqlite::params;
use serde_json::{json, Value};

pub fn record(store: &Store, client: &str, connection: Option<&str>, method: &str, outcome: &str) -> CoreResult<()> {
    store.conn.execute(
        "DELETE FROM desktop_audit WHERE timestamp < strftime('%Y-%m-%dT%H:%M:%SZ','now','-90 days')",
        [],
    )?;
    store.conn.execute(
        "INSERT INTO desktop_audit VALUES (?1,?2,?3,?4,?5,?6)",
        params![util::new_id(), util::now(), client, connection, method, outcome],
    )?;
    Ok(())
}
/// Общий классификатор; новым утилитам также доступен `record`.
pub fn is_mutating(method: &str) -> bool {
    let action = method.rsplit('.').next().unwrap_or("");
    matches!(
        action,
        "set"
            | "add"
            | "del"
            | "delete"
            | "remove"
            | "create"
            | "update"
            | "apply"
            | "run"
            | "exec"
            | "restore"
            | "prune"
            | "ban"
            | "unban"
            | "reveal"
            | "start"
            | "stop"
            | "restart"
            | "reload"
            | "enable"
            | "disable"
            | "install"
            | "upgrade"
            | "kill"
            | "signal"
            | "write"
            | "rename"
            | "move"
            | "copy"
            | "chmod"
            | "chown"
            | "mkdir"
            | "reboot"
            | "shutdown"
    )
}
pub struct AuditMethods;
#[async_trait]
impl MethodGroup for AuditMethods {
    fn prefix(&self) -> &'static str {
        "audit"
    }
    fn methods(&self) -> &'static [&'static str] {
        &["audit.list", "audit.export"]
    }
    async fn call(&self, cx: &Cx, method: &str, p: Value) -> CoreResult<Value> {
        let connection = opt_str(&p, "connection_id")?;
        let client = opt_str(&p, "client")?;
        let q = opt_str(&p, "q")?.unwrap_or_default();
        let limit = opt_u64(&p, "limit")?.unwrap_or(100).clamp(1, 10000);
        let items=cx.state.with_store(|s| {
      let mut stmt=s.conn.prepare("SELECT id,timestamp,client,connection_id,method,outcome FROM desktop_audit WHERE (?1 IS NULL OR connection_id=?1) AND (?2 IS NULL OR client=?2) AND (method LIKE ?3 OR outcome LIKE ?3) ORDER BY timestamp DESC,id DESC LIMIT ?4")?;
      let rows=stmt.query_map(params![connection,client,format!("%{q}%"),limit as i64],|r|Ok(json!({"id":r.get::<_,String>(0)?,"timestamp":r.get::<_,String>(1)?,"client":r.get::<_,String>(2)?,"connection_id":r.get::<_,Option<String>>(3)?,"method":r.get::<_,String>(4)?,"outcome":r.get::<_,String>(5)?})))?;
      Ok(rows.collect::<Result<Vec<_>,_>>()?)
    })?;
        if method == "audit.export" {
            Ok(json!({"csv":parse::csv(&items)}))
        } else {
            Ok(json!({"items":items}))
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_only_and_filterable() {
        let (_d, s) = crate::store::testutil::temp_store();
        record(&s, "1", Some("c"), "notes.set", "ok").unwrap();
        let n: i64 = s
            .conn
            .query_row("SELECT count(*) FROM desktop_audit", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        assert!(is_mutating("sec.keys.del"));
        assert!(!is_mutating("sec.keys.list"));
    }
}
