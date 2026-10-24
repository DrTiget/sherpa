pub mod parse;
use crate::daemon::{
    proto::{event, opt_bool, opt_str, opt_u64, req_str},
    registry::{unknown, Cx, MethodGroup},
    state::Subscription,
};
use crate::error::{CoreError, CoreResult};
use crate::ssh::{shell_quote, ExecOutput, PooledConn, RemoteExec};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};

pub fn script(method: &str, p: &Value) -> CoreResult<String> {
    let body=match method {
        "log.sources"=>"command -v find >/dev/null || { echo MISSING:find; exit 127; }; if command -v journalctl >/dev/null; then echo JOURNAL; else echo FILES; fi; find /var/log -maxdepth 2 -type f -print0".to_string(),
        "log.file.tail"=>{
            let path=req_str(p,"path")?;
            if !path.starts_with("/var/log/") || path.split('/').any(|s|s=="..") || path.contains('\0') {return Err(CoreError::invalid("path","path inside /var/log required"));}
            let n=opt_u64(p,"lines")?.unwrap_or(100).clamp(1,5000);
            // readlink проверяет также симлинки каталогов и конечного файла.
            format!("for t in readlink tail; do command -v \"$t\" >/dev/null || {{ echo \"MISSING:$t\"; exit 127; }}; done; p=$(readlink -f -- {}) || exit; case \"$p\" in /var/log/*) ;; *) exit 2;; esac; [ -f \"$p\" ] || exit 2; tmp=$(mktemp) || exit; trap 'rm -f \"$tmp\"' EXIT HUP INT TERM; tail -n {n} -- \"$p\" >\"$tmp\" || exit; tail -c 524288 \"$tmp\"",shell_quote(&path))
        },
        "log.vacuum"=>{
            if !opt_bool(p,"confirm")? {return Err(CoreError::invalid("confirm","explicit confirmation required"));}
            let size=opt_u64(p,"max_bytes")?.filter(|n|*n>0).ok_or_else(||CoreError::invalid("max_bytes","positive byte limit required"))?;
            format!("command -v journalctl >/dev/null || {{ echo MISSING:journalctl; exit 127; }}; journalctl --vacuum-size={size}")
        },
        "log.query"|"log.follow"=>{
            let n=opt_u64(p,"lines")?.unwrap_or(100).clamp(1,1000);
            let mut cmd=format!("command -v journalctl >/dev/null || {{ echo MISSING:journalctl; exit 127; }}; journalctl --no-pager -o json --all -n {n}");
            for (key,flag) in [("unit","--unit"),("priority","--priority"),("since","--since"),("until","--until"),("grep","--grep"),("boot","--boot"),("cursor","--after-cursor")] {
                if let Some(value)=opt_str(p,key)? {if value.contains('\0'){return Err(CoreError::invalid(key,"NUL is forbidden"));} cmd+=&format!(" {flag}={}",shell_quote(&value));}
            }
            // +N берёт первые N после курсора, обычное -n пропустило бы начало пачки.
            if opt_str(p,"cursor")?.is_some() {cmd=cmd.replace(&format!(" -n {n}"),&format!(" -n +{n}"));}
            cmd
        },
        _=>return Err(unknown(method)),
    };
    Ok(format!("LC_ALL=C; export LC_ALL; {body}"))
}

pub async fn collect<E: RemoteExec>(exec: &E, method: &str, p: &Value) -> CoreResult<Value> {
    let out = exec.exec(&script(method, p)?).await?;
    let text = out.stdout_lossy();
    if out.exit_status == Some(127) && text.starts_with("MISSING:") {
        return Err(CoreError::new("cap.missing", "Required log tool missing")
            .with("capability", text.trim_start_matches("MISSING:").trim()));
    }
    if !out.success() {
        return Err(super::command_failed(method, &out));
    }
    match method {
        "log.sources" => {
            let (kind, files) = text
                .split_once('\n')
                .ok_or_else(|| CoreError::new("remote.bad_output", "Invalid log source output"))?;
            if kind != "JOURNAL" && kind != "FILES" {
                return Err(CoreError::new("remote.bad_output", "Invalid log source output"));
            }
            let mut d = parse::paths(files);
            d["journal"] = json!(kind == "JOURNAL");
            Ok(d)
        }
        "log.file.tail" => Ok(parse::tail(&text)),
        "log.vacuum" => Ok(json!({"vacuumed":true})),
        _ => {
            let mut data = parse::journal(&text)?;
            // С grep и обычным -n journald неявно включает reverse.
            if opt_str(p, "grep")?.is_some() && opt_str(p, "cursor")?.is_none() {
                if let Some(items) = data["items"].as_array_mut() {
                    items.reverse();
                }
                data["cursor"] = data["items"]
                    .as_array()
                    .and_then(|a| a.last())
                    .map(|v| v["cursor"].clone())
                    .unwrap_or(Value::Null);
            }
            Ok(data)
        }
    }
}
struct Reader {
    conn: Arc<PooledConn>,
    sudo: bool,
}
impl RemoteExec for Reader {
    async fn exec_input(&self, cmd: &str, _: Option<&[u8]>) -> CoreResult<ExecOutput> {
        if self.sudo {
            self.conn.exec_privileged(cmd).await
        } else {
            self.conn.exec(cmd).await
        }
    }
}
pub struct LogMethods;
#[async_trait]
impl MethodGroup for LogMethods {
    fn prefix(&self) -> &'static str {
        "log"
    }
    fn methods(&self) -> &'static [&'static str] {
        &[
            "log.sources",
            "log.query",
            "log.follow",
            "log.unfollow",
            "log.file.tail",
            "log.vacuum",
        ]
    }
    async fn call(&self, cx: &Cx, method: &str, mut p: Value) -> CoreResult<Value> {
        if method == "log.unfollow" {
            let id = req_str(&p, "subscription")?;
            let mut subs = cx.state.subscriptions.lock().expect("subs");
            if !subs
                .get(&id)
                .is_some_and(|s| s.client == cx.client.id && s.kind == "log")
            {
                return Err(CoreError::new("subscription.not_found", "Subscription not found").with("subscription", id));
            }
            if let Some(s) = subs.remove(&id) {
                s.task.abort();
            }
            return Ok(json!({"unsubscribed":id}));
        }
        script(method, &p)?;
        let conn = cx.conn(&req_str(&p, "connection_id")?).await?;
        let sudo = method == "log.vacuum" || opt_bool(&p, "sudo")?;
        if method == "log.vacuum" {
            conn.ensure_writable()?;
        }
        let reader = Reader { conn, sudo };
        if method != "log.follow" {
            return collect(&reader, method, &p).await;
        }
        let interval = opt_u64(&p, "interval_ms")?.unwrap_or(1000).clamp(1000, 60000);
        let id = cx.state.new_id("l");
        let (sub, client) = (id.clone(), cx.client.clone());
        let state = cx.state.clone();
        let (ready, start) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            if start.await.is_err() {
                return;
            }
            loop {
                match collect(&reader, "log.query", &p).await {
                    Ok(data) => {
                        if let Some(cursor) = data["cursor"].as_str() {
                            p["cursor"] = json!(cursor);
                        }
                        if let Some(items) = data["items"].as_array() {
                            for item in items {
                                if !client
                                    .send(&event("log.line", json!({"subscription":sub,"data":item})))
                                    .await
                                {
                                    return;
                                }
                            }
                        }
                    }
                    Err(error) => {
                        client
                            .send(&event("error", json!({"subscription":sub,"error":error,"fatal":true})))
                            .await;
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(interval)).await;
            }
            state.subscriptions.lock().expect("subs").remove(&sub);
        });
        cx.state.subscriptions.lock().expect("subs").insert(
            id.clone(),
            Subscription {
                client: cx.client.id,
                kind: "log",
                task: task.abort_handle(),
            },
        );
        let _ = ready.send(());
        Ok(json!({"subscription":id,"interval_ms":interval}))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssh::FakeExec;
    #[tokio::test]
    async fn filters_and_cursor() {
        let p = json!({"unit":"a'; touch /tmp/pwn","priority":"3","since":"-1h","cursor":"s=abc"});
        let f = FakeExec::new().on("journalctl", "{\"MESSAGE\":\"hello\",\"__CURSOR\":\"s=next\"}\n", 0);
        assert_eq!(collect(&f, "log.query", &p).await.unwrap()["cursor"], "s=next");
        let c = &f.calls()[0];
        assert!(c.contains("LC_ALL=C") && c.contains("--after-cursor='s=abc'") && c.contains(" -n +100"));
        assert!(c.contains("'\\''"));
    }
    #[tokio::test]
    async fn grep_order_and_bad_cursor() {
        let f = FakeExec::new().on(
            "journalctl",
            "{\"__CURSOR\":\"new\",\"MESSAGE\":\"b\"}\n{\"__CURSOR\":\"old\",\"MESSAGE\":\"a\"}\n",
            0,
        );
        let d = collect(&f, "log.query", &json!({"grep":"test"})).await.unwrap();
        assert_eq!(d["items"][0]["message"], "a");
        assert_eq!(d["cursor"], "new");
        assert_eq!(
            collect(&FakeExec::new().on("journalctl", "{}", 0), "log.query", &json!({}))
                .await
                .unwrap_err()
                .code,
            "remote.bad_output"
        );
    }
    #[tokio::test]
    async fn errors_sources_and_tail() {
        assert_eq!(
            collect(
                &FakeExec::new().on("find /var/log", "FILES\n/var/log/syslog\0", 0),
                "log.sources",
                &json!({})
            )
            .await
            .unwrap()["journal"],
            false
        );
        assert_eq!(
            collect(
                &FakeExec::new().on("LC_ALL", "hello\n", 0),
                "log.file.tail",
                &json!({"path":"/var/log/syslog"})
            )
            .await
            .unwrap()["lines"],
            json!(["hello"])
        );
        assert_eq!(
            collect(&FakeExec::new().on("LC_ALL", "", 1), "log.query", &json!({}))
                .await
                .unwrap_err()
                .code,
            "remote.command_failed"
        );
        assert_eq!(
            collect(
                &FakeExec::new().on("LC_ALL", "MISSING:journalctl\n", 127),
                "log.query",
                &json!({})
            )
            .await
            .unwrap_err()
            .code,
            "cap.missing"
        );
        assert!(script("log.file.tail", &json!({"path":"/var/log/../../etc/shadow"})).is_err());
        assert!(script("log.vacuum", &json!({"max_bytes":100})).is_err());
    }
}
