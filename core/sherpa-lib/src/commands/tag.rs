//! `tag …` — теги (CORE_API §3.5).

use clap::{Arg, ArgMatches, Command};
use serde_json::Value;

use super::{s, CommandGroup, Ctx};
use crate::error::{CoreError, CoreResult};
use crate::repo::tag;

pub struct Tag;

impl CommandGroup for Tag {
    fn name(&self) -> &'static str {
        "tag"
    }

    fn command(&self) -> Command {
        Command::new("tag")
            .about("Tags")
            .subcommand_required(true)
            .subcommand(Command::new("list"))
            .subcommand(
                Command::new("rename")
                    .arg(Arg::new("old").required(true))
                    .arg(Arg::new("new").required(true)),
            )
            .subcommand(Command::new("delete").arg(Arg::new("tag").required(true)))
    }

    fn run(&self, ctx: &mut Ctx, m: &ArgMatches) -> CoreResult<Value> {
        match m.subcommand() {
            Some(("list", _)) => tag::list(ctx.store()?),
            Some(("rename", m)) => tag::rename(
                ctx.store()?,
                &s(m, "old").unwrap_or_default(),
                &s(m, "new").unwrap_or_default(),
            ),
            Some(("delete", m)) => tag::delete(ctx.store()?, &s(m, "tag").unwrap_or_default()),
            _ => Err(CoreError::new("request.unknown_command", "Unknown tag command").with("command", "tag")),
        }
    }
}
