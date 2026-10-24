pub mod parse;
use crate::{
    daemon::{
        proto::{opt_bool, opt_str, req_str},
        registry::{Cx, MethodGroup},
    },
    error::{CoreError, CoreResult},
    ssh::{shell_quote, RemoteExec},
};
use async_trait::async_trait;
use serde_json::{json, Value};
pub fn command(job: &Value, action: &str, p: &Value) -> CoreResult<String> {
    let engine = req_str(job, "engine")?;
    let source = req_str(job, "source")?;
    let repo = req_str(job, "repository")?;
    if !source.starts_with('/') || source.contains('\0') {
        return Err(CoreError::invalid("source", "expected absolute path"));
    }
    let cmd = match engine.as_str() {
        "restic" => {
            let password_file = req_str(job, "password_file")?;
            if !password_file.starts_with('/') {
                return Err(CoreError::invalid("password_file", "expected absolute path"));
            }
            let base = format!(
                "restic --repo {} --password-file {}",
                shell_quote(&repo),
                shell_quote(&password_file)
            );
            match action {
                "run" => format!("{base} backup --json -- {}", shell_quote(&source)),
                "snapshots" => format!("{base} snapshots --json"),
                "check" => format!("{base} check"),
                "prune" => format!("{base} prune"),
                "restore" => {
                    let target = req_str(p, "target")?;
                    if !target.starts_with('/') {
                        return Err(CoreError::invalid("target", "expected absolute path"));
                    }
                    let snapshot = req_str(p, "snapshot")?;
                    if snapshot.starts_with('-') {
                        return Err(CoreError::invalid("snapshot", "invalid snapshot"));
                    }
                    let dry = p.get("dry_run").is_none() || opt_bool(p, "dry_run")?;
                    format!(
                        "{base} restore {} --target {}{}",
                        shell_quote(&snapshot),
                        shell_quote(&target),
                        if dry { " --dry-run" } else { "" }
                    )
                }
                _ => return Err(CoreError::invalid("action", "unsupported restic action")),
            }
        }
        "rsync" if action == "run" => {
            if repo.starts_with('-') {
                return Err(CoreError::invalid("repository", "invalid destination"));
            }
            format!("rsync -a -s --stats -- {} {}", shell_quote(&source), shell_quote(&repo))
        }
        "rsync" => return Err(CoreError::new("cap.missing", "Operation requires restic").with("capability", "restic")),
        _ => return Err(CoreError::invalid("engine", "expected restic or rsync")),
    };
    Ok(format!(
        "LC_ALL=C; export LC_ALL; command -v {engine} >/dev/null 2>&1 || exit 127; nice -n 19 ionice -c3 {cmd}"
    ))
}
pub async fn execute<E: RemoteExec>(exec: &E, job: &Value, action: &str, p: &Value) -> CoreResult<Value> {
    let cmd = command(job, action, p)?;
    let out = exec.exec(&cmd).await?;
    if out.exit_status == Some(127) {
        return Err(CoreError::new("cap.missing", "Backup tool is missing").with("capability", job["engine"].clone()));
    }
    if !out.success() {
        return Err(CoreError::new("backup.failed", "Backup operation failed").with("exit_status", out.exit_status));
    }
    if action == "run" {
        return parse::run(&req_str(job, "engine")?, &out.stdout_lossy());
    }
    if action == "snapshots" {
        return parse::snapshots(&out.stdout_lossy());
    }
    Ok(json!({"success":true,"dry_run":action=="restore"&&(p.get("dry_run").is_none()||opt_bool(p,"dry_run")?)}))
}
pub struct BackupMethods;
#[async_trait]
impl MethodGroup for BackupMethods {
    fn prefix(&self) -> &'static str {
        "backup"
    }
    fn methods(&self) -> &'static [&'static str] {
        &[
            "backup.jobs.list",
            "backup.jobs.set",
            "backup.jobs.del",
            "backup.run",
            "backup.snapshots",
            "backup.restore",
            "backup.check",
            "backup.prune",
        ]
    }
    async fn call(&self, cx: &Cx, m: &str, p: Value) -> CoreResult<Value> {
        if m == "backup.jobs.list" {
            return cx
                .state
                .with_store(|s| super::notes::list(s, "backup", opt_str(&p, "connection_id")?.as_deref()));
        }
        if m == "backup.jobs.set" {
            let id = opt_str(&p, "id")?.unwrap_or_else(crate::util::new_id);
            let mut job = json!({"id":id,"connection_id":req_str(&p,"connection_id")?,"title":req_str(&p,"title")?,"engine":req_str(&p,"engine")?,"source":req_str(&p,"source")?,"repository":req_str(&p,"repository")?,"password_file":opt_str(&p,"password_file")?,"privileged":opt_bool(&p,"privileged")?,"schedule":opt_str(&p,"schedule")?});
            command(&job, "run", &json!({}))?;
            job["updated_at"] = crate::util::now().into();
            cx.state.with_store(|s| super::notes::put(s, "backup", &id, &job))?;
            return Ok(job);
        }
        let id = req_str(&p, "id")?;
        if m == "backup.jobs.del" {
            cx.state.confirmations.consume(cx.client.id, m, &p)?;
            return cx.state.with_store(|s| super::notes::delete(s, "backup", &id));
        }
        let job = cx.state.with_store(|s| super::notes::get(s, "backup", &id))?;
        let action = m.rsplit('.').next().unwrap_or("");
        let conn = cx.conn(&req_str(&job, "connection_id")?).await?;
        if matches!(action, "run" | "prune" | "restore") {
            conn.ensure_writable()?;
        }
        if action == "prune" || (action == "restore" && p.get("dry_run").is_some() && !opt_bool(&p, "dry_run")?) {
            if action == "restore" {
                super::sec::confirm_hostname(conn.as_ref(), &p).await?;
            }
            cx.state.confirmations.consume(cx.client.id, m, &p)?;
        }
        let exec = super::management::Executor {
            conn: conn.as_ref(),
            privileged: opt_bool(&job, "privileged")?,
        };
        let result = execute(&exec, &job, action, &p).await;
        if action == "run" {
            cx.state.with_store(|s| {
                let mut current = super::notes::get(s, "backup", &id)?;
                current["last_run_at"] = crate::util::now().into();
                current["last_run_ok"] = result.is_ok().into();
                if let Ok(data) = &result {
                    current["last_run_bytes"] = data["processed_bytes"].clone();
                }
                super::notes::put(s, "backup", &id, &current)
            })?;
        }
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssh::FakeExec;
    fn job() -> Value {
        json!({"engine":"restic","source":"/srv/a'b","repository":"sftp:host:/backup","password_file":"/root/.restic"})
    }
    #[tokio::test]
    async fn safe_restore_and_missing() {
        let p = json!({"snapshot":"abc","target":"/restore"});
        assert!(command(&job(), "restore", &p).unwrap().ends_with(" --dry-run"));
        assert!(!command(
            &job(),
            "restore",
            &json!({"snapshot":"abc","target":"/restore","dry_run":false})
        )
        .unwrap()
        .contains("dry-run"));
        let f = FakeExec::new().on("snapshots", "[]", 0);
        assert_eq!(
            execute(&f, &job(), "snapshots", &json!({})).await.unwrap()["items"],
            json!([])
        );
        assert_eq!(
            execute(&FakeExec::new(), &job(), "run", &json!({}))
                .await
                .unwrap_err()
                .code,
            "cap.missing"
        );
        assert!(command(&job(), "run", &json!({})).unwrap().contains("'\\''"));
    }
}
