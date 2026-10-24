//! `conn check` — параллельная проверка подключений (CORE_API §3.6).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::{json, Map, Value};
use tokio::sync::{mpsc, Semaphore};

use super::{arg, flag, many, num, p_bool, p_str, p_strings, repeated, runtime, sv, switch, Ctx};
use crate::error::{CoreError, CoreResult};
use crate::repo::conn::{self, Conn, ListFilter, Status};
use crate::repo::hostkey;
use crate::ssh::target::OneTimeSecrets;
use crate::ssh::{connect, ConnectOptions, Target};
use crate::util::{now, obj};

pub const MAX_BATCH: usize = 100;

pub fn command() -> Command {
    Command::new("check")
        .about("Check connections: TCP, SSH handshake, host key, authentication")
        .arg(Arg::new("id").action(ArgAction::Append))
        .arg(switch("all"))
        .arg(arg("group-id"))
        .arg(switch("recursive"))
        .arg(repeated("tag"))
        .arg(arg("timeout").value_parser(clap::value_parser!(u64).range(1..=600)))
        .arg(arg("concurrency").value_parser(clap::value_parser!(u64).range(1..=100)))
        .arg(switch("accept-new").help("Trust unknown (never changed) host keys"))
        .arg(switch("stream").help("Print each result as a JSON line, then the final envelope"))
}

/// Состояние по коду ошибки (таблица §3.6).
pub fn state_for(err: Option<&CoreError>) -> &'static str {
    match err {
        None => "online",
        Some(e) if e.code.starts_with("net.") => "offline",
        Some(e) if e.code.starts_with("auth.") => "auth_failed",
        Some(_) => "error",
    }
}

struct Job {
    idx: usize,
    conn: Conn,
    target: Result<Target, CoreError>,
    known: Vec<hostkey::HostKey>,
}

struct JobResult {
    idx: usize,
    id: String,
    host: String,
    port: u16,
    error: Option<CoreError>,
    latency_ms: Option<i64>,
    banner: Option<String>,
    accept: Option<hostkey::Presented>,
}

fn select(ctx: &mut Ctx, p: &Value) -> CoreResult<Vec<Conn>> {
    let ids = p_strings(p, "ids")?;
    let tags = p_strings(p, "tags")?;
    let group_id = p_str(p, "group_id")?;
    let store = ctx.store()?;
    if !ids.is_empty() {
        if ids.len() > MAX_BATCH {
            return Err(too_many(ids.len()));
        }
        let mut seen = Vec::new();
        return ids
            .iter()
            .filter(|id| {
                let dup = seen.contains(id);
                seen.push(*id);
                !dup
            })
            .map(|id| conn::get(store, id))
            .collect();
    }
    if !p_bool(p, "all") && group_id.is_none() && tags.is_empty() {
        return Err(CoreError::invalid_args(
            "id",
            "specify connection ids, --all, --group-id or --tag",
        ));
    }
    let f = ListFilter {
        tags,
        group_id,
        recursive: p_bool(p, "recursive"),
        ..Default::default()
    };
    Ok(conn::list(store, &f)?.0)
}

fn too_many(given: usize) -> CoreError {
    CoreError::new("request.too_many", format!("At most {MAX_BATCH} connections per check"))
        .with("max", MAX_BATCH)
        .with("given", given)
}

pub fn run(ctx: &mut Ctx, m: &ArgMatches) -> CoreResult<Value> {
    let ids = many(m, "id");
    let tags = many(m, "tag");
    let flags = obj(vec![
        ("ids", if ids.is_empty() { None } else { Some(json!(ids)) }),
        ("all", Some(flag(m, "all").into())),
        ("group_id", sv(m, "group-id")),
        ("recursive", Some(flag(m, "recursive").into())),
        ("tags", if tags.is_empty() { None } else { Some(json!(tags)) }),
        ("timeout", num::<u64>(m, "timeout").map(Value::from)),
        ("concurrency", num::<u64>(m, "concurrency").map(Value::from)),
        ("accept_new", Some(flag(m, "accept-new").into())),
        ("stream", Some(flag(m, "stream").into())),
    ]);
    let p = ctx.params(flags)?;
    let settings = ctx.settings()?.clone();
    let timeout = Duration::from_secs(
        p.get("timeout")
            .and_then(Value::as_u64)
            .unwrap_or(settings.connect_timeout_sec)
            .clamp(1, 600),
    );
    let concurrency = p
        .get("concurrency")
        .and_then(Value::as_u64)
        .map(|n| n as usize)
        .unwrap_or(settings.check_concurrency)
        .clamp(1, MAX_BATCH);
    let accept_new = p_bool(&p, "accept_new");
    let stream = p_bool(&p, "stream");
    let secrets: HashMap<String, OneTimeSecrets> = match p.get("secrets") {
        None | Some(Value::Null) => HashMap::new(),
        Some(v) => serde_json::from_value(v.clone())
            .map_err(|_| CoreError::invalid("secrets", "expected {id: {password, passphrase}}"))?,
    };

    let conns = select(ctx, &p)?;
    if conns.len() > MAX_BATCH {
        return Err(too_many(conns.len()));
    }
    let store = ctx.store()?;
    let mut jobs = Vec::with_capacity(conns.len());
    for (idx, c) in conns.into_iter().enumerate() {
        let target = Target::from_conn(store, &c, secrets.get(&c.id));
        let known = hostkey::for_addr(store, &c.host, c.port)?;
        jobs.push(Job {
            idx,
            conn: c,
            target,
            known,
        });
    }
    let total = jobs.len();

    let rt = runtime()?;
    let (tx, mut rx) = mpsc::unbounded_channel::<JobResult>();
    let sem = Arc::new(Semaphore::new(concurrency));
    for job in jobs {
        let tx = tx.clone();
        let sem = sem.clone();
        rt.spawn(async move {
            let _permit = sem.acquire_owned().await;
            let r = check_one(job, timeout, accept_new).await;
            let _ = tx.send(r);
        });
    }
    drop(tx);

    let mut items: Vec<(usize, Value)> = Vec::with_capacity(total);
    let mut summary: Map<String, Value> = Map::new();
    for k in ["online", "offline", "auth_failed", "error"] {
        summary.insert(k.into(), 0.into());
    }
    while let Some(r) = rt.block_on(rx.recv()) {
        let store = ctx.store()?;
        if let Some(p) = &r.accept {
            if let Err(e) = hostkey::trust(store, &r.host, r.port, p, false) {
                tracing::warn!("cannot save accepted host key: {e}");
            }
        }
        let state = state_for(r.error.as_ref());
        let err_json = r.error.as_ref().map(|e| serde_json::to_value(e).unwrap_or(Value::Null));
        let status = Status {
            state: state.into(),
            checked_at: Some(now()),
            latency_ms: if state == "offline" { None } else { r.latency_ms },
            error: err_json.clone(),
        };
        conn::save_status(store, &r.id, &status)?;
        *summary.get_mut(state).expect("state") = (summary[state].as_u64().unwrap_or(0) + 1).into();
        let item = json!({
            "id": r.id,
            "state": state,
            "checked_at": status.checked_at,
            "latency_ms": status.latency_ms,
            "server_banner": r.banner,
            "error": err_json,
        });
        if stream {
            ctx.emit_line(&json!({ "event": "result", "item": item }))?;
        }
        items.push((r.idx, item));
    }
    summary.insert("total".into(), total.into());
    if stream {
        return Ok(json!({ "summary": summary }));
    }
    items.sort_by_key(|(i, _)| *i);
    Ok(json!({ "items": items.into_iter().map(|(_, v)| v).collect::<Vec<_>>(), "summary": summary }))
}

async fn check_one(job: Job, timeout: Duration, accept_new: bool) -> JobResult {
    let mut res = JobResult {
        idx: job.idx,
        id: job.conn.id.clone(),
        host: job.conn.host.clone(),
        port: job.conn.port,
        error: None,
        latency_ms: None,
        banner: None,
        accept: None,
    };
    let target = match job.target {
        Ok(t) => t,
        Err(e) => {
            res.error = Some(e);
            return res;
        }
    };
    let mut opts = ConnectOptions::new(timeout, job.known);
    opts.accept_new = accept_new;
    let out = connect(&target, &opts).await;
    res.latency_ms = out.latency_ms;
    res.banner = out.server_banner;
    if out.accepted_new {
        res.accept = out.presented;
    }
    match out.result {
        Ok(session) => session.close().await,
        Err(e) => res.error = Some(e),
    }
    res
}
