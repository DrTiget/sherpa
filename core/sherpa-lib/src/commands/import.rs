//! `import ssh-config preview|apply` (CORE_API §3.9).

use std::collections::HashMap;
use std::path::PathBuf;

use clap::{ArgMatches, Command};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{arg, many, repeated, sv, CommandGroup, Ctx};
use crate::error::{CoreError, CoreResult};
use crate::repo::conn::{self, ConnDraft, ConnInput};
use crate::sshconfig::{self, Candidate, Parsed};
use crate::store::Store;
use crate::util::{obj, parse};

pub struct Import;

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemOverride {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplyInput {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    preview_id: Option<String>,
    #[serde(default)]
    select: Option<Vec<String>>,
    #[serde(default)]
    group_id: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    overrides: HashMap<String, ItemOverride>,
}

impl CommandGroup for Import {
    fn name(&self) -> &'static str {
        "import"
    }

    fn command(&self) -> Command {
        Command::new("import")
            .about("Import connections")
            .subcommand_required(true)
            .subcommand(
                Command::new("ssh-config")
                    .about("Import from ~/.ssh/config")
                    .subcommand_required(true)
                    .subcommand(Command::new("preview").arg(arg("path")).arg(arg("group-id")))
                    .subcommand(
                        Command::new("apply")
                            .arg(arg("path"))
                            .arg(arg("preview-id"))
                            .arg(repeated("select"))
                            .arg(arg("group-id"))
                            .arg(repeated("tag")),
                    ),
            )
    }

    fn run(&self, ctx: &mut Ctx, m: &ArgMatches) -> CoreResult<Value> {
        let Some(("ssh-config", m)) = m.subcommand() else {
            return Err(CoreError::new("request.unknown_command", "Unknown import source").with("command", "import"));
        };
        match m.subcommand() {
            Some(("preview", m)) => {
                let p = ctx.params(obj(vec![("path", sv(m, "path")), ("group_id", sv(m, "group-id"))]))?;
                let path = path_of(super::p_str(&p, "path")?);
                let group_id = super::p_str(&p, "group_id")?;
                let store = ctx.store()?;
                if let Some(g) = &group_id {
                    crate::repo::group::ensure_exists(store, g)?;
                }
                let parsed = sshconfig::parse(&path)?;
                preview(store, &parsed, group_id.as_deref())
            }
            Some(("apply", m)) => {
                let select = many(m, "select");
                let tags = many(m, "tag");
                let flags = obj(vec![
                    ("path", sv(m, "path")),
                    ("preview_id", sv(m, "preview-id")),
                    ("select", if select.is_empty() { None } else { Some(json!(select)) }),
                    ("group_id", sv(m, "group-id")),
                    ("tags", if tags.is_empty() { None } else { Some(json!(tags)) }),
                ]);
                let inp: ApplyInput = parse(ctx.params(flags)?)?;
                apply(ctx.store()?, inp)
            }
            _ => Err(CoreError::new("request.unknown_command", "Unknown import command").with("command", "import")),
        }
    }
}

fn path_of(p: Option<String>) -> PathBuf {
    p.map(|p| crate::ssh::target::expand_tilde(&p))
        .unwrap_or_else(sshconfig::default_path)
}

fn candidate_connection(c: &Candidate, group_id: Option<&str>, tags: &[String]) -> Value {
    let auth = match &c.identity_file {
        Some(f) => json!({ "method": "key_file", "key_path": f }),
        None => json!({ "method": "password" }),
    };
    json!({
        "name": c.key,
        "host": c.host,
        "port": c.port,
        "user": c.user,
        "auth": auth,
        "group_id": group_id,
        "tags": tags,
    })
}

fn origin(c: &Candidate) -> Value {
    json!({ "file": c.file.display().to_string(), "line": c.line })
}

fn preview(store: &Store, parsed: &Parsed, group_id: Option<&str>) -> CoreResult<Value> {
    let mut items = Vec::new();
    for c in &parsed.candidates {
        let mut item = json!({ "key": c.key, "origin": origin(c) });
        if c.duplicate_in_file {
            item["action"] = "skip".into();
            item["reason"] = "duplicate_in_file".into();
        } else if let Some(existing) = conn::find_by_name(store, &c.key)? {
            item["action"] = "skip".into();
            item["reason"] = "name_exists".into();
            item["existing_id"] = existing.into();
            item["connection"] = candidate_connection(c, group_id, &[]);
        } else {
            item["action"] = "create".into();
            item["connection"] = candidate_connection(c, group_id, &[]);
            item["defaults_from"] = json!(c.defaults_from);
            item["warnings"] = serde_json::to_value(&c.warnings)?;
        }
        items.push(item);
    }
    Ok(json!({
        "preview_id": parsed.digest,
        "source": parsed.source.display().to_string(),
        "files": parsed.files.iter().map(|f| f.display().to_string()).collect::<Vec<_>>(),
        "items": items,
        "ignored_directives": parsed.ignored,
        "warnings": parsed.warnings,
    }))
}

fn apply(store: &mut Store, inp: ApplyInput) -> CoreResult<Value> {
    let path = path_of(inp.path.clone());
    let parsed = sshconfig::parse(&path)?;
    if let Some(id) = &inp.preview_id {
        if *id != parsed.digest {
            return Err(
                CoreError::new("import.source_changed", "ssh config changed after preview")
                    .with("path", path.display().to_string()),
            );
        }
    }
    if let Some(g) = &inp.group_id {
        crate::repo::group::ensure_exists(store, g)?;
    }
    let tags = crate::repo::tag::normalize_all(&inp.tags)?;
    let firsts: Vec<&Candidate> = parsed.candidates.iter().filter(|c| !c.duplicate_in_file).collect();
    let selected: Vec<String> = match &inp.select {
        Some(s) => s.clone(),
        None => firsts.iter().map(|c| c.key.clone()).collect(),
    };

    let mut created = Vec::new();
    let mut skipped = Vec::new();
    let mut failed = Vec::new();
    for key in &selected {
        if !firsts.iter().any(|c| &c.key == key) {
            let e = CoreError::new("import.unknown_key", format!("No such host in ssh config: {key}"))
                .with("key", key.as_str());
            failed.push(json!({ "key": key, "error": e }));
        }
    }

    store.conn.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| -> CoreResult<()> {
        for c in &firsts {
            if !selected.contains(&c.key) {
                skipped.push(json!({ "key": c.key, "reason": "not_selected" }));
                continue;
            }
            let ov = inp.overrides.get(&c.key);
            let name = ov.and_then(|o| o.name.clone()).unwrap_or_else(|| c.key.clone());
            if conn::find_by_name(store, &name)?.is_some() {
                skipped.push(json!({ "key": c.key, "reason": "name_exists" }));
                continue;
            }
            let mut v = candidate_connection(c, inp.group_id.as_deref(), &tags);
            v["name"] = name.into();
            store.conn.execute_batch("SAVEPOINT item")?;
            let r = parse::<ConnInput>(v)
                .and_then(|i| ConnDraft::from_input(store, i, "ssh_config"))
                .and_then(|d| conn::insert(store, d));
            match r {
                Ok(conn) => {
                    store.conn.execute_batch("RELEASE item")?;
                    created.push(json!({ "key": c.key, "id": conn.id }));
                }
                Err(e) => {
                    store.conn.execute_batch("ROLLBACK TO item; RELEASE item")?;
                    failed.push(json!({ "key": c.key, "error": e }));
                }
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => store.conn.execute_batch("COMMIT")?,
        Err(e) => {
            let _ = store.conn.execute_batch("ROLLBACK");
            return Err(e);
        }
    }
    Ok(json!({ "created": created, "skipped": skipped, "failed": failed }))
}
