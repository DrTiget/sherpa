pub mod parse;
use crate::daemon::{
    proto::{opt_bool, opt_str, opt_u64, req_str},
    registry::{unknown, Cx, MethodGroup},
};
use crate::error::{CoreError, CoreResult};
use crate::ssh::{shell_quote, ExecOutput, PooledConn, RemoteExec};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};

const PRE: &str = "LC_ALL=C; export LC_ALL; ";
fn token(s: &str) -> bool {
    !s.is_empty() && !s.starts_with('-') && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
}
fn source(p: &Value) -> CoreResult<(Option<String>, Option<String>)> {
    let user = opt_str(p, "user")?;
    let file = opt_str(p, "file")?;
    for (k, v) in [("user", &user), ("file", &file)] {
        if v.as_ref().is_some_and(|s| !token(s) || s == "." || s == "..") {
            return Err(CoreError::invalid(k, "invalid name"));
        }
    }
    if file.as_ref().is_some_and(|s| s.contains('.')) {
        return Err(CoreError::invalid(
            "file",
            "cron.d filename must contain letters, digits, underscore or dash",
        ));
    }
    if opt_bool(p, "system")? && (user.is_some() || file.is_some()) {
        return Err(CoreError::invalid("system", "system list cannot select a user or file"));
    }
    if user.is_some() && file.is_some() {
        return Err(CoreError::invalid("file", "user and file are mutually exclusive"));
    }
    Ok((user, file))
}
fn read_body(p: &Value) -> CoreResult<String> {
    let (user, file) = source(p)?;
    if let Some(file) = file {
        return Ok(format!(
            "p={}; if [ -L \"$p\" ]; then exit 2; fi; if [ -e \"$p\" ]; then cat -- \"$p\"; fi",
            shell_quote(&format!("/etc/cron.d/{file}"))
        ));
    }
    let arg = user.map(|u| format!(" -u {}", shell_quote(&u))).unwrap_or_default();
    // Только штатное «no crontab» даёт пустой список; прочие ошибки не скрываются.
    Ok(format!("for t in crontab mktemp grep rm; do command -v \"$t\" >/dev/null || {{ echo \"MISSING:$t\"; exit 127; }}; done; err=$(mktemp) || exit; crontab{arg} -l 2>\"$err\"; rc=$?; if [ $rc -ne 0 ] && grep -q '^no crontab for ' \"$err\"; then rc=0; fi; rm -f \"$err\"; exit $rc"))
}
fn safe_error(name: &str, out: &ExecOutput) -> CoreError {
    // crontab и systemctl могут печатать команды/окружение: не возвращаем stderr.
    super::command_failed(
        name,
        &ExecOutput {
            stderr: Vec::new(),
            ..out.clone()
        },
    )
}
async fn read<E: RemoteExec>(exec: &E, p: &Value) -> CoreResult<String> {
    let out = exec.exec(&format!("{PRE}{}", read_body(p)?)).await?;
    if out.exit_status == Some(127) && out.stdout_lossy().starts_with("MISSING:") {
        return Err(CoreError::new("cap.missing", "Required cron tool missing")
            .with("capability", out.stdout_lossy().trim_start_matches("MISSING:").trim()));
    }
    if !out.success() {
        return Err(safe_error("cron.read", &out));
    }
    String::from_utf8(out.stdout).map_err(|_| CoreError::new("remote.bad_output", "Crontab is not UTF-8"))
}
fn cron_field(field: &str, min: u32, max: u32, names: &[&str]) -> bool {
    let number = |value: &str| -> Option<u32> {
        value
            .parse::<u32>()
            .ok()
            .or_else(|| {
                names
                    .iter()
                    .position(|n| n.eq_ignore_ascii_case(value))
                    .map(|i| i as u32 + min)
            })
            .filter(|n| *n >= min && *n <= max)
    };
    field.split(',').all(|item| {
        let mut parts = item.split('/');
        let range = parts.next().unwrap_or("");
        if let Some(step) = parts.next() {
            if !step.parse::<u32>().is_ok_and(|n| n > 0 && n <= max + 1) || parts.next().is_some() {
                return false;
            }
        }
        if range == "*" {
            return true;
        }
        if let Some((a, b)) = range.split_once('-') {
            matches!((number(a),number(b)),(Some(a),Some(b)) if a <= b)
        } else {
            number(range).is_some()
        }
    })
}
fn job(p: &Value, system: bool) -> CoreResult<String> {
    let schedule = req_str(p, "schedule")?;
    let valid = if schedule.starts_with('@') {
        [
            "@reboot",
            "@yearly",
            "@annually",
            "@monthly",
            "@weekly",
            "@daily",
            "@midnight",
            "@hourly",
        ]
        .contains(&schedule.as_str())
    } else {
        let fields: Vec<_> = schedule.split_whitespace().collect();
        fields.len() == 5
            && cron_field(fields[0], 0, 59, &[])
            && cron_field(fields[1], 0, 23, &[])
            && cron_field(fields[2], 1, 31, &[])
            && cron_field(
                fields[3],
                1,
                12,
                &[
                    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
                ],
            )
            && cron_field(fields[4], 0, 7, &["sun", "mon", "tue", "wed", "thu", "fri", "sat"])
    };
    if !valid || schedule.contains(['\n', '\r', '\0']) {
        return Err(CoreError::invalid(
            "schedule",
            "five cron fields or standard @alias required",
        ));
    }
    let command = req_str(p, "command")?;
    if command.contains(['\n', '\r', '\0']) {
        return Err(CoreError::invalid("command", "single line required"));
    }
    let user = if system {
        let u = req_str(p, "run_as")?;
        if !token(&u) {
            return Err(CoreError::invalid("run_as", "invalid user"));
        }
        format!(" {u}")
    } else {
        String::new()
    };
    Ok(format!("{schedule}{user} {command}"))
}
pub fn edit(text: &str, method: &str, p: &Value) -> CoreResult<String> {
    let (_, file) = source(p)?;
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    if method == "cron.add" {
        lines.push(job(p, file.is_some())?);
    } else {
        let index = opt_u64(p, "line")?
            .filter(|n| *n > 0 && *n <= lines.len() as u64)
            .ok_or_else(|| CoreError::invalid("line", "existing line required"))? as usize
            - 1;
        // Запрещено удалять/включать окружение или комментарии через API задач.
        let d = parse::crontab(text, file.is_some());
        if d["items"][index]["kind"] != "job" {
            return Err(CoreError::invalid("line", "job line required"));
        }
        match method {
            "cron.set" => lines[index] = job(p, file.is_some())?,
            "cron.del" => {
                lines.remove(index);
            }
            "cron.toggle" => {
                if !p.get("enabled").is_some_and(Value::is_boolean) {
                    return Err(CoreError::required("enabled"));
                }
                let enabled = opt_bool(p, "enabled")?;
                let raw = lines[index].trim_start();
                lines[index] = if enabled {
                    raw.strip_prefix("#SHERPA_DISABLED ").unwrap_or(raw).into()
                } else if raw.starts_with("#SHERPA_DISABLED ") {
                    raw.into()
                } else {
                    format!("#SHERPA_DISABLED {raw}")
                };
            }
            _ => return Err(unknown(method)),
        }
    }
    Ok(if lines.is_empty() {
        String::new()
    } else {
        lines.join("\n") + "\n"
    })
}
fn write_script(p: &Value, version: &str, new: &str) -> CoreResult<String> {
    let (user, file) = source(p)?;
    let read = read_body(p)?;
    let install = if let Some(file) = file {
        let path = shell_quote(&format!("/etc/cron.d/{file}"));
        format!("p={path}; [ ! -L \"$p\" ] || exit 2; staged=$(mktemp /etc/cron.d/.sherpa-XXXXXX) || exit; if install -m 0644 \"$tmp/new\" \"$staged\" && mv -f -- \"$staged\" \"$p\"; then true; else rm -f -- \"$staged\"; exit 1; fi")
    } else {
        format!(
            "crontab{} \"$tmp/new\"",
            user.map(|u| format!(" -u {}", shell_quote(&u))).unwrap_or_default()
        )
    };
    Ok(format!("{PRE}for t in flock sha256sum base64 mktemp install mv rm; do command -v \"$t\" >/dev/null || {{ echo \"MISSING:$t\"; exit 127; }}; done; umask 077; exec 9>/run/lock/sherpa-cron.lock || exit; flock -x -w 10 9 || exit; tmp=$(mktemp -d) || exit; trap 'rm -rf -- \"$tmp\"' EXIT HUP INT TERM; ( {read} ) >\"$tmp/old\" || exit; hash=$(sha256sum \"$tmp/old\"); hash=${{hash%% *}}; [ \"$hash\" = {} ] || exit 73; printf %s {} | base64 -d >\"$tmp/new\" || exit; {install}",shell_quote(version),shell_quote(&STANDARD.encode(new))))
}

pub async fn collect<E: RemoteExec>(exec: &E, method: &str, p: &Value) -> CoreResult<Value> {
    if method.starts_with("cron.timer.") {
        let action = method.trim_start_matches("cron.timer.");
        let cmd = if action == "list" {
            "systemctl list-units --type=timer --all --plain --no-legend --no-pager >\"$tmp\" || exit; while read -r unit rest; do [ -n \"$unit\" ] || continue; systemctl show --no-pager -p Id -p ActiveState -p UnitFileState -p NextElapseUSecRealtime -p LastTriggerUSec -p Unit -- \"$unit\" || exit; echo; done <\"$tmp\"".to_string()
        } else {
            if !["enable", "disable"].contains(&action) {
                return Err(unknown(method));
            }
            if !opt_bool(p, "confirm")? {
                return Err(CoreError::invalid("confirm", "explicit confirmation required"));
            }
            let unit = req_str(p, "unit")?;
            if !token(&unit) || !unit.ends_with(".timer") {
                return Err(CoreError::invalid("unit", "timer unit required"));
            }
            format!("systemctl {action} --now -- {}", shell_quote(&unit))
        };
        let out=exec.exec(&format!("{PRE}command -v systemctl >/dev/null || {{ echo MISSING:systemctl; exit 127; }}; tmp=$(mktemp) || exit; trap 'rm -f \"$tmp\"' EXIT HUP INT TERM; {cmd}")).await?;
        if out.exit_status == Some(127) {
            return Err(CoreError::new("cap.missing", "systemctl not installed").with("capability", "systemctl"));
        }
        if !out.success() {
            return Err(safe_error(method, &out));
        }
        return if action == "list" {
            parse::timers(&out.stdout_lossy())
        } else {
            Ok(json!({"updated":true}))
        };
    }
    if method == "cron.list" && opt_bool(p, "system")? {
        let out=exec.exec(&format!("{PRE}for f in /etc/cron.d/*; do [ -f \"$f\" ] && [ ! -L \"$f\" ] || continue; printf '%s\\0' \"$f\"; cat -- \"$f\" || exit; printf '\\0'; done")).await?;
        if !out.success() {
            return Err(safe_error(method, &out));
        }
        return Ok(json!({"files":parse::system_files(&out.stdout_lossy())?}));
    }
    if method != "cron.list" && !["cron.add", "cron.set", "cron.del", "cron.toggle"].contains(&method) {
        return Err(unknown(method));
    }
    if method != "cron.list" && !opt_bool(p, "confirm")? {
        return Err(CoreError::invalid("confirm", "explicit confirmation required"));
    }
    let old = read(exec, p).await?;
    if method == "cron.list" {
        return Ok(parse::crontab(&old, source(p)?.1.is_some()));
    }
    let expected = req_str(p, "version")?;
    if expected != parse::version(&old) {
        return Err(CoreError::new("cron.changed", "Crontab changed; reload before editing"));
    }
    let new = edit(&old, method, p)?;
    let out = exec.exec(&write_script(p, &expected, &new)?).await?;
    if out.exit_status == Some(73) {
        return Err(CoreError::new("cron.changed", "Crontab changed; reload before editing"));
    }
    if out.exit_status == Some(127) && out.stdout_lossy().starts_with("MISSING:") {
        return Err(CoreError::new("cap.missing", "Required cron tool missing")
            .with("capability", out.stdout_lossy().trim_start_matches("MISSING:").trim()));
    }
    if !out.success() {
        return Err(safe_error(method, &out));
    }
    Ok(json!({"updated":true,"version":parse::version(&new)}))
}
struct Executor<'a> {
    conn: &'a PooledConn,
    sudo: bool,
}
impl RemoteExec for Executor<'_> {
    async fn exec_input(&self, cmd: &str, _: Option<&[u8]>) -> CoreResult<ExecOutput> {
        if self.sudo {
            self.conn.exec_privileged(cmd).await
        } else {
            self.conn.exec(cmd).await
        }
    }
}
pub struct CronMethods;
#[async_trait]
impl MethodGroup for CronMethods {
    fn prefix(&self) -> &'static str {
        "cron"
    }
    fn methods(&self) -> &'static [&'static str] {
        &[
            "cron.list",
            "cron.add",
            "cron.set",
            "cron.del",
            "cron.toggle",
            "cron.timer.list",
            "cron.timer.enable",
            "cron.timer.disable",
        ]
    }
    async fn call(&self, cx: &Cx, method: &str, p: Value) -> CoreResult<Value> {
        if !self.methods().contains(&method) {
            return Err(unknown(method));
        }
        source(&p)?;
        let conn = cx.conn(&req_str(&p, "connection_id")?).await?;
        let write = method != "cron.list" && method != "cron.timer.list";
        if write {
            conn.ensure_writable()?;
        }
        let sudo = write
            || opt_str(&p, "user")?.is_some()
            || opt_str(&p, "file")?.is_some()
            || opt_bool(&p, "system")?
            || opt_bool(&p, "sudo")?;
        // При записи без user sudo изменил бы crontab root вместо текущего пользователя.
        if write
            && !method.starts_with("cron.timer.")
            && opt_str(&p, "user")?.is_none()
            && opt_str(&p, "file")?.is_none()
        {
            return Err(CoreError::required("user"));
        }
        collect(
            &Executor {
                conn: conn.as_ref(),
                sudo,
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
    const OLD: &str = "# header\nMAILTO=ops@example.org\n0 3 * * * /bin/backup\n";
    #[test]
    fn schedule_validation() {
        for schedule in ["60 * * * *", "*/0 * * * *", "* 24 * * *", "* * 0 * *", "* * * wat *"] {
            assert!(
                job(&json!({"schedule":schedule,"command":"true"}), false).is_err(),
                "{schedule}"
            );
        }
        assert!(job(&json!({"schedule":"0 1-3/2 * jan mon-fri","command":"true"}), false).is_ok());
    }
    #[test]
    fn editing_preserves_other_lines() {
        let p = json!({"line":3,"enabled":false});
        let d = edit(OLD, "cron.toggle", &p).unwrap();
        assert!(d.starts_with("# header\nMAILTO=ops@example.org\n"));
        assert!(d.contains("#SHERPA_DISABLED 0 3"));
        assert_eq!(edit(&d, "cron.toggle", &json!({"line":3,"enabled":true})).unwrap(), OLD);
        assert_eq!(
            edit(OLD, "cron.del", &json!({"line":3})).unwrap(),
            "# header\nMAILTO=ops@example.org\n"
        );
        assert!(edit(OLD, "cron.del", &json!({"line":2})).is_err());
    }
    #[tokio::test]
    async fn list_and_write() {
        let f = FakeExec::new().on("flock -x", "", 0).on(
            "crontab",
            "# header\nMAILTO=ops@example.org\n0 3 * * * /bin/backup\n",
            0,
        );
        assert_eq!(
            collect(&f, "cron.list", &json!({"user":"deploy"})).await.unwrap()["items"][2]["line"],
            3
        );
        let p = json!({"user":"deploy","confirm":true,"version":parse::version(OLD),"schedule":"@daily","command":"echo 'hi'"});
        assert_eq!(collect(&f, "cron.add", &p).await.unwrap()["updated"], true);
        let calls = f.calls();
        assert!(calls.last().unwrap().contains("sha256sum"));
        assert!(calls.last().unwrap().contains("crontab -u 'deploy'"));
    }
    #[tokio::test]
    async fn conflicts_and_errors() {
        let f = FakeExec::new().on("crontab", OLD, 0);
        assert_eq!(
            collect(&f, "cron.del", &json!({"confirm":true,"version":"stale"}))
                .await
                .unwrap_err()
                .code,
            "cron.changed"
        );
        assert!(collect(&f, "cron.del", &json!({})).await.is_err());
        assert!(source(&json!({"file":"../escape"})).is_err());
        assert!(job(&json!({"schedule":"* * * * *\n@reboot","command":"true"}), false).is_err());
        assert_eq!(
            collect(&FakeExec::new().on("crontab", "", 1), "cron.list", &json!({}))
                .await
                .unwrap_err()
                .code,
            "remote.command_failed"
        );
        assert_eq!(
            collect(
                &FakeExec::new().on("crontab", "MISSING:crontab", 127),
                "cron.list",
                &json!({})
            )
            .await
            .unwrap_err()
            .code,
            "cap.missing"
        );
    }
    #[tokio::test]
    async fn system_and_timers() {
        let f = FakeExec::new()
            .on("/etc/cron.d/*", "/etc/cron.d/backup\0@daily root backup\n\0", 0)
            .on("systemctl", "Id=backup.timer\nActiveState=active\n\n", 0);
        assert_eq!(
            collect(&f, "cron.list", &json!({"system":true})).await.unwrap()["files"][0]["items"][0]["user"],
            "root"
        );
        assert_eq!(
            collect(&f, "cron.timer.list", &json!({})).await.unwrap()["items"][0]["Id"],
            "backup.timer"
        );
        assert!(collect(
            &f,
            "cron.timer.disable",
            &json!({"unit":"backup.service","confirm":true})
        )
        .await
        .is_err());
    }
}
