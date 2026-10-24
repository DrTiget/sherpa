//! Проверка против настоящего OpenSSH sshd (без root, на случайном порту).
//! Если sshd нет или он не стартует — тест пропускается с сообщением.
#![cfg(unix)]

mod common;

use std::io::Read;
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use common::Env;
use serde_json::json;

struct Sshd(Child);

impl Drop for Sshd {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn find_sshd() -> Option<String> {
    ["/usr/sbin/sshd", "/usr/bin/sshd", "/sbin/sshd"]
        .iter()
        .find(|p| std::path::Path::new(p).exists())
        .map(|s| s.to_string())
}

fn openssh_pub(private: &str) -> String {
    let k = sherpa_lib::ssh::keys::decode_key(private, None).unwrap();
    k.public_key().to_openssh().unwrap()
}

#[test]
fn real_openssh_server() {
    let Some(sshd) = find_sshd() else {
        eprintln!("SKIP: sshd not found");
        return;
    };
    let e = Env::initialized();
    let dir = e.path("sshd");
    std::fs::create_dir_all(&dir).unwrap();
    let host_key = sherpa_lib::ssh::keys::generate_ed25519_openssh(None).unwrap();
    let client_key = sherpa_lib::ssh::keys::generate_ed25519_openssh(None).unwrap();
    let hk_path = dir.join("host_ed25519");
    std::fs::write(&hk_path, host_key.as_bytes()).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&hk_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::write(dir.join("authorized_keys"), openssh_pub(&client_key) + "\n").unwrap();
    let ck_path = dir.join("client_key");
    std::fs::write(&ck_path, client_key.as_bytes()).unwrap();

    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let cfg = dir.join("sshd_config");
    std::fs::write(
        &cfg,
        format!(
            "Port {port}\nListenAddress 127.0.0.1\nHostKey {hk}\nAuthorizedKeysFile {ak}\nPasswordAuthentication no\n\
             KbdInteractiveAuthentication no\nUsePAM no\nStrictModes no\nPidFile none\n",
            hk = hk_path.display(),
            ak = dir.join("authorized_keys").display()
        ),
    )
    .unwrap();
    let child = match Command::new(&sshd)
        .args(["-D", "-e", "-f"])
        .arg(&cfg)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => Sshd(c),
        Err(err) => {
            eprintln!("SKIP: cannot start sshd: {err}");
            return;
        }
    };
    let mut child = child;
    let start = Instant::now();
    loop {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        if let Ok(Some(_)) = child.0.try_wait() {
            let mut s = String::new();
            let _ = child.0.stderr.take().unwrap().read_to_string(&mut s);
            eprintln!("SKIP: sshd exited: {s}");
            return;
        }
        if start.elapsed() > Duration::from_secs(5) {
            eprintln!("SKIP: sshd did not start");
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    let user = std::env::var("USER").unwrap_or_else(|_| "root".into());
    let body = json!({"name":"openssh","host":"127.0.0.1","port":port,"user":user,
        "auth":{"method":"key_file","key_path":ck_path}});
    let id = e.create(&body.to_string());
    let r = e.ok(&["conn", "check", &id, "--accept-new", "--timeout", "10"]);
    let it = &r["items"][0];
    assert_eq!(it["state"], "online", "{r}");
    assert!(it["server_banner"].as_str().unwrap().contains("OpenSSH"));

    let body = json!({"name":"wrong-user","host":"127.0.0.1","port":port,"user":"definitely-not-a-user",
        "auth":{"method":"key_file","key_path":ck_path}});
    let bad = e.create(&body.to_string());
    let r = e.ok(&["conn", "check", &bad, "--timeout", "10"]);
    assert_eq!(r["items"][0]["state"], "auth_failed", "{r}");

    let body = json!({"name":"pw","host":"127.0.0.1","port":port,"user":user,
        "auth":{"method":"password","password":"x"}});
    let pw = e.create(&body.to_string());
    let r = e.ok(&["conn", "check", &pw, "--timeout", "10"]);
    assert_eq!(r["items"][0]["error"]["code"], "auth.method_not_allowed", "{r}");
}
