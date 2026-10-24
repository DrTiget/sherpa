//! `version` — версия, протокол, пути (CORE_API §1.6).

use clap::{ArgMatches, Command};
use serde_json::{json, Value};

use super::{arg, num, CommandGroup, Ctx};
use crate::error::{CoreError, CoreResult};
use crate::protocol::{PROTOCOL, PROTOCOL_MIN};
use crate::store::migrations;

pub struct Version;

impl CommandGroup for Version {
    fn name(&self) -> &'static str {
        "version"
    }

    fn command(&self) -> Command {
        Command::new("version")
            .about("Core version, protocol and paths")
            .arg(arg("require-protocol").value_parser(clap::value_parser!(u32)))
    }

    fn run(&self, ctx: &mut Ctx, m: &ArgMatches) -> CoreResult<Value> {
        if let Some(req) = num::<u32>(m, "require-protocol") {
            if !(PROTOCOL_MIN..=PROTOCOL).contains(&req) {
                return Err(
                    CoreError::new("protocol.unsupported", format!("Protocol {req} is not supported"))
                        .with("required", req)
                        .with("min", PROTOCOL_MIN)
                        .with("max", PROTOCOL),
                );
            }
        }
        let s = ctx.settings()?.clone();
        let commit = env!("SHERPA_COMMIT");
        Ok(json!({
            "core_version": env!("CARGO_PKG_VERSION"),
            "protocol": PROTOCOL,
            "protocol_min": PROTOCOL_MIN,
            "schema_version": migrations::latest(),
            "build": {
                "target": env!("SHERPA_TARGET"),
                "commit": if commit.is_empty() { Value::Null } else { commit.into() },
            },
            "paths": {
                "config": s.config_path.display().to_string(),
                "db": s.db_path.display().to_string(),
                "key_file": s.key_file_path.display().to_string(),
            },
            "initialized": s.db_path.exists() && s.key_file_path.exists(),
        }))
    }
}
