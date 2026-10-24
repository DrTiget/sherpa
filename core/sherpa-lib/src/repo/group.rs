//! Группы — дерево папок.

use std::collections::HashSet;

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Value};

use crate::error::{CoreError, CoreResult};
use crate::store::Store;
use crate::util::{new_id, now};

pub const MAX_NAME_LEN: usize = 128;

#[derive(Debug, Clone, Serialize)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub connection_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

pub fn not_found(id: &str) -> CoreError {
    CoreError::new("group.not_found", format!("Group not found: {id}")).with("id", id)
}

pub fn all(store: &Store) -> CoreResult<Vec<Group>> {
    let mut st = store.conn.prepare(
        "SELECT g.id, g.name, g.parent_id, g.created_at, g.updated_at,
                (SELECT COUNT(*) FROM connections c WHERE c.group_id = g.id)
         FROM groups g ORDER BY g.name_lc",
    )?;
    let rows = st
        .query_map([], |r| {
            Ok(Group {
                id: r.get(0)?,
                name: r.get(1)?,
                parent_id: r.get(2)?,
                created_at: r.get(3)?,
                updated_at: r.get(4)?,
                connection_count: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(store: &Store, id: &str) -> CoreResult<Group> {
    all(store)?.into_iter().find(|g| g.id == id).ok_or_else(|| not_found(id))
}

pub fn ensure_exists(store: &Store, id: &str) -> CoreResult<()> {
    let ok: bool = store
        .conn
        .query_row("SELECT EXISTS(SELECT 1 FROM groups WHERE id = ?1)", [id], |r| r.get(0))?;
    if ok {
        Ok(())
    } else {
        Err(not_found(id))
    }
}

/// Группа и все её потомки.
pub fn subtree(store: &Store, id: &str) -> CoreResult<HashSet<String>> {
    let groups = all(store)?;
    let mut set = HashSet::from([id.to_string()]);
    loop {
        let before = set.len();
        for g in &groups {
            if g.parent_id.as_ref().is_some_and(|p| set.contains(p)) {
                set.insert(g.id.clone());
            }
        }
        if set.len() == before {
            return Ok(set);
        }
    }
}

fn validate_name(name: &str) -> CoreResult<String> {
    let n = name.trim();
    if n.is_empty() {
        return Err(CoreError::required("name"));
    }
    if n.chars().count() > MAX_NAME_LEN {
        return Err(CoreError::too_long("name", MAX_NAME_LEN));
    }
    Ok(n.to_string())
}

fn check_name_free(store: &Store, name: &str, parent: Option<&str>, except: Option<&str>) -> CoreResult<()> {
    let taken: Option<String> = store
        .conn
        .query_row(
            "SELECT id FROM groups WHERE name_lc = ?1 AND parent_id IS ?2 AND id IS NOT ?3",
            params![name.to_lowercase(), parent, except],
            |r| r.get(0),
        )
        .optional()?;
    match taken {
        Some(_) => Err(CoreError::new("group.name_taken", format!("Group name is taken: {name}"))
            .with("name", name)
            .with("parent_id", parent.map(Value::from).unwrap_or(Value::Null))),
        None => Ok(()),
    }
}

pub fn create(store: &Store, name: &str, parent_id: Option<&str>) -> CoreResult<Group> {
    let name = validate_name(name)?;
    if let Some(p) = parent_id {
        ensure_exists(store, p)?;
    }
    check_name_free(store, &name, parent_id, None)?;
    let id = new_id();
    let ts = now();
    store.conn.execute(
        "INSERT INTO groups(id, name, name_lc, parent_id, created_at, updated_at) VALUES (?1,?2,?3,?4,?5,?5)",
        params![id, name, name.to_lowercase(), parent_id, ts],
    )?;
    get(store, &id)
}

/// `parent`: None — не менять; Some(None) — в корень; Some(Some(id)) — перенести.
pub fn update(store: &Store, id: &str, name: Option<&str>, parent: Option<Option<&str>>) -> CoreResult<Group> {
    let cur = get(store, id)?;
    let name = match name {
        Some(n) => validate_name(n)?,
        None => cur.name.clone(),
    };
    let parent_id: Option<String> = match parent {
        None => cur.parent_id.clone(),
        Some(None) => None,
        Some(Some(p)) => {
            ensure_exists(store, p)?;
            if subtree(store, id)?.contains(p) {
                return Err(CoreError::new("group.cycle", "Group cannot be moved into its descendant")
                    .with("id", id)
                    .with("parent_id", p));
            }
            Some(p.to_string())
        }
    };
    check_name_free(store, &name, parent_id.as_deref(), Some(id))?;
    store.conn.execute(
        "UPDATE groups SET name=?2, name_lc=?3, parent_id=?4, updated_at=?5 WHERE id=?1",
        params![id, name, name.to_lowercase(), parent_id, now()],
    )?;
    get(store, id)
}

pub fn delete(store: &mut Store, id: &str, mode: &str) -> CoreResult<Value> {
    let g = get(store, id)?;
    let tree = subtree(store, id)?;
    let tx = store.conn.transaction()?;
    let mut deleted_connections = 0usize;
    match mode {
        "move_to_parent" => {
            tx.execute("UPDATE connections SET group_id = ?2 WHERE group_id = ?1", params![id, g.parent_id])?;
            // Подгруппы переезжают к родителю; при конфликте имени — ошибка.
            let mut st = tx.prepare("SELECT name_lc, name FROM groups WHERE parent_id = ?1")?;
            let children = st
                .query_map([id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            drop(st);
            for (lc, name) in children {
                let clash: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM groups WHERE name_lc = ?1 AND parent_id IS ?2 AND id != ?3)",
                    params![lc, g.parent_id, id],
                    |r| r.get(0),
                )?;
                if clash {
                    return Err(CoreError::new("group.name_taken", format!("Group name is taken: {name}"))
                        .with("name", name)
                        .with("parent_id", g.parent_id.clone().map(Value::from).unwrap_or(Value::Null)));
                }
            }
            tx.execute("UPDATE groups SET parent_id = ?2 WHERE parent_id = ?1", params![id, g.parent_id])?;
            tx.execute("DELETE FROM groups WHERE id = ?1", [id])?;
        }
        "delete_connections" => {
            for gid in &tree {
                deleted_connections += tx.execute("DELETE FROM connections WHERE group_id = ?1", [gid])?;
            }
            // Удаляем от листьев к корню, чтобы не нарушить внешние ключи.
            let mut left: Vec<String> = tree.iter().cloned().collect();
            while !left.is_empty() {
                let before = left.len();
                let mut next = Vec::new();
                for gid in left {
                    let has_child: bool = tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM groups WHERE parent_id = ?1)",
                        [&gid],
                        |r| r.get(0),
                    )?;
                    if has_child {
                        next.push(gid);
                    } else {
                        tx.execute("DELETE FROM groups WHERE id = ?1", [&gid])?;
                    }
                }
                if next.len() == before {
                    return Err(CoreError::internal("group tree delete stalled"));
                }
                left = next;
            }
        }
        other => return Err(CoreError::invalid_args("--mode", format!("unknown mode {other}"))),
    }
    tx.commit()?;
    Ok(json!({
        "deleted": tree.len().max(1),
        "deleted_connections": deleted_connections,
        "mode": mode,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::testutil::temp_store;

    #[test]
    fn tree_rules() {
        let (_d, mut s) = temp_store();
        let a = create(&s, "Prod", None).unwrap();
        let b = create(&s, "DB", Some(&a.id)).unwrap();
        assert_eq!(create(&s, "prod", None).unwrap_err().code, "group.name_taken");
        create(&s, "DB", None).unwrap(); // другой уровень — можно
        assert_eq!(update(&s, &a.id, None, Some(Some(&b.id))).unwrap_err().code, "group.cycle");
        let c = create(&s, "Inner", Some(&b.id)).unwrap();
        delete(&mut s, &b.id, "move_to_parent").unwrap();
        assert_eq!(get(&s, &c.id).unwrap().parent_id.as_deref(), Some(a.id.as_str()));
        let r = delete(&mut s, &a.id, "delete_connections").unwrap();
        assert_eq!(r["deleted"], 2);
        assert!(get(&s, &c.id).is_err());
    }
}
