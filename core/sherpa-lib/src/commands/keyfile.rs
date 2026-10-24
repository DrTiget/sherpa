//! `keyfile export|status` (CORE_API §3.2).

use std::path::PathBuf;

use clap::{ArgMatches, Command};
use serde_json::{json, Value};

use super::{arg, flag, s, switch, CommandGroup, Ctx};
use crate::error::{CoreError, CoreResult};
use crate::store::crypto::{self, MasterKey};
use crate::util::now;

pub struct KeyFile;

impl CommandGroup for KeyFile {
    fn name(&self) -> &'static str {
        "keyfile"
    }

    fn command(&self) -> Command {
        Command::new("keyfile")
            .about("Key file backup and status")
            .subcommand_required(true)
            .subcommand(
                Command::new("export")
                    .about("Write a copy of the key file (permissions 600)")
                    .arg(arg("to").required(true).value_name("PATH"))
                    .arg(switch("overwrite")),
            )
            .subcommand(Command::new("status").about("Key file presence, match and permissions"))
    }

    fn run(&self, ctx: &mut Ctx, m: &ArgMatches) -> CoreResult<Value> {
        match m.subcommand() {
            Some(("export", m)) => {
                let to = PathBuf::from(s(m, "to").unwrap_or_default());
                let store = ctx.store()?;
                let key = store.key()?;
                key.export_to(&to, flag(m, "overwrite"))?;
                let fp = key.fingerprint();
                let ts = now();
                store.meta_set("key_last_export_at", &ts)?;
                Ok(json!({ "path": to.display().to_string(), "key_fingerprint": fp, "exported_at": ts }))
            }
            Some(("status", _)) => {
                let path = ctx.settings()?.key_file_path.clone();
                let store = ctx.store()?;
                let db_fp = store.meta_get("key_fingerprint")?;
                let last_export = store.meta_get("key_last_export_at")?;
                let present = path.exists();
                let (fp, matches) = match MasterKey::load(&path) {
                    Ok(k) => {
                        let fp = k.fingerprint();
                        let ok = db_fp.as_deref().is_none_or(|d| d == fp);
                        (Some(fp), ok)
                    }
                    Err(e) if e.code == "store.key_file_missing" => (None, false),
                    Err(e) => return Err(e),
                };
                Ok(json!({
                    "path": path.display().to_string(),
                    "present": present,
                    "matches_db": matches,
                    "permissions_ok": present && crypto::permissions_ok(&path),
                    "key_fingerprint": fp,
                    "last_export_at": last_export,
                }))
            }
            _ => Err(CoreError::new("request.unknown_command", "Unknown keyfile command").with("command", "keyfile")),
        }
    }
}
