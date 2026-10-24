//! `group …` — группы (CORE_API §3.4).

use clap::{Arg, ArgMatches, Command};
use serde_json::{json, Value};

use super::{arg, flag, p_req, p_str, s, sv, switch, CommandGroup, Ctx};
use crate::error::{CoreError, CoreResult};
use crate::repo::group;
use crate::util::obj;

pub struct Group;

impl CommandGroup for Group {
    fn name(&self) -> &'static str {
        "group"
    }

    fn command(&self) -> Command {
        Command::new("group")
            .about("Connection groups (nested folders)")
            .subcommand_required(true)
            .subcommand(Command::new("create").arg(arg("name")).arg(arg("parent-id")))
            .subcommand(Command::new("list"))
            .subcommand(
                Command::new("update")
                    .arg(Arg::new("id").required(true))
                    .arg(arg("name"))
                    .arg(arg("parent-id").conflicts_with("root"))
                    .arg(switch("root")),
            )
            .subcommand(
                Command::new("delete")
                    .arg(Arg::new("id").required(true))
                    .arg(arg("mode").value_parser(["move_to_parent", "delete_connections"])),
            )
    }

    fn run(&self, ctx: &mut Ctx, m: &ArgMatches) -> CoreResult<Value> {
        match m.subcommand() {
            Some(("create", m)) => {
                let p = ctx.params(obj(vec![("name", sv(m, "name")), ("parent_id", sv(m, "parent-id"))]))?;
                let name = p_req(&p, "name")?;
                let g = group::create(ctx.store()?, &name, p_str(&p, "parent_id")?.as_deref())?;
                Ok(json!({ "group": g }))
            }
            Some(("list", _)) => Ok(json!({ "items": group::all(ctx.store()?)? })),
            Some(("update", m)) => {
                let id = s(m, "id").unwrap_or_default();
                let mut flags = obj(vec![("name", sv(m, "name")), ("parent_id", sv(m, "parent-id"))]);
                if flag(m, "root") {
                    flags["parent_id"] = Value::Null;
                }
                let p = ctx.params(flags)?;
                let name = p_str(&p, "name")?;
                let parent_owned = p_str(&p, "parent_id")?;
                let parent = p.get("parent_id").map(|_| parent_owned.as_deref());
                let g = group::update(ctx.store()?, &id, name.as_deref(), parent)?;
                Ok(json!({ "group": g }))
            }
            Some(("delete", m)) => {
                let id = s(m, "id").unwrap_or_default();
                let p = ctx.params(obj(vec![("mode", sv(m, "mode"))]))?;
                let mode = p_str(&p, "mode")?.unwrap_or_else(|| "move_to_parent".into());
                group::delete(ctx.store()?, &id, &mode)
            }
            _ => Err(CoreError::new("request.unknown_command", "Unknown group command").with("command", "group")),
        }
    }
}
