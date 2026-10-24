//! Markdown и зашифрованные секреты. Общий репозиторий документов утилит.
use crate::{
    daemon::{
        proto::{opt_str, req_str},
        registry::{Cx, MethodGroup},
    },
    error::{CoreError, CoreResult},
    store::Store,
    util,
};
use async_trait::async_trait;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

pub fn get(s: &Store, kind: &str, id: &str) -> CoreResult<Value> {
    let body: Option<String> = s
        .conn
        .query_row(
            "SELECT body FROM desktop_documents WHERE kind=?1 AND id=?2",
            params![kind, id],
            |r| r.get(0),
        )
        .optional()?;
    serde_json::from_str(
        &body.ok_or_else(|| CoreError::new("document.not_found", "Document not found").with("id", id))?,
    )
    .map_err(|_| CoreError::new("store.bad_document", "Stored document is invalid"))
}
pub fn list(s: &Store, kind: &str, connection: Option<&str>) -> CoreResult<Value> {
    let mut stmt=s.conn.prepare("SELECT body FROM desktop_documents WHERE kind=?1 AND (?2 IS NULL OR connection_id=?2) ORDER BY updated_at DESC,id")?;
    let rows = stmt.query_map(params![kind, connection], |r| r.get::<_, String>(0))?;
    let mut items = Vec::new();
    for row in rows {
        items.push(
            serde_json::from_str::<Value>(&row?)
                .map_err(|_| CoreError::new("store.bad_document", "Stored document is invalid"))?,
        );
    }
    Ok(json!({"items":items}))
}
pub fn put(s: &Store, kind: &str, id: &str, body: &Value) -> CoreResult<()> {
    s.conn.execute("INSERT INTO desktop_documents(kind,id,connection_id,body,updated_at) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(kind,id) DO UPDATE SET connection_id=excluded.connection_id,body=excluded.body,updated_at=excluded.updated_at",params![kind,id,body["connection_id"].as_str(),body.to_string(),util::now()])?;
    Ok(())
}
pub fn delete(s: &Store, kind: &str, id: &str) -> CoreResult<Value> {
    let n = s.conn.execute(
        "DELETE FROM desktop_documents WHERE kind=?1 AND id=?2",
        params![kind, id],
    )?;
    if n == 0 {
        return Err(CoreError::new("document.not_found", "Document not found").with("id", id));
    }
    Ok(json!({"deleted":true}))
}
pub fn library(s: &mut Store, kind: &str, action: &str, p: &Value) -> CoreResult<Value> {
    match action {
        "list" => list(s, kind, opt_str(p, "connection_id")?.as_deref()),
        "get" => get(s, kind, &req_str(p, "id")?),
        "del" => delete(s, kind, &req_str(p, "id")?),
        "set" => {
            let id = opt_str(p, "id")?.unwrap_or_else(util::new_id);
            let title = req_str(p, "title")?;
            let connection = req_str(p, "connection_id")?;
            let mut body = json!({"id":id,"title":title,"connection_id":connection,"updated_at":util::now()});
            if kind == "vault" {
                let value = req_str(p, "value")?;
                body["note_id"] = opt_str(p, "note_id")?.into();
                let encrypted = s.encrypt(&format!("vault:{id}:value"), &value)?;
                body["has_secret"] = true.into();
                put(s, kind, &id, &body)?;
                s.conn.execute(
                    "UPDATE desktop_documents SET secret=?1 WHERE kind='vault' AND id=?2",
                    params![encrypted, id],
                )?;
            } else {
                body["markdown"] = req_str(p, "markdown")?.into();
                put(s, kind, &id, &body)?;
            }
            Ok(body)
        }
        _ => Err(CoreError::invalid("action", "unsupported action")),
    }
}
pub struct NotesMethods;
#[async_trait]
impl MethodGroup for NotesMethods {
    fn prefix(&self) -> &'static str {
        "notes"
    }
    fn methods(&self) -> &'static [&'static str] {
        &[
            "notes.list",
            "notes.get",
            "notes.set",
            "notes.del",
            "notes.secret.list",
            "notes.secret.set",
            "notes.secret.del",
            "notes.secret.reveal",
        ]
    }
    async fn call(&self, cx: &Cx, method: &str, p: Value) -> CoreResult<Value> {
        let secret = method.starts_with("notes.secret.") || method.starts_with("vault.");
        let action = method.rsplit('.').next().unwrap_or("");
        if action == "del" || action == "reveal" {
            cx.state.confirmations.consume(cx.client.id, method, &p)?;
        }
        if action == "reveal" {
            let id = req_str(&p, "id")?;
            return cx.state.with_store(|s| {
                let blob: Option<Vec<u8>> = s
                    .conn
                    .query_row(
                        "SELECT secret FROM desktop_documents WHERE kind='vault' AND id=?1",
                        [&id],
                        |r| r.get(0),
                    )
                    .optional()?
                    .flatten();
                let blob = blob.ok_or_else(|| CoreError::new("document.not_found", "Secret not found"))?;
                let value = s.decrypt_string(&format!("vault:{id}:value"), &blob)?;
                Ok(json!({"id":id,"value":value.expose(),"visible_for_sec":10}))
            });
        }
        cx.state
            .with_store(|s| library(s, if secret { "vault" } else { "notes" }, action, &p))
    }
}

/// Совместимые имена из DESKTOP §4.11.11.
pub struct VaultMethods;
#[async_trait]
impl MethodGroup for VaultMethods {
    fn prefix(&self) -> &'static str {
        "vault"
    }
    fn methods(&self) -> &'static [&'static str] {
        &["vault.list", "vault.set", "vault.del", "vault.reveal"]
    }
    async fn call(&self, cx: &Cx, method: &str, p: Value) -> CoreResult<Value> {
        NotesMethods.call(cx, method, p).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encrypted_and_redacted() {
        let (_d, mut s) = crate::store::testutil::temp_store();
        let p = json!({"title":"Grafana","value":"top-secret","connection_id":"c"});
        let v = library(&mut s, "vault", "set", &p).unwrap();
        let id = v["id"].as_str().unwrap();
        assert!(!v.to_string().contains("top-secret"));
        assert!(!list(&s, "vault", None).unwrap().to_string().contains("top-secret"));
        let blob: Vec<u8> = s
            .conn
            .query_row("SELECT secret FROM desktop_documents WHERE id=?1", [id], |r| r.get(0))
            .unwrap();
        assert!(!String::from_utf8_lossy(&blob).contains("top-secret"));
        assert_eq!(
            s.decrypt_string(&format!("vault:{id}:value"), &blob).unwrap().expose(),
            "top-secret"
        );
    }
    #[test]
    fn notes_roundtrip() {
        let (_d, mut s) = crate::store::testutil::temp_store();
        let v = library(
            &mut s,
            "notes",
            "set",
            &json!({"title":"a","markdown":"# Hello","connection_id":"c"}),
        )
        .unwrap();
        assert_eq!(
            get(&s, "notes", v["id"].as_str().unwrap()).unwrap()["markdown"],
            "# Hello"
        );
        assert_eq!(list(&s, "notes", Some("other")).unwrap()["items"], json!([]));
        delete(&s, "notes", v["id"].as_str().unwrap()).unwrap();
    }
}
