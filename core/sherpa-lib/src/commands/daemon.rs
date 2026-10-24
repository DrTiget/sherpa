//! `daemon` — запуск и управление демоном (CORE_API §4.1).

use clap::{ArgMatches, Command};
use serde_json::Value;

use super::{flag, switch, CommandGroup, Ctx};
use crate::daemon;
use crate::error::CoreResult;

pub struct Daemon;

impl CommandGroup for Daemon {
    fn name(&self) -> &'static str {
        "daemon"
    }

    fn command(&self) -> Command {
        Command::new("daemon")
            .about("Background daemon for terminal sessions and desktop utilities")
            .subcommand(Command::new("run").about("Run the daemon in foreground (used by `ensure`)"))
            .subcommand(Command::new("ensure").about("Start the daemon if it is not running"))
            .subcommand(Command::new("status").about("Daemon status"))
            .subcommand(Command::new("stop").about("Stop the daemon").arg(switch("force")))
    }

    fn run(&self, ctx: &mut Ctx, m: &ArgMatches) -> CoreResult<Value> {
        let settings = ctx.settings()?.clone();
        match m.subcommand() {
            None | Some(("run", _)) => daemon::run(&settings),
            Some(("ensure", _)) => daemon::ensure(&settings),
            Some(("status", _)) => daemon::status(&settings),
            Some(("stop", m)) => daemon::stop(&settings, flag(m, "force")),
            Some((other, _)) => Err(
                crate::error::CoreError::new("request.unknown_command", "Unknown daemon command")
                    .with("command", other),
            ),
        }
    }
}
