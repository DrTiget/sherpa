//! Чистые разборщики fail2ban, OpenSSH, ss, apt и who.
use crate::error::{CoreError, CoreResult};
use serde_json::{json, Value};
fn bad() -> CoreError {
    CoreError::new("remote.bad_output", "Unexpected security command output")
}
pub fn jails(text: &str) -> CoreResult<Value> {
    let list = text
        .lines()
        .find_map(|l| l.split_once("Jail list:").map(|(_, s)| s))
        .ok_or_else(bad)?;
    Ok(json!({"items":list.split(',').map(str::trim).filter(|s|!s.is_empty()).collect::<Vec<_>>()}))
}
pub fn bans(text: &str) -> CoreResult<Value> {
    let list = text
        .lines()
        .find_map(|l| l.split_once("Banned IP list:").map(|(_, s)| s))
        .ok_or_else(bad)?;
    let items = list
        .split_whitespace()
        .map(|s| {
            s.parse::<std::net::IpAddr>()
                .map(|ip| ip.to_string())
                .map_err(|_| bad())
        })
        .collect::<CoreResult<Vec<_>>>()?;
    Ok(json!({"items":items}))
}
pub fn logins(text: &str) -> CoreResult<Value> {
    let mut items = Vec::new();
    for line in text.lines() {
        let message = if line.starts_with('{') {
            let v: Value = serde_json::from_str(line).map_err(|_| bad())?;
            v["MESSAGE"].as_str().ok_or_else(bad)?.to_owned()
        } else {
            line.to_owned()
        };
        if message.contains("Failed password")
            || message.contains("Invalid user")
            || message.contains("authentication failure")
        {
            let words: Vec<_> = message.split_whitespace().collect();
            let ip = words
                .windows(2)
                .find(|w| w[0] == "from")
                .and_then(|w| w[1].parse::<std::net::IpAddr>().ok())
                .map(|i| i.to_string());
            items.push(json!({"message":message,"ip":ip}));
        }
    }
    Ok(json!({"items":items}))
}
pub fn keys(text: &str) -> CoreResult<Value> {
    let mut items = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let words: Vec<_> = trimmed.split_whitespace().collect();
        let idx = words
            .iter()
            .position(|w| w.starts_with("ssh-") || w.starts_with("ecdsa-"))
            .ok_or_else(bad)?;
        let data = words.get(idx + 1).ok_or_else(bad)?;
        let public =
            russh::keys::ssh_key::PublicKey::from_openssh(&format!("{} {}", words[idx], data)).map_err(|_| bad())?;
        let bits = public.key_data().rsa().map(|r| r.key_size());
        let weak = words[idx] == "ssh-dss" || bits.is_some_and(|n| n < 2048);
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|_| bad())?;
        use sha2::{Digest, Sha256};
        let fingerprint = format!(
            "SHA256:{}",
            base64::engine::general_purpose::STANDARD_NO_PAD.encode(Sha256::digest(bytes))
        );
        items.push(json!({"line":i+1,"key_type":words[idx],"public_key":format!("{} {}",words[idx],data),"comment":words[idx+2..].join(" "),"fingerprint":fingerprint,"weak":weak,"bits":bits}));
    }
    Ok(json!({"items":items}))
}
pub fn ports(text: &str) -> CoreResult<Value> {
    let mut items = Vec::new();
    for line in text.lines() {
        let w: Vec<_> = line.split_whitespace().collect();
        if w.is_empty() {
            continue;
        }
        if w.len() < 5 || !matches!(w[0], "tcp" | "udp") {
            return Err(bad());
        }
        let (address, port) = w[4].rsplit_once(':').ok_or_else(bad)?;
        let port: u16 = port.parse().map_err(|_| bad())?;
        let address = address.trim_matches(['[', ']']);
        if address.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback()) {
            continue;
        }
        items.push(json!({"protocol":w[0],"state":w[1],"address":address,"port":port,"process":w.get(6..).unwrap_or(&[]).join(" "),"risk":if matches!(port,3306|5432|6379|27017){"high"}else{"review"}}));
    }
    Ok(json!({"items":items,"scope":"non_loopback_listeners","firewall_verified":false}))
}
pub fn updates(text: &str) -> Value {
    json!({"items":text.lines().filter(|l|l.contains("-security")&&l.contains("upgradable")).filter_map(|l|{let (name,rest)=l.split_once('/')?;Some(json!({"name":name,"details":rest}))}).collect::<Vec<_>>()})
}
pub fn sessions(text: &str) -> CoreResult<Value> {
    let mut items = Vec::new();
    for l in text.lines() {
        let w: Vec<_> = l.split_whitespace().collect();
        if w.len() < 4 {
            return Err(bad());
        }
        items.push(json!({"user":w[0],"terminal":w[1],"login_at":format!("{} {}",w[2],w[3]),"source":w.get(4).map(|s|s.trim_matches(['(',')']))}));
    }
    Ok(json!({"items":items}))
}
pub fn score(jails: &Value, ports: &Value, updates: &Value, keys: &Value) -> Value {
    let count = |v: &Value| v["items"].as_array().map_or(0, Vec::len);
    let weak = keys["items"]
        .as_array()
        .map_or(0, |a| a.iter().filter(|k| k["weak"] == true).count());
    let high = ports["items"]
        .as_array()
        .map_or(0, |a| a.iter().filter(|p| p["risk"] == "high").count());
    let penalty =
        usize::from(count(jails) == 0) * 20 + count(updates).min(10) * 3 + weak.min(2) * 15 + high.min(3) * 10;
    json!({"score":100usize.saturating_sub(penalty),"checks":{"fail2ban_jails":count(jails),"security_updates":count(updates),"weak_keys":weak,"high_risk_listeners":high},"recommendations":{"enable_fail2ban":count(jails)==0,"install_security_updates":count(updates)>0,"replace_weak_keys":weak>0,"review_listeners":high>0}})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fail2ban() {
        assert_eq!(
            jails("Status\n|- Number of jail: 1\n`- Jail list: sshd\n").unwrap()["items"][0],
            "sshd"
        );
        assert_eq!(
            bans("Status for the jail: sshd\n`- Banned IP list: 192.0.2.1 2001:db8::1\n").unwrap()["items"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(bans("bad").is_err());
    }
    #[test]
    fn ssh_journal() {
        assert_eq!(
            logins("{\"MESSAGE\":\"Failed password for root from 192.0.2.1 port 22 ssh2\"}\n").unwrap()["items"][0]
                ["ip"],
            "192.0.2.1"
        );
        assert!(logins("{bad").is_err());
    }
    #[test]
    fn ss() {
        let v=ports("tcp LISTEN 0 128 0.0.0.0:22 0.0.0.0:* users:((\"sshd\",pid=712,fd=3))\ntcp LISTEN 0 128 [::1]:5432 [::]:*\n").unwrap();
        assert_eq!(v["items"].as_array().unwrap().len(), 1);
        assert!(ports("bad").is_err());
    }
    #[test]
    fn authorized_keys() {
        let v = keys(
            "# comment\nssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEj4ldCoz6NwS7cHHY4qTPXWwD5y1fVdsN7ez0Y0H1E3 deploy@web\n",
        )
        .unwrap();
        assert_eq!(v["items"][0]["line"], 2);
        assert!(keys("invalid").is_err());
    }
    #[test]
    fn apt_and_who() {
        assert_eq!(
            updates("Listing...\nopenssl/bookworm-security 3.0 amd64 [upgradable from: 3.0]\n")["items"][0]["name"],
            "openssl"
        );
        assert_eq!(
            sessions("root pts/0 2026-10-24 12:00 (192.0.2.1)\n").unwrap()["items"][0]["user"],
            "root"
        );
    }
}
