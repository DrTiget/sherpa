//! `desktop.capabilities`: что умеет сервер (ОС, systemd, пакетный менеджер,
//! утилиты, права). Определяется одной командой и кэшируется в соединении.

use std::collections::HashMap;

use serde_json::{json, Value};

use super::command_failed;
use crate::error::CoreResult;
use crate::ssh::{PooledConn, RemoteExec};

pub const TOOLS: &[&str] = &[
    "sudo",
    "systemctl",
    "journalctl",
    "ss",
    "ip",
    "docker",
    "nginx",
    "apachectl",
    "certbot",
    "openssl",
    "mysql",
    "mariadb",
    "psql",
    "redis-cli",
    "mysqldump",
    "mariadb-dump",
    "pg_dump",
    "ufw",
    "nft",
    "iptables",
    "crontab",
    "ps",
    "df",
    "du",
    "lsblk",
    "findmnt",
    "find",
    "tail",
    "readlink",
    "timeout",
    "nice",
    "ionice",
    "flock",
    "sha256sum",
    "base64",
    "apt-get",
    "dnf",
    "yum",
    "pacman",
    "apk",
    "zypper",
];

pub fn script() -> String {
    let tools = TOOLS.join(" ");
    format!(
        "LC_ALL=C; export LC_ALL\n\
         if [ -r /etc/os-release ]; then . /etc/os-release; fi\n\
         echo \"os_id=${{ID:-}}\"; echo \"os_version=${{VERSION_ID:-}}\"; echo \"os_name=${{PRETTY_NAME:-}}\"\n\
         echo \"kernel_name=$(uname -s)\"; echo \"kernel_release=$(uname -r)\"; echo \"arch=$(uname -m)\"\n\
         echo \"uid=$(id -u)\"; echo \"user=$(id -un)\"\n\
         [ -d /run/systemd/system ] && echo systemd=1\n\
         [ -r /proc/stat ] && echo procfs=1\n\
         for t in {tools}; do command -v \"$t\" >/dev/null 2>&1 && echo \"tool_$t=1\"; done\n\
         command -v sudo >/dev/null 2>&1 && sudo -n true >/dev/null 2>&1 && echo sudo_nopasswd=1\n\
         true\n"
    )
}

/// Разбор вывода `script()` в JSON (CORE_API §4.5).
pub fn parse(text: &str) -> Value {
    let kv: HashMap<&str, &str> = text.lines().filter_map(|l| l.split_once('=')).collect();
    let get = |k: &str| kv.get(k).copied().filter(|v| !v.is_empty());
    let flag = |k: &str| kv.get(k) == Some(&"1");
    let tool = |t: &str| flag(&format!("tool_{t}"));
    let pm = [
        ("apt-get", "apt"),
        ("dnf", "dnf"),
        ("yum", "yum"),
        ("pacman", "pacman"),
        ("apk", "apk"),
        ("zypper", "zypper"),
    ]
    .iter()
    .find(|(t, _)| tool(t))
    .map(|(_, n)| *n);
    let uid: Option<u32> = get("uid").and_then(|u| u.parse().ok());
    let systemd = flag("systemd");
    let tools: serde_json::Map<String, Value> = TOOLS.iter().map(|t| (t.to_string(), tool(t).into())).collect();
    json!({
        "os": {
            "id": get("os_id"),
            "version": get("os_version"),
            "name": get("os_name"),
            "kernel": get("kernel_name"),
            "kernel_release": get("kernel_release"),
            "arch": get("arch"),
        },
        "user": { "name": get("user"), "uid": uid, "is_root": uid == Some(0) },
        "privileges": { "sudo": tool("sudo"), "sudo_nopasswd": flag("sudo_nopasswd") || uid == Some(0) },
        "systemd": systemd,
        "procfs": flag("procfs"),
        "package_manager": pm,
        "tools": tools,
        "utilities": {
            "terminal": true,
            "monitor": flag("procfs"),
            "services": systemd && tool("systemctl"),
            "logs": tool("journalctl") || tool("tail"),
            "network": tool("ss") || tool("ip"),
            "firewall": tool("ufw") || tool("nft") || tool("iptables"),
            "disks": tool("df"),
            "packages": pm.is_some(),
            "cron": tool("crontab"),
            "docker": tool("docker"),
            "web": tool("nginx") || tool("apachectl"),
            "db": tool("mysql") || tool("mariadb") || tool("psql") || tool("redis-cli"),
        },
    })
}

pub async fn detect<E: RemoteExec>(exec: &E) -> CoreResult<Value> {
    let cmd = script();
    let out = exec.exec(&cmd).await?;
    if !out.success() {
        return Err(command_failed("capabilities", &out));
    }
    Ok(parse(&out.stdout_lossy()))
}

/// Возможности из кэша соединения (или определить заново).
pub async fn get(conn: &PooledConn, refresh: bool) -> CoreResult<Value> {
    let mut cache = conn.capabilities.lock().await;
    if !refresh {
        if let Some(v) = cache.as_ref() {
            return Ok(v.clone());
        }
    }
    let v = detect(conn).await?;
    *cache = Some(v.clone());
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssh::FakeExec;

    #[tokio::test]
    async fn parses_debian_with_systemd() {
        let out = "os_id=debian\nos_version=12\nos_name=Debian GNU/Linux 12 (bookworm)\nkernel_name=Linux\n\
                   kernel_release=6.1.0\narch=x86_64\nuid=1000\nuser=deploy\nsystemd=1\nprocfs=1\n\
                   tool_sudo=1\ntool_systemctl=1\ntool_journalctl=1\ntool_ss=1\ntool_apt-get=1\ntool_ps=1\ntool_df=1\n";
        let fake = FakeExec::new().on("os-release", out, 0);
        let c = detect(&fake).await.unwrap();
        assert_eq!(c["os"]["id"], "debian");
        assert_eq!(c["os"]["name"], "Debian GNU/Linux 12 (bookworm)");
        assert_eq!(c["user"]["is_root"], false);
        assert_eq!(c["privileges"]["sudo"], true);
        assert_eq!(c["privileges"]["sudo_nopasswd"], false);
        assert_eq!(c["package_manager"], "apt");
        assert_eq!(c["utilities"]["monitor"], true);
        assert_eq!(c["utilities"]["services"], true);
        assert_eq!(c["utilities"]["docker"], false);
        assert_eq!(c["tools"]["docker"], false);
    }

    #[test]
    fn minimal_root_alpine() {
        let c = parse("os_id=alpine\nuid=0\nuser=root\ntool_apk=1\n");
        assert_eq!(c["user"]["is_root"], true);
        assert_eq!(c["privileges"]["sudo_nopasswd"], true);
        assert_eq!(c["package_manager"], "apk");
        assert_eq!(c["systemd"], false);
        assert_eq!(c["utilities"]["monitor"], false);
        assert_eq!(c["os"]["version"], Value::Null);
    }
}
