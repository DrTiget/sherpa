use crate::{CoreError, CoreResult};
use russh::keys::{HashAlg, PublicKey};
use std::path::PathBuf;

#[derive(Debug, PartialEq)]
pub enum HostCheck {
    Trusted,
    Unknown,
    /// Сохранённый отпечаток отличается от предъявленного.
    Mismatch { expected: String },
}

pub fn fingerprint(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

fn host_id(host: &str, port: u16) -> String {
    if port == 22 { host.to_lowercase() } else { format!("[{}]:{port}", host.to_lowercase()) }
}

/// Собственный known_hosts приложения: строки `<host> <алгоритм> <base64-ключ>`.
pub struct KnownHosts {
    path: PathBuf,
}

impl KnownHosts {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn entries(&self) -> Vec<(String, String)> {
        std::fs::read_to_string(&self.path)
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
            .filter_map(|l| l.split_once(' ').map(|(h, k)| (h.to_string(), k.trim().to_string())))
            .collect()
    }

    pub fn check(&self, host: &str, port: u16, key: &PublicKey) -> HostCheck {
        let id = host_id(host, port);
        let mut stored = false;
        for (h, k) in self.entries().into_iter().filter(|(h, _)| *h == id) {
            let _ = h;
            if let Ok(sk) = PublicKey::from_openssh(&k) {
                if sk.key_data() == key.key_data() {
                    return HostCheck::Trusted;
                }
                // допускаем несколько ключей разных алгоритмов на хост: сравниваем только одинаковый алгоритм
                if sk.algorithm() == key.algorithm() {
                    return HostCheck::Mismatch { expected: fingerprint(&sk) };
                }
                stored = true;
            }
        }
        // есть ключ только другого алгоритма — тоже подозрительно, но для TOFU считаем неизвестным
        let _ = stored;
        HostCheck::Unknown
    }

    pub fn trust(&self, host: &str, port: u16, key: &PublicKey) -> CoreResult<()> {
        if let Some(d) = self.path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let line = key.to_openssh().map_err(|e| CoreError::msg(e.to_string()))?;
        // без комментария
        let mut parts = line.split_whitespace();
        let (alg, b64) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        let mut text = std::fs::read_to_string(&self.path).unwrap_or_default();
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&format!("{} {alg} {b64}\n", host_id(host, port)));
        std::fs::write(&self.path, text)?;
        Ok(())
    }

    /// Забыть хост (например, после намеренной смены ключа).
    pub fn forget(&self, host: &str, port: u16) -> CoreResult<()> {
        let id = host_id(host, port);
        let text: String = std::fs::read_to_string(&self.path)
            .unwrap_or_default()
            .lines()
            .filter(|l| l.split(' ').next() != Some(id.as_str()))
            .map(|l| format!("{l}\n"))
            .collect();
        std::fs::write(&self.path, text)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use russh::keys::PrivateKey;

    fn key(n: u8) -> PublicKey {
        use russh::keys::ssh_key::private::Ed25519Keypair;
        PrivateKey::from(Ed25519Keypair::from_seed(&[n; 32])).public_key().clone()
    }

    #[test]
    fn tofu_flow() {
        let dir = tempfile::tempdir().unwrap();
        let kh = KnownHosts::new(dir.path().join("known_hosts"));
        let (a, b) = (key(1), key(2));
        assert_eq!(kh.check("Host.example", 22, &a), HostCheck::Unknown);
        kh.trust("Host.example", 22, &a).unwrap();
        assert_eq!(kh.check("host.example", 22, &a), HostCheck::Trusted);
        assert_eq!(kh.check("host.example", 22, &b), HostCheck::Mismatch { expected: fingerprint(&a) });
        // другой порт — отдельный хост
        assert_eq!(kh.check("host.example", 2222, &b), HostCheck::Unknown);
        kh.forget("host.example", 22).unwrap();
        assert_eq!(kh.check("host.example", 22, &b), HostCheck::Unknown);
    }
}
