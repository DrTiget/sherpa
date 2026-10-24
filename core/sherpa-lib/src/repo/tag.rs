//! Теги: создаются неявно через подключения, пустые исчезают сами.

use serde_json::{json, Value};

use crate::error::{CoreError, CoreResult};
use crate::store::Store;

pub const MAX_TAG_LEN: usize = 64;

/// Нормализация: обрезка, нижний регистр, без пробелов внутри и запятых.
pub fn normalize(tag: &str) -> CoreResult<String> {
    let t = tag.trim().to_lowercase();
    if t.is_empty() || t.chars().count() > MAX_TAG_LEN || t.chars().any(|c| c.is_whitespace() || c == ',') {
        return Err(CoreError::new("tag.invalid", format!("Invalid tag name: {tag:?}")).with("name", tag));
    }
    Ok(t)
}

pub fn normalize_all(tags: &[String]) -> CoreResult<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for t in tags {
        let n = normalize(t)?;
        if !out.contains(&n) {
            out.push(n);
        }
    }
    out.sort();
    Ok(out)
}

pub fn list(store: &Store) -> CoreResult<Value> {
    let mut st = store
        .conn
        .prepare("SELECT tag, COUNT(*) FROM connection_tags GROUP BY tag ORDER BY tag")?;
    let items = st
        .query_map([], |r| {
            Ok(json!({ "name": r.get::<_, String>(0)?, "connection_count": r.get::<_, i64>(1)? }))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({ "items": items }))
}

fn exists(store: &Store, tag: &str) -> CoreResult<bool> {
    Ok(store.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM connection_tags WHERE tag = ?1)",
        [tag],
        |r| r.get(0),
    )?)
}

fn not_found(tag: &str) -> CoreError {
    CoreError::new("tag.not_found", format!("Tag not found: {tag}")).with("name", tag)
}

/// Переименовать; если новое имя уже есть — слияние.
pub fn rename(store: &mut Store, old: &str, new: &str) -> CoreResult<Value> {
    let old = normalize(old)?;
    let new = normalize(new)?;
    if !exists(store, &old)? {
        return Err(not_found(&old));
    }
    let tx = store.conn.transaction()?;
    tx.execute(
        "INSERT OR IGNORE INTO connection_tags(connection_id, tag)
         SELECT connection_id, ?2 FROM connection_tags WHERE tag = ?1",
        [&old, &new],
    )?;
    if old != new {
        tx.execute("DELETE FROM connection_tags WHERE tag = ?1", [&old])?;
    }
    let count: i64 = tx.query_row("SELECT COUNT(*) FROM connection_tags WHERE tag = ?1", [&new], |r| {
        r.get(0)
    })?;
    tx.commit()?;
    Ok(json!({ "tag": { "name": new, "connection_count": count } }))
}

pub fn delete(store: &Store, tag: &str) -> CoreResult<Value> {
    let tag = normalize(tag)?;
    let n = store
        .conn
        .execute("DELETE FROM connection_tags WHERE tag = ?1", [&tag])?;
    if n == 0 {
        return Err(not_found(&tag));
    }
    Ok(json!({ "deleted": tag, "connection_count": n }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes() {
        assert_eq!(normalize("  Prod ").unwrap(), "prod");
        assert!(normalize("a b").is_err());
        assert!(normalize("").is_err());
        assert_eq!(
            normalize_all(&["B".into(), "a".into(), "b".into()]).unwrap(),
            vec!["a", "b"]
        );
    }
}
