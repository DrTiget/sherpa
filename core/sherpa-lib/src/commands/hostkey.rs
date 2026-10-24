//! `hostkey …` — TOFU (CORE_API §3.8).

use std::time::Duration;

use clap::{ArgMatches, Command};
use serde_json::{json, Value};

use super::{arg, num, p_bool, p_port, p_req, p_str, port_arg, runtime, sv, switch, CommandGroup, Ctx};
use crate::error::{CoreError, CoreResult};
use crate::repo::hostkey::{self, Presented};
use crate::ssh::{connect, AuthMaterial, ConnectOptions, Target};
use crate::util::obj;

pub struct HostKey;

fn addr_args(cmd: Command, host_required: bool) -> Command {
    cmd.arg(arg("host").required(host_required)).arg(port_arg())
}

fn addr(p: &Value) -> CoreResult<(String, u16)> {
    Ok((p_req(p, "host")?, p_port(p, "port")?.unwrap_or(22)))
}

impl CommandGroup for HostKey {
    fn name(&self) -> &'static str {
        "hostkey"
    }

    fn command(&self) -> Command {
        Command::new("hostkey")
            .about("Known host keys (trust on first use)")
            .subcommand_required(true)
            .subcommand(addr_args(Command::new("list"), false))
            .subcommand(addr_args(Command::new("get"), false))
            .subcommand(
                addr_args(Command::new("accept"), false)
                    .arg(arg("key-type"))
                    .arg(arg("fingerprint"))
                    .arg(switch("replace")),
            )
            .subcommand(
                addr_args(Command::new("reject"), false)
                    .arg(arg("fingerprint"))
                    .arg(arg("key-type")),
            )
            .subcommand(addr_args(Command::new("forget"), false).arg(arg("key-type")))
    }

    fn run(&self, ctx: &mut Ctx, m: &ArgMatches) -> CoreResult<Value> {
        let (name, m) = m
            .subcommand()
            .ok_or_else(|| CoreError::new("request.unknown_command", "hostkey").with("command", "hostkey"))?;
        let flags = obj(vec![
            ("host", sv(m, "host")),
            ("port", num::<u16>(m, "port").map(Value::from)),
            (
                "key_type",
                m.try_get_one::<String>("key-type")
                    .ok()
                    .flatten()
                    .cloned()
                    .map(Value::from),
            ),
            (
                "fingerprint",
                m.try_get_one::<String>("fingerprint")
                    .ok()
                    .flatten()
                    .cloned()
                    .map(Value::from),
            ),
            (
                "replace",
                m.try_get_one::<bool>("replace")
                    .ok()
                    .flatten()
                    .copied()
                    .map(Value::from),
            ),
        ]);
        let p = ctx.params(flags)?;
        match name {
            "list" => {
                let host = p_str(&p, "host")?;
                let port = p_port(&p, "port")?;
                Ok(json!({ "items": hostkey::list(ctx.store()?, host.as_deref(), port)? }))
            }
            "get" => {
                let (host, port) = addr(&p)?;
                let items = hostkey::for_addr(ctx.store()?, &host, port)?;
                if items.is_empty() {
                    return Err(hostkey::not_found(&host, port));
                }
                Ok(json!({ "items": items }))
            }
            "accept" => {
                let (host, port) = addr(&p)?;
                let key_type = p_req(&p, "key_type")?;
                let fp = p_req(&p, "fingerprint")?;
                let timeout = Duration::from_secs(ctx.settings()?.connect_timeout_sec);
                let actual = probe(&host, port, &key_type, timeout)?;
                if actual.fingerprint != fp {
                    return Err(
                        CoreError::new("hostkey.fingerprint_changed", "Server presented a different key")
                            .with("expected", fp)
                            .with("actual", actual.fingerprint),
                    );
                }
                let hk = hostkey::trust(ctx.store()?, &host, port, &actual, p_bool(&p, "replace"))?;
                Ok(json!({ "host_key": hk }))
            }
            "reject" => {
                let (host, port) = addr(&p)?;
                let fp = p_req(&p, "fingerprint")?;
                let kt = p_str(&p, "key_type")?;
                Ok(json!({ "host_key": hostkey::reject(ctx.store()?, &host, port, &fp, kt.as_deref())? }))
            }
            "forget" => {
                let (host, port) = addr(&p)?;
                let kt = p_str(&p, "key_type")?;
                hostkey::forget(ctx.store()?, &host, port, kt.as_deref())
            }
            _ => Err(CoreError::new("request.unknown_command", "Unknown hostkey command").with("command", "hostkey")),
        }
    }
}

/// Заново получить ключ сервера указанного типа (без авторизации).
fn probe(host: &str, port: u16, key_type: &str, timeout: Duration) -> CoreResult<Presented> {
    let target = Target {
        host: host.into(),
        port,
        user: "sherpa".into(),
        auth: AuthMaterial::Password(None),
    };
    let mut opts = ConnectOptions::new(timeout, vec![]);
    opts.probe_only = true;
    opts.key_type = Some(key_type.to_string());
    if !russh::Preferred::default()
        .key
        .iter()
        .any(|a| crate::ssh::keys::key_type(a) == key_type)
    {
        return Err(CoreError::invalid_args(
            "--key-type",
            format!("unsupported key type {key_type}"),
        ));
    }
    let out = runtime()?.block_on(connect(&target, &opts));
    match (out.presented, out.result) {
        (Some(p), _) => Ok(p),
        (None, Err(e)) => Err(e),
        (None, Ok(_)) => Err(CoreError::internal("probe finished without host key")),
    }
}
