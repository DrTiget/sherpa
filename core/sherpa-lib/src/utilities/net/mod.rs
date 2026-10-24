//! Network inspection and transactional firewall changes.
pub mod parse;
mod transaction;

use super::command_failed;
use crate::daemon::proto::{opt_bool, opt_str, opt_u64, req_str};
use crate::daemon::registry::{unknown, Cx, MethodGroup};
use crate::error::{CoreError, CoreResult};
use crate::ssh::{shell_quote, RemoteExec};
use async_trait::async_trait;
use serde_json::{json, Value};

pub fn backend(p: &Value) -> CoreResult<String> {
    let b = req_str(p, "backend")?;
    if !matches!(b.as_str(), "ufw" | "nft" | "iptables") {
        return Err(CoreError::invalid("backend", "ufw | nft | iptables"));
    }
    Ok(b)
}

pub async fn collect<E: RemoteExec>(exec: &E, kind: &str, p: &Value) -> CoreResult<Value> {
    let (tool, cmd): (&str, String) = match kind {
        "net.iface.list" => ("ip", "ip -j address show".into()),
        "net.route.list" => ("ip", "ip -j -4 route show table all; r=$?; [ $r -eq 0 ] || exit $r; echo @@SHERPA@@; ip -j -6 route show table all".into()),
        "net.dns.get" => ("cat", "cat /etc/resolv.conf || exit $?; printf '\\n@@SHERPA@@\\n'; if command -v resolvectl >/dev/null 2>&1; then resolvectl dns 2>/dev/null || true; fi".into()),
        "net.sockets" => ("ss", "ss -tulpnH -e".into()),
        "net.firewall.list" => {
            let b=backend(p)?;
            let cmd=match b.as_str() { "ufw"=>"ufw status numbered", "nft"=>"nft -j -a list ruleset", _=>"iptables -S" };
            return run_collect(exec, &b, cmd, kind, p).await;
        }
        _ => return Err(unknown(kind)),
    };
    run_collect(exec, tool, &cmd, kind, p).await
}

async fn run_collect<E: RemoteExec>(exec: &E, tool: &str, cmd: &str, kind: &str, p: &Value) -> CoreResult<Value> {
    let script = format!(
        "LC_ALL=C; export LC_ALL; command -v {} >/dev/null 2>&1 || exit 127; {cmd}",
        shell_quote(tool)
    );
    let out = exec.exec(&script).await?;
    if out.exit_status == Some(127) {
        return Err(CoreError::new("cap.missing", "Network tool is unavailable").with("capability", tool));
    }
    if !out.success() {
        return Err(command_failed(kind, &out));
    }
    let text = out.stdout_lossy();
    match kind {
        "net.iface.list" => parse::ip(&text),
        "net.route.list" => {
            let (v4, v6) = text.split_once("@@SHERPA@@\n").ok_or_else(parse::bad)?;
            Ok(json!({"ipv4":parse::ip(v4)?["items"],"ipv6":parse::ip(v6)?["items"]}))
        }
        "net.dns.get" => parse::dns(&text),
        "net.sockets" => parse::sockets(&text),
        "net.firewall.list" => parse::firewall(&backend(p)?, &text),
        _ => Err(unknown(kind)),
    }
}

/// Only typed fields become firewall arguments; arbitrary shell/rules are never accepted.
pub fn rule_command(p: &Value) -> CoreResult<String> {
    let b = backend(p)?;
    let operation = req_str(p, "operation")?;
    let chain = opt_str(p, "chain")?.unwrap_or_else(|| {
        if b == "iptables" {
            "INPUT".into()
        } else {
            "input".into()
        }
    });
    let ident = |s: &str| !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !ident(&chain) {
        return Err(CoreError::invalid("chain", "expected identifier"));
    }
    let table = opt_str(p, "table")?.unwrap_or_else(|| "filter".into());
    if !ident(&table) {
        return Err(CoreError::invalid("table", "expected identifier"));
    }
    if operation == "delete" {
        let id = opt_u64(p, "rule_id")?
            .filter(|n| *n > 0)
            .ok_or_else(|| CoreError::invalid("rule_id", "expected positive rule number/handle"))?;
        return Ok(match b.as_str() {
            "ufw" => format!("ufw --force delete {id}"),
            "nft" => format!(
                "nft delete rule inet {} {} handle {id}",
                shell_quote(&table),
                shell_quote(&chain)
            ),
            _ => format!(
                "iptables -w 5 -t {} -D {} {id}",
                shell_quote(&table),
                shell_quote(&chain)
            ),
        });
    }
    if operation != "add" {
        return Err(CoreError::invalid("operation", "add | delete"));
    }
    let action = req_str(p, "action")?;
    if !matches!(action.as_str(), "allow" | "deny" | "reject") {
        return Err(CoreError::invalid("action", "allow | deny | reject"));
    }
    let proto = req_str(p, "protocol")?;
    if !matches!(proto.as_str(), "tcp" | "udp") {
        return Err(CoreError::invalid("protocol", "tcp | udp"));
    }
    let port = opt_u64(p, "port")?
        .filter(|n| (1..=65535).contains(n))
        .ok_or_else(|| CoreError::invalid("port", "1..65535"))?;
    let source = opt_str(p, "source")?.unwrap_or_else(|| "0.0.0.0/0".into());
    let (ip, prefix) = source.split_once('/').unwrap_or((&source, "32"));
    if ip.parse::<std::net::Ipv4Addr>().is_err() || prefix.parse::<u8>().ok().is_none_or(|n| n > 32) {
        return Err(CoreError::invalid("source", "expected IPv4 address/CIDR"));
    }
    let src = shell_quote(&source);
    Ok(match b.as_str() {
        "ufw" => format!("ufw {action} in proto {proto} from {src} to any port {port}"),
        "nft" => format!(
            "nft add rule inet {} {} ip saddr {src} {proto} dport {port} {}",
            shell_quote(&table),
            shell_quote(&chain),
            if action == "allow" {
                "accept"
            } else if action == "deny" {
                "drop"
            } else {
                "reject"
            }
        ),
        _ => format!(
            "iptables -w 5 -t {} -A {} -s {src} -p {proto} --dport {port} -j {}",
            shell_quote(&table),
            shell_quote(&chain),
            if action == "allow" {
                "ACCEPT"
            } else if action == "deny" {
                "DROP"
            } else {
                "REJECT"
            }
        ),
    })
}

pub struct NetMethods;
#[async_trait]
impl MethodGroup for NetMethods {
    fn prefix(&self) -> &'static str {
        "net"
    }
    fn methods(&self) -> &'static [&'static str] {
        &[
            "net.iface.list",
            "net.route.list",
            "net.dns.get",
            "net.sockets",
            "net.firewall.list",
            "net.apply",
            "net.confirm",
            "net.rollback",
        ]
    }
    async fn call(&self, cx: &Cx, method: &str, p: Value) -> CoreResult<Value> {
        let conn = cx.conn(&req_str(&p, "connection_id")?).await?;
        if method == "net.apply" {
            conn.ensure_writable()?;
            if !opt_bool(&p, "confirm")? {
                return Err(CoreError::invalid("confirm", "explicit confirmation is required"));
            }
            let b = backend(&p)?;
            let cmd = rule_command(&p)?;
            let timeout = opt_u64(&p, "confirm_timeout")?.unwrap_or(60);
            if !(10..=120).contains(&timeout) {
                return Err(CoreError::invalid("confirm_timeout", "10..120 seconds"));
            }
            cx.state.confirmations.consume(cx.client.id, method, &p)?;
            let id = uuid::Uuid::now_v7().to_string();
            let script = transaction::apply_script(&b, &id, &cmd, timeout);
            let out = conn.exec_privileged(&script).await?;
            transaction::check(&out)?;
            return Ok(json!({"transaction":id,"confirm_timeout":timeout,"state":"pending"}));
        }
        if matches!(method, "net.confirm" | "net.rollback") {
            conn.ensure_writable()?;
            let id = req_str(&p, "transaction")?;
            if uuid::Uuid::parse_str(&id).is_err() {
                return Err(CoreError::invalid("transaction", "expected UUID"));
            }
            let action = if method == "net.confirm" { "confirm" } else { "rollback" };
            let out = conn.exec_privileged(&transaction::finish_script(&id, action)).await?;
            transaction::check(&out)?;
            return Ok(json!({"transaction":id,"state":out.stdout_lossy().trim()}));
        }
        if method == "net.firewall.list" {
            // Privileged adapter keeps the parser/collector testable without SSH.
            let b = backend(&p)?;
            let cmd = match b.as_str() {
                "ufw" => "ufw status numbered",
                "nft" => "nft -j -a list ruleset",
                _ => "iptables -S",
            };
            let out = conn
                .exec_privileged(&format!(
                    "LC_ALL=C; export LC_ALL; command -v {b} >/dev/null 2>&1 || exit 127; {cmd}"
                ))
                .await?;
            if out.exit_status == Some(127) {
                return Err(CoreError::new("cap.missing", "Firewall tool is unavailable").with("capability", b));
            }
            if !out.success() {
                return Err(command_failed(method, &out));
            }
            return parse::firewall(&b, &out.stdout_lossy());
        }
        collect(conn.as_ref(), method, &p).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssh::FakeExec;
    #[tokio::test]
    async fn collector() {
        let f = FakeExec::new().on("ip -j address", "[]", 0);
        assert_eq!(
            collect(&f, "net.iface.list", &json!({})).await.unwrap()["items"],
            json!([])
        );
        assert!(f.calls()[0].starts_with("LC_ALL=C"));
    }
    #[tokio::test]
    async fn failures() {
        let f = FakeExec::new().on("ip -j", "", 2);
        assert_eq!(
            collect(&f, "net.iface.list", &json!({})).await.unwrap_err().code,
            "remote.command_failed"
        );
        assert_eq!(
            collect(&FakeExec::new(), "net.iface.list", &json!({}))
                .await
                .unwrap_err()
                .code,
            "cap.missing"
        );
    }
    #[tokio::test]
    async fn both_routes() {
        let f = FakeExec::new().on("route show", "[]\n@@SHERPA@@\n[]\n", 0);
        assert_eq!(
            collect(&f, "net.route.list", &json!({})).await.unwrap()["ipv6"],
            json!([])
        );
    }
    #[test]
    fn typed_rules() {
        for b in ["ufw", "nft", "iptables"] {
            let p = json!({"backend":b,"operation":"add","protocol":"tcp","port":22,"action":"allow","source":"192.0.2.0/24"});
            assert!(rule_command(&p).unwrap().contains("22"));
            let mut bad = p.clone();
            bad["source"] = json!("$(id)");
            assert!(rule_command(&bad).is_err());
            bad = p;
            bad["chain"] = json!("input;reboot");
            assert!(rule_command(&bad).is_err());
        }
    }
    #[test]
    fn deletion_validation() {
        assert_eq!(
            rule_command(&json!({"backend":"ufw","operation":"delete","rule_id":3})).unwrap(),
            "ufw --force delete 3"
        );
        assert!(rule_command(&json!({"backend":"ufw","operation":"delete","rule_id":0})).is_err());
    }
}
