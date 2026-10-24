//! Пакеты: менеджер из capabilities, структурированные запросы, потоковые операции.
pub mod parse;
use super::management::{checked, guarded, name, Executor};
use crate::daemon::proto::{event, req_str};
use crate::daemon::registry::{unknown, Cx, MethodGroup};
use crate::daemon::state::Subscription;
use crate::error::{CoreError, CoreResult};
use crate::ssh::{shell_quote, ExecOutput, RemoteExec};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use tokio::sync::mpsc;

fn package_names(p: &Value, required: bool) -> CoreResult<String> {
    let values = match p.get("packages") {
        None if !required => return Ok(String::new()),
        Some(Value::Array(a)) if !a.is_empty() && a.len() <= 200 => a,
        _ => return Err(CoreError::invalid("packages", "expected 1..200 package names")),
    };
    values
        .iter()
        .map(|v| {
            let s = v
                .as_str()
                .ok_or_else(|| CoreError::invalid("packages", "expected package names"))?;
            name(s, "packages")
        })
        .collect::<CoreResult<Vec<_>>>()
        .map(|v| v.join(" "))
}
pub fn validate(method: &str, p: &Value) -> CoreResult<()> {
    match method {
        "pkg.detect" | "pkg.installed" | "pkg.updates" => {}
        "pkg.search" => {
            let q = req_str(p, "query")?;
            if q.len() > 200 || q.starts_with('-') || q.contains(['\n', '\0']) {
                return Err(CoreError::invalid("query", "invalid search expression"));
            }
        }
        "pkg.install" | "pkg.remove" => {
            package_names(p, true)?;
        }
        "pkg.upgrade" => {
            package_names(p, false)?;
        }
        _ => return Err(unknown(method)),
    };
    Ok(())
}
fn tool(manager: &str) -> CoreResult<&'static str> {
    match manager {
        "apt" => Ok("apt-get"),
        "dnf" => Ok("dnf"),
        "yum" => Ok("yum"),
        "pacman" => Ok("pacman"),
        "apk" => Ok("apk"),
        _ => Err(CoreError::new("cap.missing", "Unsupported package manager").with("capability", "package_manager")),
    }
}
pub fn command(manager: &str, method: &str, p: &Value) -> CoreResult<String> {
    validate(method, p)?;
    let t = tool(manager)?;
    let args = if matches!(method, "pkg.install" | "pkg.remove" | "pkg.upgrade") {
        package_names(p, method != "pkg.upgrade")?
    } else {
        String::new()
    };
    let query = if method == "pkg.search" {
        shell_quote(&req_str(p, "query")?)
    } else {
        String::new()
    };
    let cmd = match (manager, method) {
        ("apt", "pkg.installed") => {
            "dpkg-query -W '-f=${binary:Package}\\t${Version}\\t${Architecture}\\t${db:Status-Status}\\n'".into()
        }
        ("apt", "pkg.search") => format!("apt-cache search --names-only -- {query}"),
        ("apt", "pkg.updates") => "apt list --upgradable 2>/dev/null".into(),
        ("dnf" | "yum", "pkg.installed") => "rpm -qa --qf '%{NAME}\\t%{VERSION}-%{RELEASE}\\t%{ARCH}\\n'".into(),
        ("dnf" | "yum", "pkg.search") => format!("{t} -q search -- {query}"),
        ("dnf" | "yum", "pkg.updates") => format!("{t} -q check-update"),
        ("pacman", "pkg.installed") => "pacman -Q".into(),
        ("pacman", "pkg.search") => format!("pacman -Ss -- {query}"),
        ("pacman", "pkg.updates") => "pacman -Qu".into(),
        ("apk", "pkg.installed") => "apk info -v".into(),
        ("apk", "pkg.search") => format!("apk search -v -- {query}"),
        ("apk", "pkg.updates") => "apk list --upgradable".into(),
        (_, "pkg.install" | "pkg.remove" | "pkg.upgrade") => {
            let a = method.trim_start_matches("pkg.");
            match manager {
                "apt"=>format!("DEBIAN_FRONTEND=noninteractive apt-get -y -o Dpkg::Options::=--force-confdef -o Dpkg::Options::=--force-confold {} -- {args}",if a=="upgrade" && !args.is_empty(){"install --only-upgrade"}else{a}),
                "dnf"|"yum"=>format!("{t} -y {a} -- {args}"),
                "pacman"=>format!("pacman --noconfirm {} -- {args}",match a {"install"=>"-S --needed","remove"=>"-R",_ if !args.is_empty()=>"-S --needed",_=>"-Syu"}),
                "apk"=>format!("apk --no-interactive {} -- {args}",match a {"install"=>"add","remove"=>"del",_=>"upgrade"}),
                _=>return Err(unknown(method)),
            }
        }
        _ => return Err(unknown(method)),
    };
    Ok(guarded(t, &cmd))
}
fn check(manager: &str, method: &str, out: &ExecOutput) -> CoreResult<()> {
    if method == "pkg.updates" && matches!(manager, "dnf" | "yum") && out.exit_status == Some(100) {
        return Ok(());
    }
    if matches!(method, "pkg.updates" | "pkg.search")
        && manager == "pacman"
        && out.exit_status == Some(1)
        && out.stdout.is_empty()
        && out.stderr.is_empty()
    {
        return Ok(());
    }
    let stderr = out.stderr_lossy().to_lowercase();
    if !out.success()
        && [
            "could not get lock",
            "unable to lock",
            "failed to acquire",
            "database is locked",
            "waiting for process with pid",
            "unable to lock database",
        ]
        .iter()
        .any(|s| stderr.contains(s))
    {
        return Err(CoreError::new("pkg.locked", "Package database is locked").with("manager", manager));
    }
    checked(method, tool(manager)?, out)
}
pub async fn collect<E: RemoteExec>(exec: &E, manager: &str, method: &str, p: &Value) -> CoreResult<Value> {
    let out = exec.exec(&command(manager, method, p)?).await?;
    check(manager, method, &out)?;
    parse::packages(manager, method.trim_start_matches("pkg."), &out.stdout_lossy())
}
pub async fn run<E: RemoteExec>(
    exec: &E,
    manager: &str,
    method: &str,
    p: &Value,
    output: mpsc::Sender<(bool, Vec<u8>)>,
) -> CoreResult<Value> {
    let out = exec.exec_stream(&command(manager, method, p)?, None, output).await?;
    check(manager, method, &out)?;
    Ok(json!({"changed":true,"exit_status":out.exit_status}))
}
pub struct PkgMethods;
#[async_trait]
impl MethodGroup for PkgMethods {
    fn prefix(&self) -> &'static str {
        "pkg"
    }
    fn methods(&self) -> &'static [&'static str] {
        &[
            "pkg.detect",
            "pkg.installed",
            "pkg.search",
            "pkg.updates",
            "pkg.install",
            "pkg.remove",
            "pkg.upgrade",
        ]
    }
    async fn call(&self, cx: &Cx, method: &str, p: Value) -> CoreResult<Value> {
        validate(method, &p)?;
        let connection = req_str(&p, "connection_id")?;
        let conn = cx.conn(&connection).await?;
        let caps = super::capabilities::get(&conn, false).await?;
        let manager = caps["package_manager"]
            .as_str()
            .ok_or_else(|| CoreError::new("cap.missing", "No package manager").with("capability", "package_manager"))?
            .to_string();
        tool(&manager)?;
        if method == "pkg.detect" {
            return Ok(json!({"manager":manager}));
        }
        if matches!(method, "pkg.installed" | "pkg.search" | "pkg.updates") {
            return collect(conn.as_ref(), &manager, method, &p).await;
        }
        conn.ensure_writable()?;
        super::management::confirm(cx, &conn, method, &p, method != "pkg.install").await?;
        let id = cx.state.new_id("pkg");
        let (operation, method, client, state) = (id.clone(), method.to_string(), cx.client.clone(), cx.state.clone());
        let (start, ready) = tokio::sync::oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            if ready.await.is_err() {
                return;
            }
            let (tx, mut rx) = mpsc::channel(8);
            let exec = Executor {
                conn: &conn,
                privileged: true,
            };
            let work = async {
                conn.ensure_writable()?;
                run(&exec, &manager, &method, &p, tx).await
            };
            tokio::pin!(work);
            let mut output_open = true;
            let result = loop {
                tokio::select! {
                    value=&mut work=>break value,
                    chunk=rx.recv(),if output_open=>match chunk {
                        Some((stderr,data))=>{client.send(&event("pkg.output",json!({"operation":operation,"connection_id":connection,"stream":if stderr{"stderr"}else{"stdout"},"data_b64":STANDARD.encode(data)}))).await;},
                        None=>output_open=false,
                    }
                }
            };
            while let Ok((stderr, data)) = rx.try_recv() {
                client.send(&event("pkg.output",json!({"operation":operation,"connection_id":connection,"stream":if stderr{"stderr"}else{"stdout"},"data_b64":STANDARD.encode(data)}))).await;
            }
            match result {
                Ok(data) => {
                    client
                        .send(&event(
                            "pkg.done",
                            json!({"operation":operation,"connection_id":connection,"data":data}),
                        ))
                        .await;
                }
                Err(error) => {
                    client
                        .send(&event(
                            "error",
                            json!({"operation":operation,"connection_id":connection,"error":error,"fatal":true}),
                        ))
                        .await;
                }
            }
            state.subscriptions.lock().expect("subs").remove(&operation);
        });
        cx.state.subscriptions.lock().expect("subs").insert(
            id.clone(),
            Subscription {
                client: cx.client.id,
                kind: "pkg",
                task: task.abort_handle(),
            },
        );
        let _ = start.send(());
        Ok(json!({"operation":id}))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssh::FakeExec;
    #[tokio::test]
    async fn all_manager_commands() {
        for (m, fixture) in [
            ("apt", "bash\t5.2\tamd64\tinstalled\n"),
            ("dnf", "bash\t5.2\tx86_64\n"),
            ("yum", "bash\t5.2\tx86_64\n"),
            ("pacman", "bash 5.2\n"),
            ("apk", "bash-5.2-r0\n"),
        ] {
            let f = FakeExec::new().on("LC_ALL=C", fixture, 0);
            assert_eq!(
                collect(&f, m, "pkg.installed", &json!({})).await.unwrap()["items"][0]["name"],
                "bash"
            );
        }
    }
    #[tokio::test]
    async fn exit_codes() {
        let f = FakeExec::new().on("check-update", "bash.x86_64 5.2 updates\n", 100);
        assert!(collect(&f, "dnf", "pkg.updates", &json!({})).await.is_ok());
        let f = FakeExec::new().on("pacman -Qu", "", 1);
        assert!(collect(&f, "pacman", "pkg.updates", &json!({})).await.is_ok());
        let f = FakeExec::new().on("LC_ALL=C", "", 42);
        assert_eq!(
            collect(&f, "apk", "pkg.installed", &json!({})).await.unwrap_err().code,
            "remote.command_failed"
        );
    }
    #[tokio::test]
    async fn streams_output() {
        let f = FakeExec::new().on("apt-get -y", "Reading package lists...\n", 0);
        let (tx, mut rx) = mpsc::channel(8);
        assert!(run(&f, "apt", "pkg.install", &json!({"packages":["nginx"]}), tx)
            .await
            .is_ok());
        assert_eq!(rx.recv().await.unwrap().1, b"Reading package lists...\n");
    }
    #[test]
    fn mutation_commands_and_validation() {
        for m in ["apt", "dnf", "yum", "pacman", "apk"] {
            for method in ["pkg.install", "pkg.remove", "pkg.upgrade"] {
                assert!(command(m, method, &json!({"packages":["nginx"]}))
                    .unwrap()
                    .contains("'nginx'"));
            }
        }
        assert!(validate("pkg.install", &json!({"packages":["--allow-unauthenticated"]})).is_err());
        assert!(validate("pkg.install", &json!({"packages":[]})).is_err());
        assert!(command("zypper", "pkg.installed", &json!({})).is_err());
    }
    #[test]
    fn locked() {
        let out = ExecOutput {
            stderr: b"E: Could not get lock /var/lib/dpkg/lock-frontend".to_vec(),
            exit_status: Some(100),
            ..Default::default()
        };
        assert_eq!(check("apt", "pkg.install", &out).unwrap_err().code, "pkg.locked");
    }
}
