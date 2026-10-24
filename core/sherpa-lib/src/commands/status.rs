//! `status list` — сохранённые статусы без сети (CORE_API §3.7).

use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::{json, Value};

use super::{many, CommandGroup, Ctx};
use crate::error::{CoreError, CoreResult};
use crate::repo::conn;

pub struct Status;

impl CommandGroup for Status {
    fn name(&self) -> &'static str {
        "status"
    }

    fn command(&self) -> Command {
        Command::new("status")
            .about("Saved connection statuses")
            .subcommand_required(true)
            .subcommand(Command::new("list").arg(Arg::new("id").action(ArgAction::Append)))
    }

    fn run(&self, ctx: &mut Ctx, m: &ArgMatches) -> CoreResult<Value> {
        let Some(("list", m)) = m.subcommand() else {
            return Err(CoreError::new("request.unknown_command", "Unknown status command").with("command", "status"));
        };
        let ids = many(m, "id");
        let store = ctx.store()?;
        let items: Vec<conn::Conn> = if ids.is_empty() {
            conn::all(store)?
        } else {
            ids.iter().map(|id| conn::get(store, id)).collect::<CoreResult<_>>()?
        };
        let items: Vec<Value> = items
            .iter()
            .map(|c| {
                json!({
                    "id": c.id,
                    "state": c.status.state,
                    "checked_at": c.status.checked_at,
                    "latency_ms": c.status.latency_ms,
                    "error": c.status.error,
                })
            })
            .collect();
        Ok(json!({ "items": items }))
    }
}
