//! Реестр команд CLI. Группа команд = модуль с `CommandGroup`; новая группа
//! (files, monitor, disks, net, system, logs, docker…) добавляется одной
//! строкой в `registry()` — остальное (конверт ответа, ошибки, коды выхода,
//! stdin) общее.

pub mod check;
pub mod conn;
pub mod group;
pub mod hostkey;
pub mod import;
pub mod init;
pub mod keyfile;
pub mod status;
pub mod tag;
pub mod version;

use std::io::{Read, Write};

use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::Value;

use crate::config::{Overrides, Settings};
use crate::error::{CoreError, CoreResult};
use crate::protocol::{self, Warning};
use crate::store::Store;

/// Группа команд (`conn`, `group`…) или одиночная команда (`version`).
pub trait CommandGroup: Sync {
    fn name(&self) -> &'static str;
    /// Описание подкоманды для clap (имя должно совпадать с `name`).
    fn command(&self) -> Command;
    fn run(&self, ctx: &mut Ctx, m: &ArgMatches) -> CoreResult<Value>;
}

pub fn registry() -> Vec<&'static dyn CommandGroup> {
    vec![
        &version::Version,
        &init::Init,
        &keyfile::KeyFile,
        &conn::Conn,
        &group::Group,
        &tag::Tag,
        &status::Status,
        &hostkey::HostKey,
        &import::Import,
    ]
}

/// Контекст вызова: настройки, stdin-параметры, хранилище, поток вывода.
pub struct Ctx<'a> {
    pub overrides: Overrides,
    pub input: Option<Value>,
    pub out: &'a mut dyn Write,
    pub warnings: Vec<Warning>,
    settings: Option<Settings>,
    store: Option<Store>,
}

impl<'a> Ctx<'a> {
    pub fn new(overrides: Overrides, input: Option<Value>, out: &'a mut dyn Write) -> Self {
        Self {
            overrides,
            input,
            out,
            warnings: vec![],
            settings: None,
            store: None,
        }
    }

    pub fn settings(&mut self) -> CoreResult<&Settings> {
        if self.settings.is_none() {
            self.settings = Some(Settings::load(&self.overrides)?);
        }
        Ok(self.settings.as_ref().expect("settings"))
    }

    /// Открытое хранилище (ошибка `store.not_initialized`, если его нет).
    pub fn store(&mut self) -> CoreResult<&mut Store> {
        if self.store.is_none() {
            let s = self.settings()?.clone();
            self.store = Some(Store::open(&s)?);
        }
        Ok(self.store.as_mut().expect("store"))
    }

    pub fn set_store(&mut self, store: Store) {
        self.store = Some(store);
    }

    /// Параметры из stdin, наложенные поверх флагов.
    pub fn params(&self, flags: Value) -> CoreResult<Value> {
        crate::util::merge(flags, self.input.clone())
    }

    /// Строка JSON Lines в stdout (для потоковых команд), сразу со сбросом.
    pub fn emit_line(&mut self, v: &Value) -> CoreResult<()> {
        writeln!(self.out, "{v}")
            .and_then(|_| self.out.flush())
            .map_err(|e| CoreError::internal(format!("stdout: {e}")))
    }

    fn take_warnings(&mut self) -> Vec<Warning> {
        let mut w = std::mem::take(&mut self.warnings);
        if let Some(s) = self.store.as_mut() {
            w.append(&mut s.warnings);
        }
        w
    }
}

/// Флаги, которых нет и не будет: секреты в аргументах запрещены (§2).
const SECRET_FLAGS: &[&str] = &["--password", "--passphrase", "--key-text", "--secret", "--secrets"];

fn root_command() -> Command {
    let mut cmd = Command::new("sherpa-core")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Sherpa core: SSH connections manager (JSON in stdout)")
        .subcommand_required(true)
        .arg(
            Arg::new("config")
                .long("config")
                .global(true)
                .env("SHERPA_CONFIG")
                .value_name("PATH"),
        )
        .arg(
            Arg::new("db")
                .long("db")
                .global(true)
                .env("SHERPA_DB")
                .value_name("PATH"),
        )
        .arg(
            Arg::new("key-file")
                .long("key-file")
                .global(true)
                .env("SHERPA_KEY_FILE")
                .value_name("PATH"),
        )
        .arg(
            Arg::new("input")
                .long("input")
                .global(true)
                .value_name("-")
                .help("Read command parameters as JSON object from stdin"),
        )
        .arg(
            Arg::new("pretty")
                .long("pretty")
                .global(true)
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("log-level")
                .long("log-level")
                .global(true)
                .env("SHERPA_LOG")
                .value_name("LEVEL"),
        );
    for g in registry() {
        cmd = cmd.subcommand(g.command());
    }
    cmd
}

/// Уровень логов из аргументов (до полного разбора) — для настройки логгера.
pub fn log_level_from_args(args: &[String]) -> Option<String> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--log-level" {
            return it.next().cloned();
        }
        if let Some(v) = a.strip_prefix("--log-level=") {
            return Some(v.to_string());
        }
    }
    std::env::var("SHERPA_LOG").ok()
}

/// Точка входа CLI: разбор, выполнение, ответ в `out`. Возвращает код выхода.
pub fn run_cli(args: Vec<String>, stdin: &mut dyn Read, out: &mut dyn Write) -> i32 {
    let mut pretty = args.iter().any(|a| a == "--pretty");
    let write = |out: &mut dyn Write, v: &Value, pretty: bool| {
        let s = if pretty {
            serde_json::to_string_pretty(v)
        } else {
            serde_json::to_string(v)
        }
        .unwrap_or_default();
        let _ = writeln!(out, "{s}");
        let _ = out.flush();
    };
    let fail = |out: &mut dyn Write, e: CoreError, w: &[Warning], pretty: bool| {
        write(out, &protocol::failure(&e, w), pretty);
        e.exit_code()
    };

    if let Some(a) = args.iter().skip(1).find(|a| {
        SECRET_FLAGS
            .iter()
            .any(|f| a.as_str() == *f || a.starts_with(&format!("{f}=")))
    }) {
        let arg = a.split('=').next().unwrap_or(a).to_string();
        let e = CoreError::new(
            "request.secret_in_args",
            "Secrets must be passed via stdin JSON (--input -)",
        )
        .with("arg", arg);
        return fail(out, e, &[], pretty);
    }

    let matches = match root_command().try_get_matches_from(&args) {
        Ok(m) => m,
        Err(e) => {
            use clap::error::ErrorKind as K;
            match e.kind() {
                K::DisplayHelp | K::DisplayVersion | K::DisplayHelpOnMissingArgumentOrSubcommand => {
                    let _ = write!(out, "{}", e.render());
                    return if e.kind() == K::DisplayHelpOnMissingArgumentOrSubcommand {
                        2
                    } else {
                        0
                    };
                }
                _ => {}
            }
            let code = match e.kind() {
                K::InvalidSubcommand => "request.unknown_command",
                _ => "request.invalid_args",
            };
            let text = e.render().to_string();
            let first = text
                .lines()
                .next()
                .unwrap_or("")
                .trim_start_matches("error: ")
                .to_string();
            let mut err = CoreError::new(code, first.clone());
            if code == "request.unknown_command" {
                let cmd = e
                    .get(clap::error::ContextKind::InvalidSubcommand)
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                err = err.with("command", cmd);
            } else {
                let arg = e
                    .get(clap::error::ContextKind::InvalidArg)
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                err = err.with("arg", arg).with("reason", first);
            }
            return fail(out, err, &[], pretty);
        }
    };
    pretty = matches.get_flag("pretty");

    let input = match matches.get_one::<String>("input").map(String::as_str) {
        None => None,
        Some("-") => {
            let mut buf = String::new();
            if let Err(e) = stdin.read_to_string(&mut buf) {
                return fail(
                    out,
                    CoreError::new("request.invalid_json", format!("Cannot read stdin: {e}")).with("offset", 0),
                    &[],
                    pretty,
                );
            }
            if buf.trim().is_empty() {
                None
            } else {
                match serde_json::from_str::<Value>(&buf) {
                    Ok(v @ Value::Object(_)) => Some(v),
                    Ok(_) => {
                        return fail(
                            out,
                            CoreError::new("request.invalid_json", "stdin must contain a JSON object")
                                .with("offset", 0),
                            &[],
                            pretty,
                        )
                    }
                    Err(e) => {
                        let offset = buf
                            .lines()
                            .take(e.line().saturating_sub(1))
                            .map(|l| l.len() + 1)
                            .sum::<usize>()
                            + e.column();
                        return fail(
                            out,
                            CoreError::new("request.invalid_json", format!("Invalid JSON: {e}")).with("offset", offset),
                            &[],
                            pretty,
                        );
                    }
                }
            }
        }
        Some(other) => {
            return fail(
                out,
                CoreError::invalid_args("--input", format!("only '-' (stdin) is supported, got {other}")),
                &[],
                pretty,
            )
        }
    };

    let overrides = Overrides {
        config: matches.get_one::<String>("config").map(Into::into),
        db: matches.get_one::<String>("db").map(Into::into),
        key_file: matches.get_one::<String>("key-file").map(Into::into),
    };
    let (name, sub) = matches.subcommand().expect("subcommand required");
    let group = registry().into_iter().find(|g| g.name() == name).expect("registered");
    let mut ctx = Ctx::new(overrides, input, out);
    let result = group.run(&mut ctx, sub);
    let warnings = ctx.take_warnings();
    let out = ctx.out;
    match result {
        Ok(data) => {
            write(out, &protocol::success(data, &warnings), pretty);
            0
        }
        Err(e) => fail(out, e, &warnings, pretty),
    }
}

// ---------- помощники разбора флагов ----------

pub(crate) fn s(m: &ArgMatches, id: &str) -> Option<String> {
    m.get_one::<String>(id).cloned()
}

pub(crate) fn sv(m: &ArgMatches, id: &str) -> Option<Value> {
    s(m, id).map(Value::from)
}

pub(crate) fn many(m: &ArgMatches, id: &str) -> Vec<String> {
    m.get_many::<String>(id)
        .map(|v| v.cloned().collect())
        .unwrap_or_default()
}

pub(crate) fn flag(m: &ArgMatches, id: &str) -> bool {
    m.get_flag(id)
}

pub(crate) fn num<T: Clone + Send + Sync + 'static>(m: &ArgMatches, id: &str) -> Option<T> {
    m.get_one::<T>(id).cloned()
}

pub(crate) fn arg(id: &'static str) -> Arg {
    Arg::new(id).long(id)
}

pub(crate) fn switch(id: &'static str) -> Arg {
    Arg::new(id).long(id).action(ArgAction::SetTrue)
}

pub(crate) fn repeated(id: &'static str) -> Arg {
    Arg::new(id).long(id).action(ArgAction::Append)
}

pub(crate) fn port_arg() -> Arg {
    arg("port").value_parser(clap::value_parser!(u16).range(1..))
}

/// Строковое поле из объекта параметров.
pub(crate) fn p_str(p: &Value, key: &str) -> CoreResult<Option<String>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(CoreError::invalid(key, "expected string")),
    }
}

pub(crate) fn p_req(p: &Value, key: &str) -> CoreResult<String> {
    p_str(p, key)?
        .filter(|s| !s.is_empty())
        .ok_or_else(|| CoreError::required(key))
}

pub(crate) fn p_bool(p: &Value, key: &str) -> bool {
    p.get(key).and_then(Value::as_bool).unwrap_or(false)
}

pub(crate) fn p_port(p: &Value, key: &str) -> CoreResult<Option<u16>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .filter(|n| (1..=65535).contains(n))
            .map(|n| Some(n as u16))
            .ok_or_else(|| {
                CoreError::new("validation.out_of_range", "Port is out of range")
                    .with("field", key)
                    .with("min", 1)
                    .with("max", 65535)
            }),
    }
}

pub(crate) fn p_strings(p: &Value, key: &str) -> CoreResult<Vec<String>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(vec![]),
        Some(v) => serde_json::from_value(v.clone()).map_err(|_| CoreError::invalid(key, "expected array of strings")),
    }
}

/// tokio-рантайм для сетевых команд CLI.
pub(crate) fn runtime() -> CoreResult<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .map_err(|e| CoreError::internal(format!("tokio runtime: {e}")))
}
