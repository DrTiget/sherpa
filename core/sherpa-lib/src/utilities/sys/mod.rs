//! Настройки системы и локальные учётные записи.
pub mod parse;
use super::management::{checked, guarded, name, Executor};
use crate::daemon::proto::{opt_bool, opt_str, opt_u64, req_str};
use crate::daemon::registry::{unknown, Cx, MethodGroup};
use crate::error::{CoreError, CoreResult};
use crate::ssh::{shell_quote, RemoteExec};
use async_trait::async_trait;
use serde_json::{json, Value};
use zeroize::Zeroizing;

pub struct Plan {
    pub command: String,
    pub input: Option<Zeroizing<Vec<u8>>>,
    pub mutating: bool,
    pub dangerous: bool,
    pub tool: &'static str,
}
pub fn plan(method: &str, p: &Value) -> CoreResult<Plan> {
    let mut input = None;
    let mutating = !matches!(method, "sys.info" | "sys.time.get" | "sys.user.list" | "sys.group.list");
    let dangerous = matches!(
        method,
        "sys.hostname.set" | "sys.power" | "sys.user.remove" | "sys.group.remove"
    );
    let account = || -> CoreResult<String> { name(&req_str(p, "name")?, "name") };
    let (tool, cmd) = match method {
        "sys.info" => ("hostname", "hostname && uname -s && uname -r && uname -m".into()),
        "sys.hostname.set" => {
            let h = req_str(p, "hostname")?;
            if h.len() > 253
                || h.split('.').any(|s| {
                    s.is_empty()
                        || s.starts_with('-')
                        || s.ends_with('-')
                        || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                })
            {
                return Err(CoreError::invalid("hostname", "expected DNS hostname"));
            }
            (
                "hostnamectl",
                format!("hostnamectl set-hostname -- {}", shell_quote(&h)),
            )
        }
        "sys.time.get" => (
            "timedatectl",
            "timedatectl show --no-pager --property=Timezone,TimeUSec,RTCTimeUSec,NTP,NTPSynchronized,CanNTP".into(),
        ),
        "sys.time.set_zone" => {
            let z = req_str(p, "timezone")?;
            if z.starts_with('/')
                || z.split('/').any(|s| {
                    s.is_empty() || s == ".." || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b"_+-".contains(&b))
                })
            {
                return Err(CoreError::invalid("timezone", "expected zoneinfo name"));
            }
            (
                "timedatectl",
                format!("timedatectl set-timezone -- {}", shell_quote(&z)),
            )
        }
        "sys.time.set" => {
            let t = req_str(p, "time")?;
            if t.len() > 64 || !t.bytes().all(|b| b.is_ascii_digit() || b"-: .+".contains(&b)) {
                return Err(CoreError::invalid("time", "expected date and time"));
            }
            ("timedatectl", format!("timedatectl set-time -- {}", shell_quote(&t)))
        }
        "sys.time.ntp" => {
            if !p.get("enabled").is_some_and(Value::is_boolean) {
                return Err(CoreError::required("enabled"));
            }
            (
                "timedatectl",
                format!("timedatectl set-ntp {}", opt_bool(p, "enabled")?),
            )
        }
        "sys.user.list" => (
            "getent",
            "getent passwd && printf '@@SHERPA@@\\n' && getent group".into(),
        ),
        "sys.group.list" => ("getent", "getent group".into()),
        "sys.user.add" => {
            let user = account()?;
            let mut flags = if opt_bool(p, "create_home")? {
                "-m".to_string()
            } else {
                "-M".to_string()
            };
            for (field, flag) in [("home", "-d"), ("shell", "-s")] {
                if let Some(v) = opt_str(p, field)? {
                    if !v.starts_with('/') || v.contains(['\n', '\0']) {
                        return Err(CoreError::invalid(field, "expected absolute path"));
                    }
                    flags += &format!(" {flag} {}", shell_quote(&v));
                }
            }
            ("useradd", format!("useradd {flags} -- {user}"))
        }
        "sys.group.add" => ("groupadd", format!("groupadd -- {}", account()?)),
        "sys.user.remove" => {
            let user = account()?;
            if req_str(p, "name")? == "root" {
                return Err(CoreError::invalid("name", "cannot remove root"));
            }
            (
                "userdel",
                format!(
                    "userdel {} -- {user}",
                    if opt_bool(p, "remove_home")? { "-r" } else { "" }
                ),
            )
        }
        "sys.group.remove" => {
            let group = account()?;
            if req_str(p, "name")? == "root" {
                return Err(CoreError::invalid("name", "cannot remove root group"));
            }
            ("groupdel", format!("groupdel -- {group}"))
        }
        "sys.user.passwd" => {
            account()?;
            let password = Zeroizing::new(req_str(p, "password")?);
            if password.contains(['\n', '\r', '\0', ':']) {
                return Err(CoreError::invalid(
                    "password",
                    "password contains unsupported characters",
                ));
            }
            input = Some(Zeroizing::new(
                format!("{}:{}\n", req_str(p, "name")?, password.as_str()).into_bytes(),
            ));
            ("chpasswd", "chpasswd 2>/dev/null".into())
        }
        "sys.user.sudo" => {
            let user = account()?;
            if !p.get("enabled").is_some_and(Value::is_boolean) {
                return Err(CoreError::required("enabled"));
            }
            let g = opt_str(p, "group")?;
            let group=match g.as_deref() {Some("sudo")=>"g=sudo;",Some("wheel")=>"g=wheel;",None=>"if getent group sudo >/dev/null; then g=sudo; elif getent group wheel >/dev/null; then g=wheel; else echo SHERPA_MISSING; exit 127; fi;",_=>return Err(CoreError::invalid("group","sudo | wheel"))};
            if opt_bool(p, "enabled")? {
                ("usermod", format!("{group} usermod -a -G \"$g\" -- {user}"))
            } else {
                ("gpasswd", format!("{group} gpasswd -d {user} \"$g\""))
            }
        }
        "sys.power" => {
            let action = req_str(p, "action")?;
            let flag = match action.as_str() {
                "reboot" => "-r",
                "shutdown" => "-h",
                _ => return Err(CoreError::invalid("action", "reboot | shutdown")),
            };
            let delay = opt_u64(p, "delay_sec")?.unwrap_or(60);
            if !(60..=86400).contains(&delay) {
                return Err(CoreError::invalid("delay_sec", "60..86400"));
            }
            ("shutdown", format!("shutdown {flag} +{}", delay.div_ceil(60)))
        }
        _ => return Err(unknown(method)),
    };
    Ok(Plan {
        command: guarded(tool, &cmd),
        tool,
        input,
        mutating,
        dangerous,
    })
}
pub async fn collect<E: RemoteExec>(exec: &E, method: &str, p: &Value) -> CoreResult<Value> {
    let plan = plan(method, p)?;
    let out = exec
        .exec_input(&plan.command, plan.input.as_deref().map(|v| v.as_slice()))
        .await?;
    if method == "sys.user.passwd" && !out.success() {
        return Err(CoreError::new("sys.password_failed", "Password update failed").with("name", req_str(p, "name")?));
    }
    checked(method, plan.tool, &out)?;
    match method {
        "sys.info" => parse::info(&out.stdout_lossy()),
        "sys.time.get" => parse::time(&out.stdout_lossy()),
        "sys.user.list" => parse::users(&out.stdout_lossy()),
        "sys.group.list" => parse::groups(&out.stdout_lossy()),
        "sys.power" => Ok(json!({"scheduled":true,"delay_sec":opt_u64(p,"delay_sec")?.unwrap_or(60).div_ceil(60)*60})),
        _ => Ok(json!({"changed":true})),
    }
}
pub struct SysMethods;
#[async_trait]
impl MethodGroup for SysMethods {
    fn prefix(&self) -> &'static str {
        "sys"
    }
    fn methods(&self) -> &'static [&'static str] {
        &[
            "sys.info",
            "sys.hostname.set",
            "sys.time.get",
            "sys.time.set_zone",
            "sys.time.set",
            "sys.time.ntp",
            "sys.user.list",
            "sys.user.add",
            "sys.user.remove",
            "sys.user.passwd",
            "sys.user.sudo",
            "sys.group.list",
            "sys.group.add",
            "sys.group.remove",
            "sys.power",
        ]
    }
    async fn call(&self, cx: &Cx, method: &str, p: Value) -> CoreResult<Value> {
        let plan = plan(method, &p)?;
        let conn = cx.conn(&req_str(&p, "connection_id")?).await?;
        if plan.mutating {
            conn.ensure_writable()?;
        }
        if plan.dangerous {
            super::management::confirm(cx, &conn, method, &p, true).await?;
        }
        collect(
            &Executor {
                conn: &conn,
                privileged: plan.mutating,
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
    use crate::ssh::{ExecOutput, FakeExec};
    #[tokio::test]
    async fn reads_and_errors() {
        let f = FakeExec::new().on("hostname &&", "srv\nLinux\n6.1.0-18-amd64\nx86_64\n", 0);
        assert_eq!(collect(&f, "sys.info", &json!({})).await.unwrap()["hostname"], "srv");
        assert!(f.calls()[0].starts_with("LC_ALL=C"));
        let f = FakeExec::new().on("getent", "", 1);
        assert_eq!(
            collect(&f, "sys.user.list", &json!({})).await.unwrap_err().code,
            "remote.command_failed"
        );
    }
    #[test]
    fn validate_and_confirm() {
        assert!(plan("sys.power", &json!({"action":"reboot"})).unwrap().dangerous);
        assert!(plan("sys.user.remove", &json!({"name":"root"})).is_err());
        assert!(plan("sys.time.ntp", &json!({})).is_err());
        assert!(plan("sys.time.set_zone", &json!({"timezone":"../../etc/passwd"})).is_err());
        assert!(plan("sys.user.add", &json!({"name":"--help"})).is_err());
        assert!(plan("sys.power", &json!({"action":"reboot","delay_sec":0})).is_err());
    }
    struct PasswordExec;
    impl RemoteExec for PasswordExec {
        async fn exec_input(&self, c: &str, i: Option<&[u8]>) -> CoreResult<ExecOutput> {
            assert!(!c.contains("secret"));
            assert_eq!(i.unwrap(), b"deploy:secret\n");
            Ok(ExecOutput {
                exit_status: Some(0),
                ..Default::default()
            })
        }
    }
    #[tokio::test]
    async fn password_uses_stdin() {
        collect(
            &PasswordExec,
            "sys.user.passwd",
            &json!({"name":"deploy","password":"secret"}),
        )
        .await
        .unwrap();
    }
    #[test]
    fn newline_password_rejected() {
        assert!(plan("sys.user.passwd", &json!({"name":"deploy","password":"s\nroot:hacked"})).is_err());
    }
    #[tokio::test]
    async fn mutations() {
        for (m, p, cmd) in [
            ("sys.hostname.set", json!({"hostname":"web.example.org"}), "hostnamectl"),
            ("sys.time.set_zone", json!({"timezone":"Europe/Moscow"}), "timedatectl"),
            ("sys.time.ntp", json!({"enabled":true}), "timedatectl"),
            ("sys.user.add", json!({"name":"deploy","create_home":true}), "useradd"),
            ("sys.group.add", json!({"name":"ops"}), "groupadd"),
            ("sys.user.sudo", json!({"name":"deploy","enabled":true}), "usermod"),
            (
                "sys.user.remove",
                json!({"name":"deploy","remove_home":true}),
                "userdel",
            ),
            ("sys.group.remove", json!({"name":"ops"}), "groupdel"),
            (
                "sys.power",
                json!({"action":"shutdown","delay_sec":61}),
                "shutdown -h +2",
            ),
        ] {
            let f = FakeExec::new().on(cmd, "", 0);
            collect(&f, m, &p).await.unwrap();
            assert!(plan(m, &p).unwrap().mutating);
        }
    }
}
