//! Реальные текстовые форматы dpkg/apt, rpm/dnf/yum, pacman и apk.
use crate::error::CoreResult;
use crate::utilities::management::bad;
use serde_json::{json, Value};

fn apk_package(token: &str) -> CoreResult<(&str, &str)> {
    let offset = token
        .as_bytes()
        .windows(2)
        .rposition(|w| w[0] == b'-' && w[1].is_ascii_digit())
        .ok_or_else(|| bad("apk"))?;
    Ok((&token[..offset], &token[offset + 1..]))
}
fn entry(
    name: &str,
    version: Option<&str>,
    arch: Option<&str>,
    description: Option<&str>,
    installed: Option<&str>,
) -> Value {
    json!({"name":name,"version":version,"arch":arch,"description":description,"installed_version":installed})
}
pub fn packages(manager: &str, kind: &str, text: &str) -> CoreResult<Value> {
    let mut items = Vec::new();
    let mut pending: Option<usize> = None;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let s = line.trim();
        let f: Vec<_> = s.split_whitespace().collect();
        match (manager, kind) {
            ("apt", "installed") => {
                let f: Vec<_> = line.split('\t').collect();
                if f.len() != 4 {
                    return Err(bad("dpkg-query"));
                }
                if f[3] == "installed" {
                    items.push(entry(f[0], Some(f[1]), Some(f[2]), None, None));
                }
            }
            ("apt", "search") => {
                let (n, d) = s.split_once(" - ").ok_or_else(|| bad("apt-cache"))?;
                items.push(entry(n, None, None, Some(d), None));
            }
            ("apt", "updates") => {
                if s.starts_with("Listing...") {
                    continue;
                }
                let (n, _) = f
                    .first()
                    .and_then(|s| s.split_once('/'))
                    .ok_or_else(|| bad("apt list"))?;
                if f.len() < 3 {
                    return Err(bad("apt list"));
                }
                let old = s
                    .split_once("[upgradable from: ")
                    .and_then(|(_, s)| s.strip_suffix(']'));
                items.push(entry(n, Some(f[1]), Some(f[2]), None, old));
            }
            ("dnf" | "yum", "installed") => {
                let f: Vec<_> = line.split('\t').collect();
                if f.len() != 3 {
                    return Err(bad("rpm"));
                }
                items.push(entry(f[0], Some(f[1]), Some(f[2]), None, None));
            }
            ("dnf" | "yum", "search") => {
                if s.starts_with('=')
                    || s.starts_with("Last metadata expiration check:")
                    || s.starts_with("Loaded plugins:")
                    || s.starts_with("No matches found")
                {
                    continue;
                }
                let (n, d) = s.split_once(" : ").ok_or_else(|| bad("dnf search"))?;
                let (n, a) = n.rsplit_once('.').ok_or_else(|| bad("dnf search"))?;
                items.push(entry(n, None, Some(a), Some(d), None));
            }
            ("dnf" | "yum", "updates") => {
                if s.starts_with("Last metadata expiration check:")
                    || s.starts_with("Loaded plugins:")
                    || s == "Obsoleting Packages"
                {
                    continue;
                }
                if f.len() != 3 {
                    return Err(bad("dnf check-update"));
                }
                let (n, a) = f[0].rsplit_once('.').ok_or_else(|| bad("dnf check-update"))?;
                items.push(entry(n, Some(f[1]), Some(a), None, None));
            }
            ("pacman", "installed") => {
                if f.len() != 2 {
                    return Err(bad("pacman -Q"));
                }
                items.push(entry(f[0], Some(f[1]), None, None, None));
            }
            ("pacman", "updates") => {
                if f.len() != 4 || f[2] != "->" {
                    return Err(bad("pacman -Qu"));
                }
                items.push(entry(f[0], Some(f[3]), None, None, Some(f[1])));
            }
            ("pacman", "search") => {
                if line.starts_with(char::is_whitespace) {
                    let i = pending.take().ok_or_else(|| bad("pacman -Ss"))?;
                    items[i]["description"] = s.into();
                } else {
                    if f.len() < 2 {
                        return Err(bad("pacman -Ss"));
                    }
                    let (_, n) = f[0].split_once('/').ok_or_else(|| bad("pacman -Ss"))?;
                    items.push(entry(n, Some(f[1]), None, None, None));
                    pending = Some(items.len() - 1);
                }
            }
            ("apk", "installed" | "search") => {
                let (n, v) = apk_package(s)?;
                items.push(entry(n, Some(v), None, None, None));
            }
            ("apk", "updates") => {
                if s.starts_with("WARNING:") {
                    continue;
                }
                if f.len() < 3 {
                    return Err(bad("apk list"));
                }
                let (n, v) = apk_package(f[0])?;
                let old = s
                    .split_once("[upgradable from: ")
                    .and_then(|(_, s)| s.strip_suffix(']'));
                if old.is_none() {
                    return Err(bad("apk list"));
                }
                items.push(entry(n, Some(v), Some(f[1]), None, old));
            }
            _ => return Err(bad("package manager")),
        }
    }
    Ok(json!({"manager":manager,"items":items}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn debian() {
        let v = packages(
            "apt",
            "installed",
            "bash\t5.2.15-2+b7\tamd64\tinstalled\nold\t1\tall\tconfig-files\n",
        )
        .unwrap();
        assert_eq!(v["items"].as_array().unwrap().len(), 1);
        assert_eq!(
            packages("apt", "search", "nginx - small, powerful, scalable web/proxy server\n").unwrap()["items"][0]
                ["name"],
            "nginx"
        );
        assert_eq!(
            packages(
                "apt",
                "updates",
                "Listing...\nopenssl/bookworm-security 3.0.15-1~deb12u1 amd64 [upgradable from: 3.0.14-1~deb12u2]\n"
            )
            .unwrap()["items"][0]["installed_version"],
            "3.0.14-1~deb12u2"
        );
    }
    #[test]
    fn rpm() {
        for m in ["dnf", "yum"] {
            assert_eq!(
                packages(m, "installed", "bash\t5.1.8-9.el9\tx86_64\n").unwrap()["items"][0]["arch"],
                "x86_64"
            );
            assert_eq!(packages(m,"search","===== Name Exactly Matched: nginx =====\nnginx.x86_64 : A high performance web server and reverse proxy server\n").unwrap()["items"][0]["name"],"nginx");
            assert_eq!(
                packages(m, "updates", "\nopenssl.x86_64   1:3.2.2-6.fc40   updates\n").unwrap()["items"][0]["version"],
                "1:3.2.2-6.fc40"
            );
        }
    }
    #[test]
    fn arch() {
        assert_eq!(
            packages("pacman", "installed", "bash 5.2.037-1\n").unwrap()["items"][0]["name"],
            "bash"
        );
        assert_eq!(
            packages(
                "pacman",
                "search",
                "extra/nginx 1.26.2-1 [installed]\n    Lightweight HTTP server and IMAP/POP3 proxy server\n"
            )
            .unwrap()["items"][0]["description"],
            "Lightweight HTTP server and IMAP/POP3 proxy server"
        );
        assert_eq!(
            packages("pacman", "updates", "bash 5.2.032-1 -> 5.2.037-1\n").unwrap()["items"][0]["installed_version"],
            "5.2.032-1"
        );
    }
    #[test]
    fn alpine() {
        assert_eq!(
            packages("apk", "installed", "alpine-baselayout-3.6.5-r0\n").unwrap()["items"][0]["name"],
            "alpine-baselayout"
        );
        assert_eq!(
            packages("apk", "search", "nginx-1.26.2-r0\n").unwrap()["items"][0]["version"],
            "1.26.2-r0"
        );
        assert_eq!(
            packages(
                "apk",
                "updates",
                "nginx-1.26.2-r0 x86_64 {nginx} (BSD-2-Clause) [upgradable from: 1.26.1-r0]\n"
            )
            .unwrap()["items"][0]["installed_version"],
            "1.26.1-r0"
        );
    }
    #[test]
    fn malformed() {
        for m in ["apt", "dnf", "yum", "pacman", "apk"] {
            for k in ["installed", "search", "updates"] {
                assert!(packages(m, k, "garbage").is_err(), "{m} {k}");
                assert_eq!(packages(m, k, "").unwrap()["items"], json!([]));
            }
        }
    }
}
