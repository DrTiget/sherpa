//! Хранилище ядра: SQLite + ключ-файл.

pub mod crypto;
pub mod migrations;

use std::cell::OnceCell;

use rusqlite::{Connection, OpenFlags, OptionalExtension};

use crate::config::Settings;
use crate::error::{CoreError, CoreResult};
use crate::protocol::Warning;
use crypto::MasterKey;

pub struct Store {
    pub conn: Connection,
    pub settings: Settings,
    key: OnceCell<MasterKey>,
    pub warnings: Vec<Warning>,
}

impl Store {
    /// Открыть существующее хранилище (с автоматическими миграциями).
    pub fn open(settings: &Settings) -> CoreResult<Store> {
        if !settings.db_path.exists() {
            return Err(CoreError::new("store.not_initialized", "Store is not initialized, run `init`")
                .with("db", settings.db_path.display().to_string()));
        }
        let conn = Connection::open_with_flags(&settings.db_path, OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(|e| map_open_err(settings, e))?;
        Self::finish_open(conn, settings)
    }

    /// Создать БД, если её нет (для `init`). Возвращает (store, создана_ли).
    pub fn open_or_create(settings: &Settings) -> CoreResult<(Store, bool)> {
        let existed = settings.db_path.exists();
        if let Some(dir) = settings.db_path.parent() {
            crypto::create_private_dir(dir)?;
        }
        let conn = Connection::open(&settings.db_path).map_err(|e| map_open_err(settings, e))?;
        Ok((Self::finish_open(conn, settings)?, !existed))
    }

    fn finish_open(mut conn: Connection, settings: &Settings) -> CoreResult<Store> {
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "journal_mode", "WAL").ok();
        migrate(&mut conn)?;
        Ok(Store {
            conn,
            settings: settings.clone(),
            key: OnceCell::new(),
            warnings: Vec::new(),
        })
    }

    pub fn meta_get(&self, key: &str) -> CoreResult<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    pub fn meta_set(&self, key: &str, value: &str) -> CoreResult<()> {
        self.conn.execute(
            "INSERT INTO meta(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    pub fn schema_version(&self) -> CoreResult<u32> {
        schema_version(&self.conn)
    }

    pub fn has_secrets(&self) -> CoreResult<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM connections WHERE secret_password IS NOT NULL
                OR secret_key_text IS NOT NULL OR secret_passphrase IS NOT NULL)",
            [],
            |r| r.get(0),
        )?)
    }

    /// Ключ шифрования; загружается при первом обращении и сверяется с БД.
    pub fn key(&mut self) -> CoreResult<&MasterKey> {
        if self.key.get().is_none() {
            let path = &self.settings.key_file_path;
            let key = MasterKey::load(path)?;
            match self.meta_get("key_fingerprint")? {
                Some(fp) if fp != key.fingerprint() => {
                    return Err(CoreError::new("crypto.key_mismatch", "Key file does not match the database")
                        .with("path", path.display().to_string()));
                }
                Some(_) => {}
                None => self.meta_set("key_fingerprint", &key.fingerprint())?,
            }
            if let Some(w) = crypto::permission_warning(path) {
                tracing::warn!("{}", w.message);
                self.warnings.push(w);
            }
            let _ = self.key.set(key);
        }
        Ok(self.key.get().expect("key loaded"))
    }

    pub fn encrypt(&mut self, aad: &str, plain: &str) -> CoreResult<Vec<u8>> {
        self.key()?.encrypt(aad, plain.as_bytes())
    }

    pub fn decrypt_string(&mut self, aad: &str, blob: &[u8]) -> CoreResult<crate::secret::Secret> {
        let field = aad.rsplit(':').next().unwrap_or("").to_string();
        let plain = self.key()?.decrypt(aad, blob)?;
        let s = String::from_utf8(plain.to_vec())
            .map_err(|_| CoreError::new("crypto.decrypt_failed", "Secret is not UTF-8").with("field", field))?;
        Ok(crate::secret::Secret::new(s))
    }
}

fn map_open_err(settings: &Settings, e: rusqlite::Error) -> CoreError {
    let path = settings.db_path.display().to_string();
    match &e {
        rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::CannotOpen => {
            CoreError::new("store.permission_denied", format!("Cannot open database: {e}")).with("path", path)
        }
        _ => CoreError::from(e),
    }
}

fn schema_version(conn: &Connection) -> CoreResult<u32> {
    let has_meta: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='meta')",
        [],
        |r| r.get(0),
    )?;
    if !has_meta {
        return Ok(0);
    }
    let v: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key='schema_version'", [], |r| r.get(0))
        .optional()?;
    Ok(v.and_then(|s| s.parse().ok()).unwrap_or(0))
}

fn migrate(conn: &mut Connection) -> CoreResult<()> {
    let current = schema_version(conn)?;
    let latest = migrations::latest();
    if current > latest {
        return Err(CoreError::new("store.schema_too_new", "Database was created by a newer core")
            .with("schema", current)
            .with("supported", latest));
    }
    for v in current..latest {
        let tx = conn.transaction()?;
        let fail = |e: rusqlite::Error| {
            CoreError::new("store.migration_failed", format!("Migration failed: {e}"))
                .with("from", v)
                .with("to", v + 1)
        };
        tx.execute_batch(migrations::MIGRATIONS[v as usize]).map_err(fail)?;
        tx.execute(
            "INSERT INTO meta(key, value) VALUES ('schema_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [(v + 1).to_string()],
        )
        .map_err(fail)?;
        tx.commit().map_err(fail)?;
        tracing::info!("migrated schema {} -> {}", v, v + 1);
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod testutil {
    use super::*;

    /// Временное хранилище с ключом — для юнит-тестов.
    pub fn temp_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let settings = crate::config::Settings::load(&crate::config::Overrides {
            config: Some(dir.path().join("core.toml")),
            db: Some(dir.path().join("s.db")),
            key_file: Some(dir.path().join("s.key")),
        })
        .unwrap();
        let key = MasterKey::generate();
        key.write_new(&settings.key_file_path).unwrap();
        let (store, _) = Store::open_or_create(&settings).unwrap();
        store.meta_set("key_fingerprint", &key.fingerprint()).unwrap();
        (dir, store)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_and_rejects_newer_schema() {
        let (_d, store) = testutil::temp_store();
        assert_eq!(store.schema_version().unwrap(), migrations::latest());
        store.meta_set("schema_version", "999").unwrap();
        let settings = store.settings.clone();
        drop(store);
        assert_eq!(Store::open(&settings).err().unwrap().code, "store.schema_too_new");
    }

    #[test]
    fn key_mismatch_detected() {
        let (_d, mut store) = testutil::temp_store();
        store.meta_set("key_fingerprint", "SHA256:other").unwrap();
        assert_eq!(store.key().err().unwrap().code, "crypto.key_mismatch");
    }
}
