pub mod parse;
use crate::{
    daemon::{
        proto::{opt_str, req_str},
        registry::{Cx, MethodGroup},
    },
    error::{CoreError, CoreResult},
    ssh::{shell_quote, RemoteExec},
};
use async_trait::async_trait;
use serde_json::{json, Value};
fn name(s: &str, field: &str) -> CoreResult<String> {
    if s.starts_with('-') || s.is_empty() || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b)) {
        return Err(CoreError::invalid(field, "invalid name"));
    }
    Ok(shell_quote(s))
}
fn home(user: &str) -> CoreResult<String> {
    Ok(format!("entry=$(getent passwd {} ) || exit 44; home=$(printf '%s' \"$entry\" | cut -d: -f6); case \"$home\" in /*) ;; *) exit 44;; esac; file=\"$home/.ssh/authorized_keys\"; ",name(user,"user")?))
}
pub fn command(m: &str, p: &Value) -> CoreResult<(String, &'static str)> {
    let (cmd,tool)=match m {
 "sec.fail2ban.jails"=>("fail2ban-client status".into(),"fail2ban-client"),
 "sec.fail2ban.bans"=>(format!("fail2ban-client status {}",name(&req_str(p,"jail")?,"jail")?),"fail2ban-client"),
 "sec.fail2ban.ban"|"sec.fail2ban.unban"=>{let ip=req_str(p,"ip")?.parse::<std::net::IpAddr>().map_err(|_|CoreError::invalid("ip","invalid IP address"))?;(format!("fail2ban-client set {} {} {}",name(&req_str(p,"jail")?,"jail")?,if m.ends_with(".unban"){"unbanip"}else{"banip"},shell_quote(&ip.to_string())),"fail2ban-client")},
 "sec.logins"=>("if command -v journalctl >/dev/null 2>&1; then journalctl --since '-24 hours' -t sshd -t ssh -o json --no-pager -n 10000; elif [ -r /var/log/auth.log ]; then tail -n 10000 /var/log/auth.log; else exit 127; fi".into(),""),
 "sec.sessions"=>("who".into(),"who"),
 "sec.ports"=>("ss -H -lntup".into(),"ss"),
 "sec.updates"=>("apt list --upgradable 2>/dev/null".into(),"apt"),
 "sec.keys.list"=>{if let Some(user)=opt_str(p,"user")?{(format!("{} if [ -f \"$file\" ]; then cat -- \"$file\"; fi",home(&user)?),"getent")}else{("getent passwd | while IFS=: read -r user x uid gid desc home shell; do if [ -f \"$home/.ssh/authorized_keys\" ]; then printf '@@SHERPA_USER@@%s\\n' \"$user\"; cat -- \"$home/.ssh/authorized_keys\" || exit; printf '\\n'; fi; done".into(),"getent")}},
 "sec.keys.add"=>{let key=req_str(p,"public_key")?;if key.contains(['\n','\r','\0'])||!key.starts_with([ 's','e']){return Err(CoreError::invalid("public_key","expected one public key"));}let parsed=parse::keys(&key)?;if parsed["items"].as_array().map_or(0,Vec::len)!=1{return Err(CoreError::invalid("public_key","expected one public key"));}
 let user=req_str(p,"user")?;(format!("{} umask 077; mkdir -p -- \"$home/.ssh\" || exit; [ ! -L \"$file\" ] || exit 45; [ ! -L \"$home/.ssh\" ] || exit 45; exec 9<\"$home/.ssh\"; flock -x 9 || exit; printf '\\n%s\\n' {} >> \"$file\" || exit; chmod 600 -- \"$file\"; chown {} -- \"$home/.ssh\" \"$file\"",home(&user)?,shell_quote(&key),name(&user,"user")?),"flock")},
 _=>return Err(crate::daemon::registry::unknown(m))};
    Ok((
        format!(
            "LC_ALL=C; export LC_ALL; {}{cmd}",
            if tool.is_empty() {
                String::new()
            } else {
                format!("command -v {tool} >/dev/null 2>&1 || exit 127; ")
            }
        ),
        tool,
    ))
}
fn parse_keys(text: &str, p: &Value) -> CoreResult<Value> {
    if p.get("user").is_some() {
        return parse::keys(text);
    }
    let mut items = Vec::new();
    for section in text.split("@@SHERPA_USER@@").skip(1) {
        let (user, keys) = section
            .split_once('\n')
            .ok_or_else(|| CoreError::new("remote.bad_output", "Missing user header"))?;
        let parsed = parse::keys(keys)?;
        if let Some(a) = parsed["items"].as_array() {
            for key in a {
                let mut key = key.clone();
                key["user"] = user.into();
                items.push(key);
            }
        }
    }
    Ok(json!({"items":items}))
}
pub async fn collect<E: RemoteExec>(exec: &E, m: &str, p: &Value) -> CoreResult<Value> {
    let (cmd, tool) = command(m, p)?;
    let out = exec.exec(&cmd).await?;
    if out.exit_status == Some(127) {
        return Err(CoreError::new("cap.missing", "Security tool is missing").with(
            "capability",
            if tool.is_empty() {
                "journalctl or auth.log"
            } else {
                tool
            },
        ));
    }
    if !out.success() {
        return Err(super::command_failed(m, &out));
    }
    let text = out.stdout_lossy();
    match m {
        "sec.fail2ban.jails" => parse::jails(&text),
        "sec.fail2ban.bans" => parse::bans(&text),
        "sec.logins" => parse::logins(&text),
        "sec.sessions" => parse::sessions(&text),
        "sec.keys.list" => parse_keys(&text, p),
        "sec.ports" => parse::ports(&text),
        "sec.updates" => Ok(parse::updates(&text)),
        _ => Ok(json!({"success":true})),
    }
}
pub async fn delete_key<E: RemoteExec>(exec: &E, p: &Value) -> CoreResult<Value> {
    let user = req_str(p, "user")?;
    let fingerprint = req_str(p, "fingerprint")?;
    let read_p = json!({"user":user});
    let (cmd, _) = command("sec.keys.list", &read_p)?;
    let out = exec.exec(&cmd).await?;
    if !out.success() {
        return Err(super::command_failed("sec.keys.list", &out));
    }
    let text = out.stdout_lossy();
    let keys = parse::keys(&text)?;
    let line = keys["items"]
        .as_array()
        .and_then(|a| a.iter().find(|k| k["fingerprint"] == fingerprint))
        .and_then(|k| k["line"].as_u64())
        .ok_or_else(|| CoreError::new("sec.key_not_found", "Authorized key not found"))?;
    use sha2::{Digest, Sha256};
    let hash = hex::encode(Sha256::digest(&out.stdout));
    let cmd=format!("LC_ALL=C; export LC_ALL; command -v flock >/dev/null 2>&1 || exit 127; {} [ ! -L \"$file\" ] || exit 45; [ ! -L \"$home/.ssh\" ] || exit 45; exec 9<\"$home/.ssh\"; flock -x 9 || exit; actual=$(sha256sum -- \"$file\") || exit; [ \"${{actual%% *}}\" = {} ] || exit 46; umask 077; tmp=$(mktemp \"$home/.ssh/.sherpa-keys.XXXXXX\") || exit; trap 'rm -f -- \"$tmp\"' EXIT; awk 'NR != {line}' \"$file\" > \"$tmp\" && chown --reference=\"$file\" \"$tmp\" && chmod --reference=\"$file\" \"$tmp\" && mv -- \"$tmp\" \"$file\"",home(&user)?,shell_quote(&hash));
    let out = exec.exec(&cmd).await?;
    if out.exit_status == Some(46) {
        return Err(CoreError::new(
            "sec.keys_changed",
            "Authorized keys changed; refresh before retrying",
        ));
    }
    if !out.success() {
        return Err(super::command_failed("sec.keys.del", &out));
    }
    Ok(json!({"deleted":true}))
}
pub struct SecMethods;
#[async_trait]
impl MethodGroup for SecMethods {
    fn prefix(&self) -> &'static str {
        "sec"
    }
    fn methods(&self) -> &'static [&'static str] {
        &[
            "sec.score",
            "sec.fail2ban.jails",
            "sec.fail2ban.bans",
            "sec.fail2ban.ban",
            "sec.fail2ban.unban",
            "sec.logins",
            "sec.sessions",
            "sec.keys.list",
            "sec.keys.add",
            "sec.keys.del",
            "sec.ports",
            "sec.updates",
        ]
    }
    async fn call(&self, cx: &Cx, m: &str, p: Value) -> CoreResult<Value> {
        let conn = cx.conn(&req_str(&p, "connection_id")?).await?;
        let exec = super::management::Executor {
            conn: conn.as_ref(),
            privileged: true,
        };
        if matches!(
            m,
            "sec.fail2ban.ban" | "sec.fail2ban.unban" | "sec.keys.add" | "sec.keys.del"
        ) {
            conn.ensure_writable()?;
            if m == "sec.keys.del" && req_str(&p, "user")? == "root" {
                confirm_hostname(&exec, &p).await?;
            }
            cx.state.confirmations.consume(cx.client.id, m, &p)?;
        }
        if m == "sec.keys.del" {
            return delete_key(&exec, &p).await;
        }
        if m == "sec.score" {
            let mut checks = Vec::new();
            let mut unavailable = Vec::new();
            for method in ["sec.fail2ban.jails", "sec.ports", "sec.updates", "sec.keys.list"] {
                match collect(&exec, method, &p).await {
                    Ok(data) => checks.push(data),
                    Err(error) if error.code == "cap.missing" => {
                        unavailable.push(json!({"method":method,"error":error}));
                        checks.push(json!({"items":[]}));
                    }
                    Err(error) => return Err(error),
                }
            }
            let mut result = parse::score(&checks[0], &checks[1], &checks[2], &checks[3]);
            if !unavailable.is_empty() {
                result["score"] = result["score"].as_u64().unwrap_or(0).min(50).into();
            }
            result["complete"] = unavailable.is_empty().into();
            result["unavailable"] = unavailable.into();
            return Ok(result);
        }
        collect(&exec, m, &p).await
    }
}

/// Дополнительное подтверждение имени сервера для критичных изменений.
pub async fn confirm_hostname<E: RemoteExec>(exec: &E, p: &Value) -> CoreResult<()> {
    let expected = req_str(p, "confirm_hostname")?;
    let out = exec.exec("LC_ALL=C; export LC_ALL; hostname").await?;
    if !out.success() {
        return Err(super::command_failed("hostname", &out));
    }
    let actual = out.stdout_lossy();
    if actual.trim().is_empty() || actual.trim() != expected {
        return Err(CoreError::new(
            "danger.hostname_mismatch",
            "Server hostname confirmation does not match",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssh::FakeExec;
    #[tokio::test]
    async fn fake_checks() {
        let f = FakeExec::new().on("fail2ban-client status", "Status\n`- Jail list: sshd\n", 0);
        assert_eq!(
            collect(&f, "sec.fail2ban.jails", &json!({})).await.unwrap()["items"][0],
            "sshd"
        );
        assert!(f.calls()[0].contains("LC_ALL=C"));
        assert_eq!(
            collect(&FakeExec::new(), "sec.ports", &json!({}))
                .await
                .unwrap_err()
                .code,
            "cap.missing"
        );
        assert!(command("sec.fail2ban.ban", &json!({"jail":"sshd;id","ip":"192.0.2.1"})).is_err());
    }
    #[tokio::test]
    async fn changed_key_is_not_deleted() {
        let key = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEj4ldCoz6NwS7cHHY4qTPXWwD5y1fVdsN7ez0Y0H1E3 test\n";
        let fp = parse::keys(key).unwrap()["items"][0]["fingerprint"].clone();
        let f = FakeExec::new().on("cat --", key, 0).on("sha256sum", "", 46);
        assert_eq!(
            delete_key(&f, &json!({"user":"root","fingerprint":fp}))
                .await
                .unwrap_err()
                .code,
            "sec.keys_changed"
        );
    }
}
