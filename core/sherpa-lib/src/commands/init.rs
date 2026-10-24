//! `init` — создать конфиг, БД и ключ-файл (CORE_API §3.1).

use clap::{ArgMatches, Command};
use serde_json::{json, Value};

use super::{flag, switch, CommandGroup, Ctx};
use crate::config;
use crate::error::{CoreError, CoreResult};
use crate::store::crypto::{self, MasterKey};
use crate::store::Store;

pub struct Init;

impl CommandGroup for Init {
    fn name(&self) -> &'static str {
        "init"
    }

    fn command(&self) -> Command {
        Command::new("init")
            .about("Create config, database and key file (idempotent)")
            .arg(switch("force-new-key").help("Create a new key file; all saved secrets are erased"))
    }

    fn run(&self, ctx: &mut Ctx, m: &ArgMatches) -> CoreResult<Value> {
        let force = flag(m, "force-new-key");
        let s = ctx.settings()?.clone();

        let mut created_config = false;
        if !s.config_exists {
            if let Some(dir) = s.config_path.parent() {
                crypto::create_private_dir(dir)?;
            }
            std::fs::write(&s.config_path, config::config_template(&s.db_path, &s.key_file_path))
                .map_err(|e| CoreError::fs("fs.write_failed", &s.config_path, &e))?;
            created_config = true;
        }

        let (store, created_db) = Store::open_or_create(&s)?;
        let known_fp = store.meta_get("key_fingerprint")?;
        let mut created_key = false;
        let key = if s.key_file_path.exists() {
            let key = MasterKey::load(&s.key_file_path)?;
            match &known_fp {
                Some(fp) if *fp != key.fingerprint() => {
                    return Err(
                        CoreError::new("crypto.key_mismatch", "Key file does not match the database")
                            .with("path", s.key_file_path.display().to_string()),
                    );
                }
                Some(_) => {}
                None => store.meta_set("key_fingerprint", &key.fingerprint())?,
            }
            key
        } else {
            if known_fp.is_some() && !force {
                return Err(CoreError::new(
                    "store.key_file_missing",
                    "Key file is missing; restore it or use --force-new-key",
                )
                .with("path", s.key_file_path.display().to_string()));
            }
            let key = MasterKey::generate();
            key.write_new(&s.key_file_path)?;
            if known_fp.is_some() {
                // Старые секреты без старого ключа не расшифровать — удаляем.
                store.conn.execute(
                    "UPDATE connections SET secret_password=NULL, secret_key_text=NULL, secret_passphrase=NULL",
                    [],
                )?;
                store
                    .conn
                    .execute("DELETE FROM meta WHERE key='key_last_export_at'", [])?;
                tracing::warn!("new key file created, saved secrets erased");
            }
            store.meta_set("key_fingerprint", &key.fingerprint())?;
            created_key = true;
            key
        };
        if let Some(w) = crypto::permission_warning(&s.key_file_path) {
            ctx.warnings.push(w);
        }
        let exported = store.meta_get("key_last_export_at")?.is_some();
        let fp = key.fingerprint();
        ctx.set_store(store);
        Ok(json!({
            "created": { "config": created_config, "db": created_db, "key_file": created_key },
            "paths": {
                "config": s.config_path.display().to_string(),
                "db": s.db_path.display().to_string(),
                "key_file": s.key_file_path.display().to_string(),
            },
            "key_fingerprint": fp,
            "backup_recommended": created_key || !exported,
        }))
    }
}
