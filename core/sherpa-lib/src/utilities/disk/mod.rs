pub mod parse;
use crate::daemon::{
    proto::{event, opt_bool, opt_u64, req_str},
    registry::{unknown, Cx, MethodGroup},
    state::Subscription,
};
use crate::error::{CoreError, CoreResult};
use crate::ssh::{shell_quote, RemoteExec};
use async_trait::async_trait;
use serde_json::{json, Value};

pub fn script(method: &str, p: &Value) -> CoreResult<String> {
    let body = if method == "disk.list" {
        "for t in df lsblk findmnt; do command -v \"$t\" >/dev/null || { echo \"MISSING:$t\"; exit 127; }; done; df -PB1 || exit; echo @@SHERPA_DISK@@; lsblk --json --bytes -o NAME,KNAME,TYPE,SIZE,FSTYPE,MOUNTPOINT || exit; echo @@SHERPA_DISK@@; findmnt --json -o TARGET,SOURCE,FSTYPE,OPTIONS || exit; echo @@SHERPA_DISK@@; df -Pi".to_string()
    } else {
        let path = req_str(p, "path")?;
        if !path.starts_with('/') || path.contains('\0') {
            return Err(CoreError::invalid("path", "absolute path required"));
        }
        let seconds = opt_u64(p, "timeout_sec")?.unwrap_or(30).clamp(1, 120);
        let depth = opt_u64(p, "depth")?.unwrap_or(2).min(10);
        let one = if opt_bool(p, "one_filesystem")? { "-x" } else { "" };
        let cmd = if method == "disk.scan" {
            format!("du -k -0 --max-depth={depth} {one} -- {}", shell_quote(&path))
        } else {
            let min = opt_u64(p, "min_size")?.unwrap_or(100 * 1024 * 1024);
            let mut filters = String::new();
            if let Some(days) = opt_u64(p, "min_age_days")? {
                filters += &format!(" -mtime +{days}");
            }
            if let Some(days) = opt_u64(p, "max_age_days")? {
                filters += &format!(" -mtime -{days}");
            }
            for (key, flag) in [("owner", "-user"), ("extension", "-name")] {
                if let Some(value) = crate::daemon::proto::opt_str(p, key)? {
                    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b)) {
                        return Err(CoreError::invalid(key, "invalid filter"));
                    }
                    let value = if key == "extension" {
                        format!("*.{}", value.trim_start_matches('.'))
                    } else {
                        value
                    };
                    filters += &format!(" {flag} {}", shell_quote(&value));
                }
            }
            format!(
                "find {} {} -type f -size +{}c{filters} -printf '%s\\t%p\\0'",
                shell_quote(&path),
                if one.is_empty() { "" } else { "-xdev" },
                min.saturating_sub(1)
            )
        };
        format!("for t in timeout nice ionice {}; do command -v \"$t\" >/dev/null || {{ echo \"MISSING:$t\"; exit 127; }}; done; timeout -s TERM -k 2 {seconds}s nice -n 19 ionice -c3 {cmd}",if method=="disk.scan"{"du"}else{"find"})
    };
    Ok(format!("LC_ALL=C; export LC_ALL; {body}"))
}

pub async fn collect<E: RemoteExec>(exec: &E, method: &str, p: &Value) -> CoreResult<Value> {
    let out = exec.exec(&script(method, p)?).await?;
    let text = out.stdout_lossy();
    if out.exit_status == Some(127) {
        if let Some(tool) = text.strip_prefix("MISSING:") {
            return Err(CoreError::new("cap.missing", "Required disk tool missing").with("capability", tool.trim()));
        }
    }
    let partial = out.exit_status == Some(124);
    if !out.success() && !partial {
        return Err(super::command_failed(method, &out));
    }
    if method == "disk.list" {
        return parse::list(&text);
    }
    // timeout может прервать последний NUL-запись: не выдаём её как полную.
    let complete = if partial {
        text.rfind('\0').map(|i| &text[..=i]).unwrap_or("")
    } else {
        &text
    };
    let mut items = parse::sizes(complete, method == "disk.large")?;
    items.sort_by_key(|v| std::cmp::Reverse(v["size_bytes"].as_u64().unwrap_or(0)));
    let limit = opt_u64(p, "limit")?.unwrap_or(1000).clamp(1, 5000) as usize;
    let truncated = items.len() > limit;
    items.truncate(limit);
    Ok(json!({"items":items,"partial":partial,"truncated":truncated}))
}

pub struct DiskMethods;
#[async_trait]
impl MethodGroup for DiskMethods {
    fn prefix(&self) -> &'static str {
        "disk"
    }
    fn methods(&self) -> &'static [&'static str] {
        &["disk.list", "disk.scan", "disk.large", "disk.scan.cancel"]
    }
    async fn call(&self, cx: &Cx, method: &str, p: Value) -> CoreResult<Value> {
        if method == "disk.scan.cancel" {
            let id = req_str(&p, "subscription")?;
            let mut subs = cx.state.subscriptions.lock().expect("subs");
            if !subs
                .get(&id)
                .is_some_and(|s| s.client == cx.client.id && s.kind == "disk")
            {
                return Err(CoreError::new("subscription.not_found", "Subscription not found").with("subscription", id));
            }
            if let Some(s) = subs.remove(&id) {
                s.task.abort();
            }
            return Ok(json!({"unsubscribed":id}));
        }
        if !self.methods().contains(&method) {
            return Err(unknown(method));
        }
        script(method, &p)?;
        let connection_id = req_str(&p, "connection_id")?;
        let conn = cx.conn(&connection_id).await?;
        if method != "disk.scan" {
            return collect(conn.as_ref(), method, &p).await;
        }
        let id = cx.state.new_id("d");
        let (sub, client) = (id.clone(), cx.client.clone());
        let state = cx.state.clone();
        let (ready, start) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            if start.await.is_err() {
                return;
            }
            if !client
                .send(&event(
                    "disk.scan.progress",
                    json!({"subscription":sub,"connection_id":connection_id,"state":"started"}),
                ))
                .await
            {
                state.subscriptions.lock().expect("subs").remove(&sub);
                return;
            }
            let ev = match collect(conn.as_ref(), "disk.scan", &p).await {
                Ok(data) => event(
                    "disk.scan.done",
                    json!({"subscription":sub,"connection_id":connection_id,"data":data}),
                ),
                Err(error) => event("error", json!({"subscription":sub,"error":error,"fatal":true})),
            };
            client.send(&ev).await;
            state.subscriptions.lock().expect("subs").remove(&sub);
        });
        cx.state.subscriptions.lock().expect("subs").insert(
            id.clone(),
            Subscription {
                client: cx.client.id,
                kind: "disk",
                task: task.abort_handle(),
            },
        );
        let _ = ready.send(());
        Ok(json!({"subscription":id}))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssh::FakeExec;
    #[tokio::test]
    async fn scan_partial() {
        let f = FakeExec::new().on("du -k", "10\t/tmp/a\0broken", 124);
        let d = collect(
            &f,
            "disk.scan",
            &json!({"path":"/tmp/'a","depth":3,"one_filesystem":true}),
        )
        .await
        .unwrap();
        assert_eq!(d["partial"], true);
        assert_eq!(d["items"][0]["size_bytes"], 10240);
        let cmd = &f.calls()[0];
        assert!(cmd.contains("LC_ALL=C") && cmd.contains("ionice -c3") && cmd.contains("--max-depth=3 -x"));
    }
    #[tokio::test]
    async fn large_and_errors() {
        let p = json!({"path":"/tmp"});
        let f = FakeExec::new().on("find '/tmp'", "100\t/tmp/a\0", 0);
        assert_eq!(
            collect(&f, "disk.large", &p).await.unwrap()["items"][0]["size_bytes"],
            100
        );
        assert_eq!(
            collect(&FakeExec::new().on("LC_ALL", "", 1), "disk.large", &p)
                .await
                .unwrap_err()
                .code,
            "remote.command_failed"
        );
        assert_eq!(
            collect(&FakeExec::new().on("LC_ALL", "MISSING:find\n", 127), "disk.large", &p)
                .await
                .unwrap_err()
                .code,
            "cap.missing"
        );
        assert!(script("disk.scan", &json!({"path":"relative"})).is_err());
    }
}
