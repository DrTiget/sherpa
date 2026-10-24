//! `conn …` — подключения (CORE_API §3.3, §3.6).

use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::{json, Value};

use super::{arg, flag, many, num, port_arg, repeated, s, sv, switch, CommandGroup, Ctx};
use crate::error::{CoreError, CoreResult};
use crate::repo::conn::{self, ConnDraft, ConnInput, ListFilter};
use crate::util::{obj, parse};

pub struct Conn;

/// Простые поля подключения флагами (секретов среди них нет).
fn field_args(cmd: Command) -> Command {
    cmd.arg(arg("name"))
        .arg(arg("host"))
        .arg(port_arg())
        .arg(arg("user"))
        .arg(arg("description"))
        .arg(arg("group-id"))
        .arg(repeated("tag"))
        .arg(arg("auth").value_parser(["password", "key_file", "key_text"]))
        .arg(arg("key-path"))
}

fn field_flags(m: &ArgMatches, tags_given: bool) -> Value {
    let auth = match (s(m, "auth"), s(m, "key-path")) {
        (None, None) => None,
        (method, key_path) => Some(obj(vec![
            ("method", Some(method.unwrap_or_else(|| "key_file".into()).into())),
            ("key_path", key_path.map(Value::from)),
        ])),
    };
    obj(vec![
        ("name", sv(m, "name")),
        ("host", sv(m, "host")),
        ("port", num::<u16>(m, "port").map(Value::from)),
        ("user", sv(m, "user")),
        ("description", sv(m, "description")),
        ("group_id", sv(m, "group-id")),
        ("tags", if tags_given { Some(json!(many(m, "tag"))) } else { None }),
        ("auth", auth),
    ])
}

impl CommandGroup for Conn {
    fn name(&self) -> &'static str {
        "conn"
    }

    fn command(&self) -> Command {
        Command::new("conn")
            .about("Connections")
            .subcommand_required(true)
            .subcommand(field_args(
                Command::new("create").about("Create a connection (secrets via --input -)"),
            ))
            .subcommand(Command::new("get").arg(Arg::new("id").required(true)))
            .subcommand(
                Command::new("list")
                    .arg(arg("q"))
                    .arg(arg("host"))
                    .arg(repeated("tag"))
                    .arg(arg("group-id").conflicts_with("no-group"))
                    .arg(switch("no-group"))
                    .arg(switch("recursive"))
                    .arg(arg("status").value_parser(["online", "offline", "auth_failed", "error", "unknown"]))
                    .arg(arg("sort").value_parser(["name", "host", "status", "last_checked"]))
                    .arg(switch("desc"))
                    .arg(arg("limit").value_parser(clap::value_parser!(u32)))
                    .arg(arg("offset").value_parser(clap::value_parser!(u32))),
            )
            .subcommand(field_args(Command::new("update").arg(Arg::new("id").required(true))))
            .subcommand(Command::new("delete").arg(Arg::new("id").required(true).action(ArgAction::Append)))
            .subcommand(
                Command::new("duplicate")
                    .arg(Arg::new("id").required(true))
                    .arg(arg("name")),
            )
            .subcommand(super::check::command())
    }

    fn run(&self, ctx: &mut Ctx, m: &ArgMatches) -> CoreResult<Value> {
        match m.subcommand() {
            Some(("create", m)) => {
                let p = ctx.params(field_flags(m, true))?;
                let inp: ConnInput = parse(p)?;
                let store = ctx.store()?;
                let draft = ConnDraft::from_input(store, inp, "manual")?;
                let c = conn::insert(store, draft)?;
                Ok(json!({ "connection": c.to_json() }))
            }
            Some(("get", m)) => {
                let c = conn::get(ctx.store()?, &s(m, "id").unwrap_or_default())?;
                Ok(json!({ "connection": c.to_json() }))
            }
            Some(("list", m)) => {
                let flags = obj(vec![
                    ("q", sv(m, "q")),
                    ("host", sv(m, "host")),
                    ("tags", Some(json!(many(m, "tag")))),
                    ("group_id", sv(m, "group-id")),
                    ("no_group", Some(flag(m, "no-group").into())),
                    ("recursive", Some(flag(m, "recursive").into())),
                    ("status", sv(m, "status")),
                    ("sort", sv(m, "sort")),
                    ("desc", Some(flag(m, "desc").into())),
                    ("limit", num::<u32>(m, "limit").map(Value::from)),
                    ("offset", num::<u32>(m, "offset").map(Value::from)),
                ]);
                let f: ListFilter = parse(ctx.params(flags)?)?;
                let (items, total) = conn::list(ctx.store()?, &f)?;
                Ok(json!({ "items": items.iter().map(|c| c.to_list_json()).collect::<Vec<_>>(), "total": total }))
            }
            Some(("update", m)) => {
                let id = s(m, "id").unwrap_or_default();
                let given = m.contains_id("tag") && m.value_source("tag").is_some();
                let p = ctx.params(field_flags(m, given))?;
                let patch = p.as_object().cloned().unwrap_or_default();
                let c = conn::update(ctx.store()?, &id, &patch)?;
                Ok(json!({ "connection": c.to_json() }))
            }
            Some(("delete", m)) => conn::delete(ctx.store()?, &many(m, "id")),
            Some(("duplicate", m)) => {
                let id = s(m, "id").unwrap_or_default();
                let p = ctx.params(obj(vec![("name", sv(m, "name"))]))?;
                let name = super::p_str(&p, "name")?;
                let c = conn::duplicate(ctx.store()?, &id, name.as_deref())?;
                Ok(json!({ "connection": c.to_json() }))
            }
            Some(("check", m)) => super::check::run(ctx, m),
            _ => Err(CoreError::new("request.unknown_command", "Unknown conn command").with("command", "conn")),
        }
    }
}
