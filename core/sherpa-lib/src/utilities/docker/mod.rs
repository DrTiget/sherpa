use crate::daemon::proto::{opt_bool, opt_u64, req_str};
use crate::daemon::registry::{unknown, Cx, MethodGroup};
use crate::error::{CoreError, CoreResult};
use crate::ssh::{shell_quote, RemoteExec};
use async_trait::async_trait;
use serde_json::{json, Value};
pub mod parse;

pub fn command(method: &str, p: &Value) -> CoreResult<(String, bool)> {
    let mut write = false;
    let args = match method {
        "docker.ping" => "version --format json".into(),
        "docker.ps" => "ps -a --no-trunc --format json".into(),
        "docker.images" => "image ls --no-trunc --format json".into(),
        "docker.volumes" => "volume ls --format json".into(),
        "docker.networks" => "network ls --no-trunc --format json".into(),
        "docker.stats" => "stats --no-stream --no-trunc --format json".into(),
        // без --format docker печатает один многострочный JSON-массив, а разбор идёт построчно
        "docker.inspect" => format!("inspect --format '{{{{json .}}}}' -- {}", shell_quote(&req_str(p, "id")?)),
        "docker.logs" => format!(
            "logs --timestamps --tail {} -- {}",
            opt_u64(p, "tail")?.unwrap_or(200).min(5000),
            shell_quote(&req_str(p, "id")?)
        ),
        "docker.start" | "docker.stop" | "docker.restart" | "docker.pause" | "docker.rm" => {
            write = true;
            if !opt_bool(p, "confirm")? {
                return Err(CoreError::new(
                    "action.confirm_required",
                    "Explicit confirmation required",
                ));
            }
            format!(
                "{} -- {}",
                method.trim_start_matches("docker."),
                shell_quote(&req_str(p, "id")?)
            )
        }
        _ => return Err(unknown(method)),
    };
    Ok((
        format!("LC_ALL=C; export LC_ALL; command -v docker >/dev/null 2>&1 || exit 127; docker {args}"),
        write,
    ))
}
pub async fn collect<E: RemoteExec>(exec: &E, method: &str, p: &Value) -> CoreResult<Value> {
    let (cmd, _) = command(method, p)?;
    let out = exec.exec(&cmd).await?;
    decode(method, &out)
}
fn decode(method: &str, out: &crate::ssh::ExecOutput) -> CoreResult<Value> {
    if out.exit_status == Some(127) {
        return Err(CoreError::new("cap.missing", "Docker is unavailable").with("capability", "docker"));
    }
    if !out.success() {
        return Err(super::command_failed(method, out));
    }
    if method == "docker.logs" {
        let mut lines: Vec<String> = out.stdout_lossy().lines().map(str::to_owned).collect();
        lines.extend(out.stderr_lossy().lines().map(str::to_owned));
        return Ok(json!({"lines":lines}));
    }
    if matches!(
        method,
        "docker.start" | "docker.stop" | "docker.restart" | "docker.pause" | "docker.rm"
    ) {
        return Ok(json!({"completed":true}));
    }
    let mut data = if method == "docker.stats" {
        parse::stats(&out.stdout_lossy())?
    } else {
        parse::objects(&out.stdout_lossy())?
    };
    if method == "docker.inspect" {
        if let Some(items) = data["items"].as_array_mut() {
            for item in items {
                // Inspect may contain passwords in Env, labels and command arguments.
                *item = json!({"Id":item["Id"],"Name":item["Name"],"Image":item["Image"],"State":item["State"],"Mounts":item["Mounts"],"NetworkSettings":item["NetworkSettings"]});
            }
        }
    }
    Ok(data)
}
pub struct DockerMethods;
#[async_trait]
impl MethodGroup for DockerMethods {
    fn prefix(&self) -> &'static str {
        "docker"
    }
    fn methods(&self) -> &'static [&'static str] {
        &[
            "docker.ping",
            "docker.ps",
            "docker.images",
            "docker.volumes",
            "docker.networks",
            "docker.inspect",
            "docker.stats",
            "docker.logs",
            "docker.start",
            "docker.stop",
            "docker.restart",
            "docker.pause",
            "docker.rm",
            "docker.unsubscribe",
        ]
    }
    async fn call(&self, cx: &Cx, method: &str, p: Value) -> CoreResult<Value> {
        if method == "docker.unsubscribe" {
            let id = req_str(&p, "subscription")?;
            let mut subs = cx.state.subscriptions.lock().expect("subs");
            if let Some(s) = subs.get(&id) {
                if s.client == cx.client.id && s.kind == "docker" {
                    s.task.abort();
                    subs.remove(&id);
                    return Ok(json!({"unsubscribed":id}));
                }
            }
            return Err(CoreError::new("subscription.not_found", "Subscription not found").with("subscription", id));
        }
        let (cmd, write) = command(method, &p)?;
        let conn = cx.conn(&req_str(&p, "connection_id")?).await?;
        if write {
            conn.ensure_writable()?;
            cx.state.confirmations.consume(cx.client.id, method, &p)?;
        }
        if (method == "docker.logs" && opt_bool(&p, "follow")?)
            || (method == "docker.stats" && opt_bool(&p, "subscribe")?)
        {
            let id = cx.state.new_id("d");
            let sub = id.clone();
            let client = cx.client.clone();
            let method = method.to_owned();
            let interval = opt_u64(&p, "interval_ms")?.unwrap_or(2000).max(1000);
            let task = tokio::spawn(async move {
                if method == "docker.logs" {
                    let command = cmd.replacen("docker logs ", "docker logs --follow ", 1);
                    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
                    let run = conn.exec_privileged_io(&command, None, Some(tx));
                    tokio::pin!(run);
                    let result = loop {
                        tokio::select! {
                            out = &mut run => break out,
                            chunk = rx.recv() => if let Some((stderr, bytes)) = chunk {
                                use base64::Engine;
                                let ev = crate::daemon::proto::event("docker.logs", json!({"subscription":sub,"stderr":stderr,"data_b64":base64::engine::general_purpose::STANDARD.encode(bytes)}));
                                if !client.send(&ev).await { return; }
                            }
                        }
                    };
                    while let Some((stderr, bytes)) = rx.recv().await {
                        use base64::Engine;
                        if !client.send(&crate::daemon::proto::event("docker.logs", json!({"subscription":sub,"stderr":stderr,"data_b64":base64::engine::general_purpose::STANDARD.encode(bytes)}))).await { return; }
                    }
                    match result.and_then(|out| decode(&method, &out)) {
                        Ok(_) => {
                            client
                                .send(&crate::daemon::proto::event(
                                    "docker.logs.done",
                                    json!({"subscription":sub}),
                                ))
                                .await;
                        }
                        Err(e) => {
                            client
                                .send(&crate::daemon::proto::event(
                                    "error",
                                    json!({"subscription":sub,"error":e,"fatal":true}),
                                ))
                                .await;
                        }
                    }
                    return;
                }
                loop {
                    match conn.exec_privileged(&cmd).await.and_then(|out| decode(&method, &out)) {
                        Ok(data) => {
                            if !client
                                .send(&crate::daemon::proto::event(
                                    "docker.stats",
                                    json!({"subscription":sub,"data":data}),
                                ))
                                .await
                            {
                                break;
                            }
                        }
                        Err(e) => {
                            client
                                .send(&crate::daemon::proto::event(
                                    "error",
                                    json!({"subscription":sub,"error":e,"fatal":true}),
                                ))
                                .await;
                            break;
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(interval)).await;
                }
            });
            cx.state.subscriptions.lock().expect("subs").insert(
                id.clone(),
                crate::daemon::state::Subscription {
                    client: cx.client.id,
                    kind: "docker",
                    task: task.abort_handle(),
                },
            );
            return Ok(json!({"subscription":id,"interval_ms":interval}));
        }
        decode(method, &conn.exec_privileged(&cmd).await?)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssh::FakeExec;
    #[tokio::test]
    async fn list_and_failure() {
        let f = FakeExec::new().on("docker ps", r#"{"ID":"a1","Names":"web","State":"running"}"#, 0);
        assert_eq!(
            collect(&f, "docker.ps", &json!({})).await.unwrap()["items"][0]["ID"],
            "a1"
        );
        assert!(f.calls()[0].contains("LC_ALL=C"));
        assert_eq!(
            collect(&FakeExec::new(), "docker.ps", &json!({}))
                .await
                .unwrap_err()
                .code,
            "cap.missing"
        );
    }
    #[test]
    fn confirmation_and_quote() {
        assert!(command("docker.rm", &json!({"id":"x"})).is_err());
        assert!(command("docker.stop", &json!({"id":"a'; reboot","confirm":true}))
            .unwrap()
            .0
            .contains("'a'\\''; reboot'"));
    }
}

#[cfg(test)]
mod behavior_tests {
    use super::*;
    use crate::ssh::FakeExec;
    #[tokio::test]
    async fn logs_keep_both_streams() {
        let mut f = FakeExec::new().on("docker logs", "2026-10-24T10:00:00Z ready\n", 0);
        f.rules[0].1.stderr = b"2026-10-24T10:00:01Z warning\n".to_vec();
        let d = collect(&f, "docker.logs", &json!({"id":"web","tail":99999}))
            .await
            .unwrap();
        assert_eq!(d["lines"].as_array().unwrap().len(), 2);
        assert!(f.calls()[0].contains("--tail 5000"));
    }
    #[tokio::test]
    async fn inspect_omits_secrets() {
        let f = FakeExec::new().on(
            "docker inspect",
            r#"[{"Id":"abc","Config":{"Env":["PASSWORD=secret"],"Cmd":["secret"]},"Name":"/web"}]"#,
            0,
        );
        let d = collect(&f, "docker.inspect", &json!({"id":"web"})).await.unwrap();
        assert_eq!(d["items"][0]["Id"], "abc");
        assert!(!d.to_string().contains("secret"));
    }
    #[tokio::test]
    async fn failed_and_malformed() {
        let f = FakeExec::new().on("docker ps", "", 1);
        assert_eq!(
            collect(&f, "docker.ps", &json!({})).await.unwrap_err().code,
            "remote.command_failed"
        );
        let f = FakeExec::new().on("docker ps", "junk", 0);
        assert_eq!(
            collect(&f, "docker.ps", &json!({})).await.unwrap_err().code,
            "remote.bad_output"
        );
    }
    #[tokio::test]
    async fn mutations_are_explicit() {
        for action in ["start", "stop", "restart", "pause", "rm"] {
            let method = format!("docker.{action}");
            let f = FakeExec::new().on(&format!("docker {action}"), "web\n", 0);
            assert!(collect(&f, &method, &json!({"id":"web"})).await.is_err());
            assert!(f.calls().is_empty());
            assert_eq!(
                collect(&f, &method, &json!({"id":"web","confirm":true})).await.unwrap()["completed"],
                true
            );
        }
    }
}
