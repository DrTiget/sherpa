//! Утилита «Монитор ресурсов» (DESKTOP.md §4.3) — эталон для новых утилит.
//!
//! - `collect.rs`-логика здесь: команда для сервера + разбор (`parse.rs`);
//! - работает через `RemoteExec`, поэтому тестируется на `FakeExec`;
//! - методы демона: `monitor.snapshot`, `monitor.subscribe`, `monitor.unsubscribe`.

pub mod parse;

use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};

use super::command_failed;
use crate::daemon::proto::{event, opt_str, opt_u64, req_str};
use crate::daemon::registry::{unknown, Cx, MethodGroup};
use crate::daemon::state::Subscription;
use crate::error::{CoreError, CoreResult};
use crate::ssh::RemoteExec;
use parse::CpuTimes;

pub const SEP: &str = "@@SHERPA@@";
pub const DEFAULT_PROCESSES: u64 = 50;
pub const MAX_PROCESSES: u64 = 500;
pub const MIN_INTERVAL_MS: u64 = 1000;
/// Пауза между двумя замерами CPU, если предыдущего замера нет.
pub const FIRST_SAMPLE_MS: u64 = 500;

#[derive(Debug, Clone)]
pub struct Options {
    pub processes: u64,
    /// `cpu` или `mem`.
    pub sort: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            processes: DEFAULT_PROCESSES,
            sort: "cpu".into(),
        }
    }
}

impl Options {
    pub fn from_params(p: &Value) -> CoreResult<Options> {
        let processes = opt_u64(p, "processes")?.unwrap_or(DEFAULT_PROCESSES).min(MAX_PROCESSES);
        let sort = opt_str(p, "sort")?.unwrap_or_else(|| "cpu".into());
        if sort != "cpu" && sort != "mem" {
            return Err(CoreError::invalid("sort", "cpu | mem"));
        }
        Ok(Options { processes, sort })
    }
}

/// Скрипт для сервера. С `sleep_ms` — два замера /proc/stat (первый запуск).
pub fn script(opts: &Options, sleep_ms: Option<u64>) -> String {
    let sort = if opts.sort == "mem" { "-rss" } else { "-pcpu" };
    let first = match sleep_ms {
        Some(ms) => format!(
            "grep '^cpu' /proc/stat; echo {SEP}; sleep {}.{:03}; ",
            ms / 1000,
            ms % 1000
        ),
        None => String::new(),
    };
    format!(
        "LC_ALL=C; export LC_ALL; [ -r /proc/stat ] || {{ echo NOPROC; exit 3; }}; {first}\
         grep '^cpu' /proc/stat; echo {SEP}; cat /proc/meminfo; echo {SEP}; cat /proc/loadavg; echo {SEP}; \
         cat /proc/uptime; echo {SEP}; \
         ps -eo pid=,ppid=,user=,pcpu=,pmem=,rss=,etimes=,stat=,comm= --sort={sort} 2>/dev/null | head -n {n}; true",
        n = opts.processes
    )
}

/// Сборщик: помнит прошлый замер CPU, чтобы считать загрузку без паузы.
#[derive(Default)]
pub struct Collector {
    prev: Option<Vec<CpuTimes>>,
}

impl Collector {
    pub async fn sample<E: RemoteExec>(&mut self, exec: &E, opts: &Options) -> CoreResult<Value> {
        let sleep = if self.prev.is_none() {
            Some(FIRST_SAMPLE_MS)
        } else {
            None
        };
        let cmd = script(opts, sleep);
        let out = exec.exec(&cmd).await?;
        let text = out.stdout_lossy();
        if out.exit_status == Some(3) && text.contains("NOPROC") {
            return Err(CoreError::new("cap.missing", "Server has no /proc (not Linux?)").with("capability", "procfs"));
        }
        if !out.success() {
            return Err(command_failed("monitor", &out));
        }
        let mut parts: Vec<&str> = text.split(&format!("{SEP}\n")).collect();
        let first = if sleep.is_some() {
            Some(cpu_section(&mut parts)?)
        } else {
            None
        };
        if parts.len() < 5 {
            return Err(CoreError::new("remote.bad_output", "Unexpected monitor output").with("command", "monitor"));
        }
        let cur = parse::cpu_times(parts[0]);
        let prev = first.or_else(|| self.prev.clone()).unwrap_or_default();
        let (mem, swap) = parse::meminfo(parts[1]);
        let per_core: Vec<Option<f64>> = (1..cur.len())
            .map(|i| prev.get(i).and_then(|p| parse::usage_pct(*p, cur[i])))
            .collect();
        let usage = match (prev.first(), cur.first()) {
            (Some(p), Some(c)) => parse::usage_pct(*p, *c),
            _ => None,
        };
        let data = json!({
            "timestamp": crate::util::now(),
            "cpu": { "usage_pct": usage, "cores": per_core.len(), "per_core": per_core },
            "memory": mem,
            "swap": swap,
            "load": parse::loadavg(parts[2]),
            "uptime_sec": parse::uptime(parts[3]),
            "processes": parse::processes(parts[4]),
        });
        self.prev = Some(cur);
        Ok(data)
    }
}

fn cpu_section(parts: &mut Vec<&str>) -> CoreResult<Vec<CpuTimes>> {
    if parts.is_empty() {
        return Err(CoreError::new("remote.bad_output", "Unexpected monitor output").with("command", "monitor"));
    }
    Ok(parse::cpu_times(parts.remove(0)))
}

pub struct MonitorMethods;

#[async_trait]
impl MethodGroup for MonitorMethods {
    fn prefix(&self) -> &'static str {
        "monitor"
    }

    fn methods(&self) -> &'static [&'static str] {
        &["monitor.snapshot", "monitor.subscribe", "monitor.unsubscribe"]
    }

    async fn call(&self, cx: &Cx, method: &str, p: Value) -> CoreResult<Value> {
        match method {
            "monitor.snapshot" => {
                let opts = Options::from_params(&p)?;
                let conn = cx.conn(&req_str(&p, "connection_id")?).await?;
                Collector::default().sample(conn.as_ref(), &opts).await
            }
            "monitor.subscribe" => {
                let opts = Options::from_params(&p)?;
                let connection_id = req_str(&p, "connection_id")?;
                let interval = opt_u64(&p, "interval_ms")?.unwrap_or(2000).max(MIN_INTERVAL_MS);
                let conn = cx.conn(&connection_id).await?;
                let id = cx.state.new_id("m");
                let (client, sub) = (cx.client.clone(), id.clone());
                let task = tokio::spawn(async move {
                    let mut c = Collector::default();
                    loop {
                        let started = tokio::time::Instant::now();
                        let ev = match c.sample(conn.as_ref(), &opts).await {
                            Ok(data) => event(
                                "monitor",
                                json!({ "subscription": sub, "connection_id": connection_id, "data": data }),
                            ),
                            Err(e) => {
                                let ev = event("error", json!({ "subscription": sub, "error": e, "fatal": true }));
                                client.send(&ev).await;
                                break;
                            }
                        };
                        if !client.send(&ev).await {
                            break;
                        }
                        tokio::time::sleep_until(started + Duration::from_millis(interval)).await;
                    }
                });
                cx.state.subscriptions.lock().expect("subs").insert(
                    id.clone(),
                    Subscription {
                        client: cx.client.id,
                        kind: "monitor",
                        task: task.abort_handle(),
                    },
                );
                Ok(json!({ "subscription": id, "interval_ms": interval }))
            }
            "monitor.unsubscribe" => {
                let id = req_str(&p, "subscription")?;
                let mut subs = cx.state.subscriptions.lock().expect("subs");
                match subs.get(&id) {
                    Some(s) if s.client == cx.client.id && s.kind == "monitor" => {
                        s.task.abort();
                        subs.remove(&id);
                        Ok(json!({ "unsubscribed": id }))
                    }
                    _ => {
                        Err(CoreError::new("subscription.not_found", "Subscription not found").with("subscription", id))
                    }
                }
            }
            _ => Err(unknown(method)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse::fixtures::*;
    use super::*;
    use crate::ssh::FakeExec;

    fn output(stat_first: Option<&str>, stat: &str) -> String {
        let mut s = String::new();
        if let Some(f) = stat_first {
            s += &format!("{f}{SEP}\n");
        }
        s += &format!("{stat}{SEP}\n{MEMINFO}{SEP}\n{LOADAVG}{SEP}\n{UPTIME}{SEP}\n{PS}");
        s
    }

    #[tokio::test]
    async fn first_sample_uses_two_reads() {
        let fake = FakeExec::new().on("/proc/stat", &output(Some(STAT1), STAT2), 0);
        let mut c = Collector::default();
        let d = c.sample(&fake, &Options::default()).await.unwrap();
        assert!(fake.calls()[0].contains("sleep 0.500"));
        assert!(fake.calls()[0].contains("--sort=-pcpu"));
        assert_eq!(d["cpu"]["usage_pct"], 50.0);
        assert_eq!(d["cpu"]["cores"], 2);
        assert_eq!(d["cpu"]["per_core"], json!([75.0, 25.0]));
        assert_eq!(d["memory"]["usage_pct"], 25.0);
        assert_eq!(d["swap"]["total_bytes"], 2_000_000u64 * 1024);
        assert_eq!(d["load"]["1m"], 0.52);
        assert_eq!(d["uptime_sec"], 86461);
        assert_eq!(d["processes"][1]["command"], "php-fpm: pool www");
    }

    #[tokio::test]
    async fn next_sample_uses_previous() {
        let fake = FakeExec::new().on("/proc/stat", &output(Some(STAT1), STAT1), 0);
        let mut c = Collector::default();
        c.sample(&fake, &Options::default()).await.unwrap();
        let fake2 = FakeExec::new().on("/proc/stat", &output(None, STAT2), 0);
        let opts = Options {
            processes: 10,
            sort: "mem".into(),
        };
        let d = c.sample(&fake2, &opts).await.unwrap();
        let cmd = &fake2.calls()[0];
        assert!(!cmd.contains("sleep"));
        assert!(cmd.contains("--sort=-rss") && cmd.contains("head -n 10"));
        assert_eq!(d["cpu"]["usage_pct"], 50.0);
    }

    #[tokio::test]
    async fn errors() {
        let fake = FakeExec::new().on("/proc/stat", "NOPROC\n", 3);
        let e = Collector::default()
            .sample(&fake, &Options::default())
            .await
            .unwrap_err();
        assert_eq!(e.code, "cap.missing");
        let fake = FakeExec::new().on("/proc/stat", "", 1);
        assert_eq!(
            Collector::default()
                .sample(&fake, &Options::default())
                .await
                .unwrap_err()
                .code,
            "remote.command_failed"
        );
        let fake = FakeExec::new().on("/proc/stat", "junk", 0);
        assert_eq!(
            Collector::default()
                .sample(&fake, &Options::default())
                .await
                .unwrap_err()
                .code,
            "remote.bad_output"
        );
        assert!(Options::from_params(&json!({"sort": "disk"})).is_err());
        assert_eq!(
            Options::from_params(&json!({"processes": 9999})).unwrap().processes,
            MAX_PROCESSES
        );
    }
}
