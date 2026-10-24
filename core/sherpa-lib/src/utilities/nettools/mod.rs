//! Server-side diagnostic jobs, polled as bounded event streams.
pub mod parse;
use super::command_failed;
use crate::daemon::proto::{event, opt_str, opt_u64, req_str};
use crate::daemon::registry::{unknown, Cx, MethodGroup};
use crate::daemon::state::Subscription;
use crate::error::{CoreError, CoreResult};
use crate::ssh::{shell_quote, RemoteExec};
use async_trait::async_trait;
use serde_json::{json, Value};

pub fn command(kind: &str, p: &Value) -> CoreResult<(String, String)> {
    let target = req_str(p, "target")?;
    if target.len() > 2048 || target.starts_with('-') || target.chars().any(char::is_control) {
        return Err(CoreError::invalid("target", "invalid diagnostic destination"));
    }
    let host = || -> CoreResult<()> {
        if target.len() > 253 || !target.chars().all(|c| c.is_ascii_alphanumeric() || ".-:_".contains(c)) {
            Err(CoreError::invalid("target", "expected hostname or IP address"))
        } else {
            Ok(())
        }
    };
    let t = shell_quote(&target);
    let (tool, cmd) = match kind {
        "nettools.ping" => {
            host()?;
            let n = opt_u64(p, "count")?.unwrap_or(4);
            if !(1..=30).contains(&n) {
                return Err(CoreError::invalid("count", "1..30"));
            }
            ("ping", format!("ping -n -c {n} -W 2 {t}"))
        }
        "nettools.trace" => {
            host()?;
            ("traceroute", format!("traceroute -n -m 20 -w 1 {t}"))
        }
        "nettools.dig" => {
            host()?;
            let ty = opt_str(p, "record_type")?.unwrap_or_else(|| "A".into());
            if !matches!(ty.as_str(), "A" | "AAAA" | "MX" | "TXT" | "NS") {
                return Err(CoreError::invalid("record_type", "A | AAAA | MX | TXT | NS"));
            }
            ("dig", format!("dig +time=3 +tries=1 {t} {ty}"))
        }
        "nettools.curl" => {
            if !(target.starts_with("http://") || target.starts_with("https://"))
                || target
                    .split("//")
                    .nth(1)
                    .is_none_or(|s| s.is_empty() || s.contains('@'))
            {
                return Err(CoreError::invalid("target", "expected HTTP(S) URL without credentials"));
            }
            (
                "curl",
                format!("curl --head --silent --show-error --max-time 30 --proto '=http,https' -- {t}"),
            )
        }
        "nettools.port" => {
            host()?;
            let port = opt_u64(p, "port")?
                .filter(|n| (1..=65535).contains(n))
                .ok_or_else(|| CoreError::invalid("port", "1..65535"))?;
            ("nc", format!("nc -z -v -w 5 {t} {port}"))
        }
        _ => return Err(unknown(kind)),
    };
    Ok((tool.into(), cmd))
}

pub fn launch_script(id: &str, tool: &str, cmd: &str) -> String {
    let dir = shell_quote(&format!("/tmp/sherpa-nettools-{id}"));
    let job=format!("d={dir}; ulimit -f 128 || exit 1; timeout 60 sh -c {} >\"$d/output\" 2>&1; r=$?; echo \"$r\" >\"$d/status.tmp\"; mv \"$d/status.tmp\" \"$d/status\"; sleep 120; rm -rf -- \"$d\"",shell_quote(cmd));
    format!("LC_ALL=C; export LC_ALL; umask 077; for t in {} timeout nohup sh head sleep mv rm; do command -v \"$t\" >/dev/null 2>&1 || exit 127; done; d={dir}; mkdir \"$d\" || exit 1; : >\"$d/output\"; nohup sh -c {} </dev/null >/dev/null 2>&1 &",shell_quote(tool),shell_quote(&job))
}

pub async fn start<E: RemoteExec>(exec: &E, id: &str, kind: &str, p: &Value) -> CoreResult<()> {
    let (tool, cmd) = command(kind, p)?;
    let out = exec.exec(&launch_script(id, &tool, &cmd)).await?;
    if out.exit_status == Some(127) {
        return Err(CoreError::new("cap.missing", "Diagnostic tool is unavailable").with("capability", tool));
    }
    if !out.success() {
        return Err(command_failed(kind, &out));
    }
    Ok(())
}

pub async fn poll<E: RemoteExec>(exec: &E, id: &str) -> CoreResult<String> {
    let dir = shell_quote(&format!("/tmp/sherpa-nettools-{id}"));
    // Status MUST be read before output so final output cannot lag the final status.
    let cmd=format!("LC_ALL=C; export LC_ALL; d={dir}; [ -d \"$d\" ] || exit 1; if [ -f \"$d/status\" ]; then cat \"$d/status\"; else echo running; fi; head -c {} \"$d/output\"",parse::MAX_OUTPUT);
    let out = exec.exec(&cmd).await?;
    if !out.success() {
        return Err(command_failed("nettools.poll", &out));
    }
    Ok(out.stdout_lossy())
}

pub struct NettoolsMethods;
#[async_trait]
impl MethodGroup for NettoolsMethods {
    fn prefix(&self) -> &'static str {
        "nettools"
    }
    fn methods(&self) -> &'static [&'static str] {
        &[
            "nettools.ping",
            "nettools.trace",
            "nettools.dig",
            "nettools.curl",
            "nettools.port",
            "nettools.cancel",
        ]
    }
    async fn call(&self, cx: &Cx, method: &str, p: Value) -> CoreResult<Value> {
        if method == "nettools.cancel" {
            let id = req_str(&p, "subscription")?;
            let mut subs = cx.state.subscriptions.lock().expect("subs");
            match subs.get(&id) {
                Some(s) if s.client == cx.client.id && s.kind == "nettools" => {
                    s.task.abort();
                    subs.remove(&id);
                    return Ok(json!({"unsubscribed":id}));
                }
                _ => {
                    return Err(
                        CoreError::new("subscription.not_found", "Subscription not found").with("subscription", id)
                    )
                }
            }
        }
        command(method, &p)?;
        let connection_id = req_str(&p, "connection_id")?;
        let conn = cx.conn(&connection_id).await?;
        let job = uuid::Uuid::now_v7().to_string();
        start(conn.as_ref(), &job, method, &p).await?;
        let id = cx.state.new_id("nt");
        let (client, sub, state) = (cx.client.clone(), id.clone(), cx.state.clone());
        let kind = method.to_string();
        let task = tokio::spawn(async move {
            let mut offset = 0;
            let result:CoreResult<()>=async {
                for _ in 0..90 {
                    let text=poll(conn.as_ref(),&job).await?;
                    let (code,output)=parse::frame(&text)?;
                    let truncated=output.len()>=parse::MAX_OUTPUT;
                    let end=if code.is_some()||truncated {output.len()} else {output.rfind('\n').map(|n|n+1).unwrap_or(0)};
                    if end<offset || !output.is_char_boundary(offset) {return Err(CoreError::new("remote.bad_output","Diagnostic output changed unexpectedly"));}
                    if end>offset {
                        let ev=event("nettools.output",json!({"subscription":sub,"connection_id":connection_id,"method":kind,"records":parse::records(&output[offset..end])}));
                        if !client.send(&ev).await {return Ok(());}
                        offset=end;
                    }
                    if code.is_some()||truncated {
                        client.send(&event("nettools.done",json!({"subscription":sub,"exit_status":code,"truncated":truncated}))).await;
                        return Ok(());
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
                Err(CoreError::new("nettools.timeout","Diagnostic job exceeded polling deadline"))
            }.await;
            if let Err(e) = result {
                client
                    .send(&event("error", json!({"subscription":sub,"error":e,"fatal":true})))
                    .await;
            }
            state.subscriptions.lock().expect("subs").remove(&sub);
        });
        cx.state.subscriptions.lock().expect("subs").insert(
            id.clone(),
            Subscription {
                client: cx.client.id,
                kind: "nettools",
                task: task.abort_handle(),
            },
        );
        Ok(json!({"subscription":id}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssh::FakeExec;
    #[test]
    fn commands() {
        for k in ["ping", "trace", "dig", "port"] {
            assert!(command(&format!("nettools.{k}"), &json!({"target":"example.org","port":443})).is_ok());
        }
        assert!(command("nettools.curl", &json!({"target":"https://example.org/"}))
            .unwrap()
            .1
            .contains("--head"));
    }
    #[test]
    fn injection() {
        for target in ["-x", "host;id", "$(id)", "a\nb"] {
            assert!(command("nettools.ping", &json!({"target":target})).is_err());
        }
        assert!(command("nettools.curl", &json!({"target":"file:///etc/passwd"})).is_err());
        assert!(command("nettools.dig", &json!({"target":"a","record_type":"ANY"})).is_err());
    }
    #[tokio::test]
    async fn fake_start_and_poll() {
        let f = FakeExec::new().on("nohup", "", 0).on("head -c", "0\nHTTP/2 200\n", 0);
        start(&f, "test", "nettools.curl", &json!({"target":"https://example.org/"}))
            .await
            .unwrap();
        let out = poll(&f, "test").await.unwrap();
        assert_eq!(parse::frame(&out).unwrap().0, Some(0));
        assert!(f.calls().iter().all(|s| s.starts_with("LC_ALL=C")));
    }
    #[tokio::test]
    async fn missing_tool() {
        assert_eq!(
            start(
                &FakeExec::new(),
                "test",
                "nettools.ping",
                &json!({"target":"example.org"})
            )
            .await
            .unwrap_err()
            .code,
            "cap.missing"
        );
    }
}
