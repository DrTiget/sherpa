//! Host keys по TOFU (CORE_API §3.8, §6.4). Ключ привязан к (host, port).

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Value};

use crate::error::{CoreError, CoreResult};
use crate::store::Store;
use crate::util::now;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HostKey {
    pub host: String,
    pub port: u16,
    pub key_type: String,
    pub fingerprint: String,
    pub state: String,
    pub first_seen_at: String,
    pub accepted_at: Option<String>,
    pub replaced_fingerprint: Option<String>,
}

/// Ключ, предъявленный сервером.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Presented {
    pub key_type: String,
    pub fingerprint: String,
}

/// Решение по предъявленному ключу.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Trusted,
    Unknown,
    Rejected,
    Mismatch { known: HostKey },
}

pub fn verify(known: &[HostKey], p: &Presented) -> Verdict {
    if known
        .iter()
        .any(|k| k.state == "rejected" && k.fingerprint == p.fingerprint)
    {
        return Verdict::Rejected;
    }
    let trusted = known.iter().filter(|k| k.state == "trusted");
    let mut same_type = None;
    for k in trusted {
        if k.fingerprint == p.fingerprint {
            return Verdict::Trusted;
        }
        if k.key_type == p.key_type {
            same_type = Some(k.clone());
        }
    }
    match same_type {
        Some(known) => Verdict::Mismatch { known },
        None => Verdict::Unknown,
    }
}

/// Ошибка для решения (кроме Trusted).
pub fn verdict_error(host: &str, port: u16, p: &Presented, v: &Verdict) -> Option<CoreError> {
    let base = |code: &str, msg: &str| {
        CoreError::new(code, msg)
            .with("host", host)
            .with("port", port)
            .with("key_type", p.key_type.clone())
            .with("fingerprint", p.fingerprint.clone())
    };
    match v {
        Verdict::Trusted => None,
        Verdict::Unknown => Some(base("hostkey.unknown", "Unknown host key")),
        Verdict::Rejected => Some(
            CoreError::new("hostkey.rejected", "Host key was rejected by the user")
                .with("host", host)
                .with("port", port)
                .with("fingerprint", p.fingerprint.clone()),
        ),
        Verdict::Mismatch { known } => Some(
            base("hostkey.mismatch", "Host key has changed")
                .with("known_fingerprint", known.fingerprint.clone())
                .with(
                    "known_accepted_at",
                    known.accepted_at.clone().map(Value::from).unwrap_or(Value::Null),
                ),
        ),
    }
}

const COLS: &str = "host, port, key_type, fingerprint, state, first_seen_at, accepted_at, replaced_fingerprint";

fn row(r: &rusqlite::Row) -> rusqlite::Result<HostKey> {
    Ok(HostKey {
        host: r.get(0)?,
        port: r.get::<_, i64>(1)? as u16,
        key_type: r.get(2)?,
        fingerprint: r.get(3)?,
        state: r.get(4)?,
        first_seen_at: r.get(5)?,
        accepted_at: r.get(6)?,
        replaced_fingerprint: r.get(7)?,
    })
}

pub fn list(store: &Store, host: Option<&str>, port: Option<u16>) -> CoreResult<Vec<HostKey>> {
    let mut st = store.conn.prepare(&format!(
        "SELECT {COLS} FROM host_keys WHERE (?1 IS NULL OR host = ?1) AND (?2 IS NULL OR port = ?2)
         ORDER BY host, port, state DESC, key_type"
    ))?;
    let rows = st.query_map(params![host, port], row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn for_addr(store: &Store, host: &str, port: u16) -> CoreResult<Vec<HostKey>> {
    list(store, Some(host), Some(port))
}

/// Сохранить ключ как доверенный. Другой ключ того же типа — только с `replace`.
pub fn trust(store: &mut Store, host: &str, port: u16, p: &Presented, replace: bool) -> CoreResult<HostKey> {
    let known = for_addr(store, host, port)?;
    let mut replaced = None;
    if let Verdict::Mismatch { known } = verify(&known, p) {
        if !replace {
            return Err(verdict_error(host, port, p, &Verdict::Mismatch { known }).expect("error"));
        }
        replaced = Some(known.fingerprint);
    }
    let ts = now();
    let tx = store.conn.transaction()?;
    if let Some(old) = &replaced {
        tx.execute(
            "DELETE FROM host_keys WHERE host=?1 AND port=?2 AND fingerprint=?3",
            params![host, port, old],
        )?;
    }
    let first_seen: Option<String> = tx
        .query_row(
            "SELECT first_seen_at FROM host_keys WHERE host=?1 AND port=?2 AND fingerprint=?3",
            params![host, port, p.fingerprint],
            |r| r.get(0),
        )
        .optional()?;
    tx.execute(
        "INSERT INTO host_keys(host, port, key_type, fingerprint, state, first_seen_at, accepted_at, replaced_fingerprint)
         VALUES (?1,?2,?3,?4,'trusted',?5,?6,?7)
         ON CONFLICT(host, port, fingerprint) DO UPDATE SET state='trusted', key_type=excluded.key_type,
            accepted_at=excluded.accepted_at, replaced_fingerprint=excluded.replaced_fingerprint",
        params![host, port, p.key_type, p.fingerprint, first_seen.unwrap_or_else(|| ts.clone()), ts, replaced],
    )?;
    tx.commit()?;
    get(store, host, port, &p.fingerprint)
}

pub fn get(store: &Store, host: &str, port: u16, fingerprint: &str) -> CoreResult<HostKey> {
    store
        .conn
        .query_row(
            &format!("SELECT {COLS} FROM host_keys WHERE host=?1 AND port=?2 AND fingerprint=?3"),
            params![host, port, fingerprint],
            row,
        )
        .optional()?
        .ok_or_else(|| not_found(host, port))
}

pub fn not_found(host: &str, port: u16) -> CoreError {
    CoreError::new("hostkey.not_found", format!("No stored host key for {host}:{port}"))
        .with("host", host)
        .with("port", port)
}

/// Запомнить отклонённый отпечаток.
pub fn reject(store: &Store, host: &str, port: u16, fingerprint: &str, key_type: Option<&str>) -> CoreResult<HostKey> {
    let ts = now();
    store.conn.execute(
        "INSERT INTO host_keys(host, port, key_type, fingerprint, state, first_seen_at)
         VALUES (?1,?2,?3,?4,'rejected',?5)
         ON CONFLICT(host, port, fingerprint) DO UPDATE SET state='rejected', accepted_at=NULL",
        params![host, port, key_type.unwrap_or(""), fingerprint, ts],
    )?;
    get(store, host, port, fingerprint)
}

pub fn forget(store: &Store, host: &str, port: u16, key_type: Option<&str>) -> CoreResult<Value> {
    let n = store.conn.execute(
        "DELETE FROM host_keys WHERE host=?1 AND port=?2 AND (?3 IS NULL OR key_type=?3)",
        params![host, port, key_type],
    )?;
    if n == 0 {
        return Err(not_found(host, port));
    }
    Ok(json!({ "forgotten": n }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::testutil::temp_store;

    fn p(t: &str, f: &str) -> Presented {
        Presented {
            key_type: t.into(),
            fingerprint: f.into(),
        }
    }

    #[test]
    fn tofu_flow() {
        let (_d, mut s) = temp_store();
        let a = p("ssh-ed25519", "SHA256:a");
        assert_eq!(verify(&for_addr(&s, "h", 22).unwrap(), &a), Verdict::Unknown);
        trust(&mut s, "h", 22, &a, false).unwrap();
        assert_eq!(verify(&for_addr(&s, "h", 22).unwrap(), &a), Verdict::Trusted);
        // другой порт — другой адрес
        assert_eq!(verify(&for_addr(&s, "h", 2222).unwrap(), &a), Verdict::Unknown);
        let b = p("ssh-ed25519", "SHA256:b");
        assert!(matches!(
            verify(&for_addr(&s, "h", 22).unwrap(), &b),
            Verdict::Mismatch { .. }
        ));
        assert_eq!(trust(&mut s, "h", 22, &b, false).unwrap_err().code, "hostkey.mismatch");
        let hk = trust(&mut s, "h", 22, &b, true).unwrap();
        assert_eq!(hk.replaced_fingerprint.as_deref(), Some("SHA256:a"));
        reject(&s, "h", 22, "SHA256:c", Some("ssh-rsa")).unwrap();
        assert_eq!(
            verify(&for_addr(&s, "h", 22).unwrap(), &p("ssh-rsa", "SHA256:c")),
            Verdict::Rejected
        );
        // новый тип ключа при известном другом — неизвестный, а не подмена
        assert_eq!(
            verify(&for_addr(&s, "h", 22).unwrap(), &p("ecdsa-sha2-nistp256", "SHA256:d")),
            Verdict::Unknown
        );
        assert_eq!(forget(&s, "h", 22, None).unwrap()["forgotten"], 2);
        assert_eq!(forget(&s, "h", 22, None).unwrap_err().code, "hostkey.not_found");
    }
}
