//! systemd: структурированные юниты, статус, журнал и действия.
pub mod parse;
use super::management::{checked, guarded, name, Executor};
use crate::daemon::proto::{opt_u64, req_str};
use crate::daemon::registry::{unknown, Cx, MethodGroup};
use crate::error::{CoreError, CoreResult};
use crate::ssh::RemoteExec;
use async_trait::async_trait;
use serde_json::{json, Value};

const PROPERTIES: &str = "Id,Description,LoadState,ActiveState,SubState,UnitFileState,MainPID,MemoryCurrent,User";
pub struct Plan {
    pub command: String,
    pub tool: &'static str,
    pub mutating: bool,
}
pub fn plan(method: &str, p: &Value) -> CoreResult<Plan> {
    let action = method.strip_prefix("svc.").ok_or_else(|| unknown(method))?;
    let mutating = matches!(action, "start" | "stop" | "restart" | "reload" | "enable" | "disable");
    let (tool, cmd) = match action {
        "list" => (
            "systemctl",
            format!(
                r#"files=$(systemctl list-unit-files --type=service --no-legend --no-pager) || exit $?; loaded=$(systemctl list-units --all --type=service --plain --no-legend --no-pager) || exit $?; units=$(printf '%s\n%s\n' "$files" "$loaded" | awk '{{print $1}}' | grep -v '@\.' | sort -u); set -f; set --; for unit in $units; do set -- "$@" "$unit"; done; [ "$#" -eq 0 ] || systemctl show --no-pager --property={PROPERTIES} -- "$@""#
            ),
        ),
        "status" => (
            "systemctl",
            format!(
                "systemctl show --no-pager --property={PROPERTIES} -- {}",
                name(&req_str(p, "unit")?, "unit")?
            ),
        ),
        "logs" => (
            "journalctl",
            format!(
                "journalctl --no-pager --output=json --lines={} --unit={}",
                opt_u64(p, "lines")?.unwrap_or(100).min(2000),
                name(&req_str(p, "unit")?, "unit")?
            ),
        ),
        _ if mutating => (
            "systemctl",
            format!(
                "systemctl --no-ask-password {action} -- {}",
                name(&req_str(p, "unit")?, "unit")?
            ),
        ),
        _ => return Err(unknown(method)),
    };
    Ok(Plan {
        command: guarded(tool, &cmd),
        tool,
        mutating,
    })
}
pub async fn collect<E: RemoteExec>(exec: &E, method: &str, p: &Value) -> CoreResult<Value> {
    let plan = plan(method, p)?;
    let out = exec.exec(&plan.command).await?;
    checked(method, plan.tool, &out)?;
    match method {
        "svc.list" => parse::units(&out.stdout_lossy()),
        "svc.status" => {
            let data = parse::units(&out.stdout_lossy())?;
            let item = data["items"]
                .as_array()
                .and_then(|v| v.first())
                .ok_or_else(|| super::management::bad(method))?;
            if item["load_state"] == "not-found" {
                return Err(CoreError::new("svc.not_found", "Unit not found").with("unit", req_str(p, "unit")?));
            }
            Ok(item.clone())
        }
        "svc.logs" => parse::logs(&out.stdout_lossy()),
        _ => Ok(json!({"changed":true})),
    }
}
pub struct SvcMethods;
#[async_trait]
impl MethodGroup for SvcMethods {
    fn prefix(&self) -> &'static str {
        "svc"
    }
    fn methods(&self) -> &'static [&'static str] {
        &[
            "svc.list",
            "svc.status",
            "svc.start",
            "svc.stop",
            "svc.restart",
            "svc.reload",
            "svc.enable",
            "svc.disable",
            "svc.logs",
        ]
    }
    async fn call(&self, cx: &Cx, method: &str, p: Value) -> CoreResult<Value> {
        let plan = plan(method, &p)?;
        let conn = cx.conn(&req_str(&p, "connection_id")?).await?;
        if plan.mutating {
            conn.ensure_writable()?;
            super::management::confirm(cx, &conn, method, &p, true).await?;
        }
        collect(
            &Executor {
                conn: &conn,
                privileged: plan.mutating || method == "svc.logs",
            },
            method,
            &p,
        )
        .await
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssh::FakeExec;
    #[tokio::test]
    async fn status_and_locale() {
        let f = FakeExec::new().on(
            "systemctl show",
            "Id=nginx.service\nLoadState=loaded\nActiveState=active\n",
            0,
        );
        assert_eq!(
            collect(&f, "svc.status", &json!({"unit":"nginx.service"}))
                .await
                .unwrap()["active_state"],
            "active"
        );
        assert!(f.calls()[0].starts_with("LC_ALL=C"));
    }
    #[tokio::test]
    async fn failures_and_missing() {
        for (text, code, expected) in [
            ("", 1, "remote.command_failed"),
            ("SHERPA_MISSING", 127, "cap.missing"),
            ("junk", 0, "remote.bad_output"),
        ] {
            let f = FakeExec::new().on("systemctl", text, code);
            assert_eq!(collect(&f, "svc.list", &json!({})).await.unwrap_err().code, expected);
        }
    }
    #[tokio::test]
    async fn actions() {
        for action in ["start", "stop", "restart", "reload", "enable", "disable"] {
            let method = format!("svc.{action}");
            let p = json!({"unit":"sshd.service"});
            assert!(plan(&method, &p).unwrap().mutating);
            let f = FakeExec::new().on("systemctl", "", 0);
            assert_eq!(collect(&f, &method, &p).await.unwrap()["changed"], true);
            assert!(f.calls()[0].contains(&format!("{action} -- 'sshd.service'")));
        }
    }
    #[test]
    fn reject_options_and_injection() {
        for unit in ["--all", "a; reboot", "*", "a\nb"] {
            assert!(plan("svc.stop", &json!({"unit":unit})).is_err());
        }
    }
}
