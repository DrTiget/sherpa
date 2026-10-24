//! Pure parsers for iproute2, resolv.conf, ss and firewall output.
use crate::error::{CoreError, CoreResult};
use serde_json::{json, Value};

pub fn bad() -> CoreError {
    CoreError::new("remote.bad_output", "Unexpected network command output")
}

pub fn ip(text: &str) -> CoreResult<Value> {
    let v: Value = serde_json::from_str(text).map_err(|_| bad())?;
    let items = v.as_array().ok_or_else(bad)?;
    if items.iter().any(|v| !v.is_object()) {
        return Err(bad());
    }
    Ok(json!({"items": items}))
}

pub fn dns(text: &str) -> CoreResult<Value> {
    let (conf, resolved) = text.split_once("@@SHERPA@@\n").ok_or_else(bad)?;
    let mut servers = Vec::new();
    let mut search = Vec::new();
    let mut options = Vec::new();
    for line in conf.lines() {
        let line = line.split(['#', ';']).next().unwrap_or("");
        let fields: Vec<_> = line.split_whitespace().collect();
        match fields.first().copied() {
            Some("nameserver") if fields.len() == 2 => {
                if fields[1].parse::<std::net::IpAddr>().is_err() {
                    return Err(bad());
                }
                servers.push(fields[1]);
            }
            Some("search" | "domain") => search.extend_from_slice(&fields[1..]),
            Some("options") => options.extend_from_slice(&fields[1..]),
            _ => {}
        }
    }
    let links: Vec<Value> = resolved
        .lines()
        .filter_map(|line| {
            let (link, rest) = line.split_once(':')?;
            Some(json!({"link":link.trim(), "servers":rest.split_whitespace().collect::<Vec<_>>() }))
        })
        .collect();
    Ok(json!({"servers":servers,"search":search,"options":options,"resolved":links}))
}

pub fn sockets(text: &str) -> CoreResult<Value> {
    let mut items = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let f: Vec<_> = line.split_whitespace().collect();
        if f.len() < 6 || !matches!(f[0], "tcp" | "udp" | "tcp6" | "udp6") {
            return Err(bad());
        }
        let endpoint = |s: &str| -> CoreResult<Value> {
            let (addr, port) = s.rsplit_once(':').ok_or_else(bad)?;
            let port = if port == "*" {
                Value::Null
            } else {
                json!(port.parse::<u16>().map_err(|_| bad())?)
            };
            Ok(json!({"address":addr.trim_matches(['[',']']),"port":port}))
        };
        let rest = f[6..].join(" ");
        let mut processes = Vec::new();
        for part in rest.split("(\"").skip(1) {
            let (name, attrs) = part.split_once('"').ok_or_else(bad)?;
            let number = |key: &str| {
                attrs
                    .split_once(key)
                    .and_then(|(_, s)| s.split(|c: char| !c.is_ascii_digit()).next())
                    .and_then(|s| s.parse::<u64>().ok())
            };
            processes.push(json!({"name":name,"pid":number("pid="),"fd":number("fd=")}));
        }
        items.push(json!({"protocol":f[0],"state":f[1],"recv_queue":f[2].parse::<u64>().map_err(|_| bad())?,"send_queue":f[3].parse::<u64>().map_err(|_| bad())?,"local":endpoint(f[4])?,"peer":endpoint(f[5])?,"uid":rest.split_whitespace().find_map(|f|f.strip_prefix("uid:").and_then(|v|v.parse::<u64>().ok())),"processes":processes}));
    }
    Ok(json!({"items":items}))
}

pub fn firewall(backend: &str, text: &str) -> CoreResult<Value> {
    if backend == "nft" {
        let v: Value = serde_json::from_str(text).map_err(|_| bad())?;
        let entries = v.get("nftables").and_then(Value::as_array).ok_or_else(bad)?;
        return Ok(json!({"backend":backend,"items":entries}));
    }
    let mut items = Vec::new();
    let mut table = "filter";
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        if backend == "ufw" {
            if !line.starts_with('[') {
                continue;
            }
            let (id, rule) = line.split_once(']').ok_or_else(bad)?;
            let id: u64 = id.trim_start_matches('[').trim().parse().map_err(|_| bad())?;
            let (rule, comment) = rule.split_once('#').unwrap_or((rule, ""));
            let fields: Vec<_> = rule.split_whitespace().collect();
            let action_at = fields
                .iter()
                .position(|s| matches!(*s, "ALLOW" | "DENY" | "REJECT" | "LIMIT"))
                .ok_or_else(bad)?;
            items.push(json!({"id":id,"destination":fields[..action_at].join(" "),"action":fields[action_at],"direction":fields.get(action_at+1),"source":fields.get(action_at+2..).unwrap_or_default().join(" "),"comment":comment.trim()}));
        } else if line.starts_with('#') || line == "COMMIT" {
            continue;
        } else if let Some(name) = line.strip_prefix('*') {
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(bad());
            }
            table = name;
        } else if line.starts_with("-A ") || line.starts_with("-P ") || line.starts_with("-N ") || line.starts_with(':')
        {
            items.push(json!({"table":table,"tokens":line.split_whitespace().collect::<Vec<_>>()}));
        } else {
            return Err(bad());
        }
    }
    if backend == "ufw" && !text.starts_with("Status:") {
        return Err(bad());
    }
    Ok(
        json!({"backend":backend,"active":if backend=="ufw" {Some(text.starts_with("Status: active"))} else {None},"items":items}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn iproute2() {
        let v=ip(r#"[{"ifindex":1,"ifname":"lo","flags":["LOOPBACK","UP"],"mtu":65536,"addr_info":[{"family":"inet","local":"127.0.0.1","prefixlen":8}]}]"#).unwrap();
        assert_eq!(v["items"][0]["ifname"], "lo");
        assert!(ip("{}").is_err());
        assert!(ip("[null]").is_err());
    }
    #[test]
    fn routes() {
        assert_eq!(
            ip(r#"[{"dst":"default","gateway":"192.0.2.1","dev":"eth0","metric":100}]"#).unwrap()["items"][0]["metric"],
            100
        );
        assert!(ip("garbage").is_err());
    }
    #[test]
    fn resolvers() {
        let v=dns("# systemd-resolved\nnameserver 127.0.0.53\nsearch example.org\noptions edns0 trust-ad\n@@SHERPA@@\nGlobal:\nLink 2 (eth0): 192.0.2.53 2001:db8::53\n").unwrap();
        assert_eq!(v["resolved"][1]["servers"][1], "2001:db8::53");
        assert!(dns("nameserver bad\n@@SHERPA@@\n").is_err());
    }
    #[test]
    fn ss_processes() {
        let v=sockets("tcp LISTEN 0 128 [::]:22 [::]:* users:((\"sshd\",pid=812,fd=3),(\"systemd\",pid=1,fd=10))\nudp UNCONN 0 0 127.0.0.53%lo:53 0.0.0.0:*\n").unwrap();
        assert_eq!(v["items"][0]["processes"][1]["pid"], 1);
        assert_eq!(v["items"][0]["local"]["address"], "::");
        assert_eq!(v["items"][1]["processes"], json!([]));
        assert!(sockets("bad").is_err());
    }
    #[test]
    fn ufw() {
        let v=firewall("ufw","Status: active\n\n     To                         Action      From\n     --                         ------      ----\n[ 1] 22/tcp                     ALLOW IN    10.0.0.0/24 # SSH\n[ 2] 80/tcp (v6)                ALLOW IN    Anywhere (v6)\n").unwrap();
        assert_eq!(v["items"][0]["id"], 1);
        assert_eq!(v["items"][1]["source"], "Anywhere (v6)");
        assert!(firewall("ufw", "bad").is_err());
    }
    #[test]
    fn nft_iptables() {
        assert_eq!(firewall("nft",r#"{"nftables":[{"rule":{"family":"inet","table":"filter","chain":"input","handle":4,"expr":[{"accept":null}]}}]}"#).unwrap()["items"][0]["rule"]["handle"],4);
        assert_eq!(
            firewall("iptables", "-P INPUT ACCEPT\n-A INPUT -p tcp --dport 22 -j ACCEPT\n").unwrap()["items"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(firewall("nft", "{}").is_err());
    }
    #[test]
    fn iptables_save_tables() {
        let v=firewall("iptables", "# Generated by iptables-save v1.8.9\n*filter\n:INPUT ACCEPT [10:400]\n-A INPUT -p tcp -m tcp --dport 22 -j ACCEPT\nCOMMIT\n*nat\n:PREROUTING ACCEPT [0:0]\nCOMMIT\n").unwrap();
        assert_eq!(v["items"][2]["table"], "nat");
        assert!(firewall("iptables", "*\n").is_err());
    }
}
