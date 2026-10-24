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
fn files_transfers_and_tunnels() {
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
             Subsystem sftp internal-sftp\nAllowTcpForwarding yes\nKbdInteractiveAuthentication no\nUsePAM no\nStrictModes no\nPidFile none\n",
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

    tokio::runtime::Runtime::new().unwrap().block_on(exercise(&e, &id));
}

use serde_json::Value;
use sherpa_lib::{
    config::{Overrides, Settings},
    daemon::{
        registry::{dispatch, Cx},
        state::{ClientHandle, DaemonState},
    },
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
async fn call(cx: &Cx, method: &str, p: Value) -> Value {
    dispatch(cx, method, p)
        .await
        .unwrap_or_else(|e| panic!("{method}: {e:?}"))
}
async fn wait_job(cx: &Cx, kind: &str, id: &str, wanted: &str) -> Value {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let all = call(cx, &format!("{kind}.list"), json!({})).await;
            let data = all["items"].as_array().unwrap().iter().find(|v| v["id"] == id).unwrap()["data"].clone();
            if data["status"] == wanted {
                return data;
            }
            assert_ne!(data["status"], "failed", "{data}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("job timed out")
}
async fn exercise(e: &Env, id: &str) {
    let settings = Settings::load(&Overrides {
        config: Some(e.path("core.toml")),
        db: Some(e.path("sherpa.db")),
        key_file: Some(e.path("sherpa.key")),
    })
    .unwrap();
    let state = DaemonState::new(settings.clone(), sherpa_lib::daemon::paths(&settings));
    let (tx, _rx) = tokio::sync::mpsc::channel(32);
    let cx = Cx {
        state: state.clone(),
        client: ClientHandle { id: 1, tx },
    };
    let base = e.path("remote");
    std::fs::create_dir(&base).unwrap();
    let f = base.join("editor.txt");
    std::fs::write(&f, "hello").unwrap();
    let read = call(&cx, "files.read", json!({"connection_id":id,"path":f})).await;
    assert_eq!(read["text"], "hello");
    let bad = dispatch(
        &cx,
        "files.write",
        json!({"connection_id":id,"path":f,"text":"no","mtime":0}),
    )
    .await
    .unwrap_err();
    assert_eq!(bad.code, "fs.conflict");
    call(
        &cx,
        "files.write",
        json!({"connection_id":id,"path":f,"text":"Привет","mtime":read["mtime"]}),
    )
    .await;
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "Привет");
    call(&cx, "files.chmod", json!({"connection_id":id,"path":f,"mode":384})).await;
    let meta = call(&cx, "files.stat", json!({"connection_id":id,"path":f})).await;
    assert_eq!(meta["permissions"].as_u64().unwrap() & 511, 384);
    assert!(meta["uid"].is_number());
    assert!(meta["mtime"].is_number());
    let renamed = base.join("renamed.txt");
    call(&cx, "files.rename", json!({"connection_id":id,"path":f,"to":renamed})).await;
    assert!(renamed.exists());
    let found = call(
        &cx,
        "files.search",
        json!({"connection_id":id,"path":base,"query":"renamed"}),
    )
    .await;
    assert_eq!(found["items"].as_array().unwrap().len(), 1);
    let list = call(&cx, "files.list", json!({"connection_id":id,"path":base})).await;
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    let big = base.join("big");
    std::fs::write(&big, vec![b'x'; 131073]).unwrap();
    assert_eq!(
        dispatch(&cx, "files.read", json!({"connection_id":id,"path":big}))
            .await
            .unwrap_err()
            .code,
        "fs.too_large"
    );
    let conn = cx.conn(id).await.unwrap();
    conn.set_readonly(true);
    assert_eq!(
        dispatch(
            &cx,
            "files.mkdir",
            json!({"connection_id":id,"path":base.join("blocked")})
        )
        .await
        .unwrap_err()
        .code,
        "priv.readonly"
    );
    conn.set_readonly(false);
    let tree = e.path("upload");
    std::fs::create_dir(&tree).unwrap();
    std::fs::create_dir(tree.join("sub")).unwrap();
    let bytes: Vec<u8> = (0..300000).map(|i| (i % 251) as u8).collect();
    std::fs::write(tree.join("sub/data"), &bytes).unwrap();
    let remote = base.join("tree");
    let job = call(
        &cx,
        "transfer.start",
        json!({"connection_id":id,"direction":"upload","local":tree,"remote":remote}),
    )
    .await;
    let completed = wait_job(&cx, "transfer", job["id"].as_str().unwrap(), "completed").await;
    assert_eq!(completed["bytes"], 300000);
    assert_eq!(std::fs::read(remote.join("sub/data")).unwrap(), bytes);
    let download = e.path("download");
    let job = call(
        &cx,
        "transfer.start",
        json!({"connection_id":id,"direction":"download","local":download,"remote":remote}),
    )
    .await;
    wait_job(&cx, "transfer", job["id"].as_str().unwrap(), "completed").await;
    assert_eq!(std::fs::read(download.join("sub/data")).unwrap(), bytes);
    let partial = e.path("partial");
    std::fs::write(&partial, &bytes[..12345]).unwrap();
    let job = call(&cx,"transfer.start",json!({"connection_id":id,"direction":"download","local":partial,"remote":remote.join("sub/data"),"resume":true})).await;
    wait_job(&cx, "transfer", job["id"].as_str().unwrap(), "completed").await;
    assert_eq!(std::fs::read(&partial).unwrap(), bytes);
    let partial_upload = base.join("partial-upload");
    std::fs::write(&partial_upload, &bytes[..54321]).unwrap();
    let job = call(&cx,"transfer.start",json!({"connection_id":id,"direction":"upload","local":tree.join("sub/data"),"remote":partial_upload,"resume":true})).await;
    wait_job(&cx, "transfer", job["id"].as_str().unwrap(), "completed").await;
    assert_eq!(std::fs::read(&partial_upload).unwrap(), bytes);
    std::fs::write(&partial, b"bad prefix").unwrap();
    let job = call(&cx,"transfer.start",json!({"connection_id":id,"direction":"download","local":partial,"remote":remote.join("sub/data"),"resume":true})).await;
    let failed = wait_job(&cx, "transfer", job["id"].as_str().unwrap(), "failed").await;
    assert_eq!(failed["error"]["code"], "transfer.conflict");
    // Симлинк не позволяет рекурсивной операции уйти за выбранный каталог.
    let outside = e.path("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("keep"), b"keep").unwrap();
    std::os::unix::fs::symlink(&outside, remote.join("link")).unwrap();
    // Очередь и отмена до открытия канала.
    let permits = state.jobs.transfers.clone().acquire_many_owned(2).await.unwrap();
    let job = call(
        &cx,
        "transfer.start",
        json!({"connection_id":id,"direction":"download","local":e.path("cancel"),"remote":renamed}),
    )
    .await;
    call(&cx, "transfer.cancel", json!({"id":job["id"]})).await;
    wait_job(&cx, "transfer", job["id"].as_str().unwrap(), "cancelled").await;
    drop(permits);
    assert!(!e.path("cancel").exists());
    // Токен нельзя повторить или использовать на другом пути.
    let delete = json!({"connection_id":id,"path":remote,"recursive":true});
    assert_eq!(
        dispatch(&cx, "files.delete", delete.clone()).await.unwrap_err().code,
        "danger.confirmation_required"
    );
    let ticket = call(&cx, "danger.prepare", json!({"method":"files.delete","params":delete})).await;
    let mut confirmed = delete;
    confirmed["confirm_token"] = ticket["confirm_token"].clone();
    call(&cx, "files.delete", confirmed.clone()).await;
    assert!(!remote.exists());
    assert!(outside.join("keep").exists());
    assert_eq!(
        dispatch(&cx, "files.delete", confirmed).await.unwrap_err().code,
        "danger.confirmation_required"
    );
    call(
        &cx,
        "desktop.state.set",
        json!({"connection_id":id,"state":{"windows":[{"app":"files","x":20}]}}),
    )
    .await;
    let second = DaemonState::new(settings.clone(), sherpa_lib::daemon::paths(&settings));
    let other = Cx {
        state: second,
        client: cx.client.clone(),
    };
    assert_eq!(
        call(&other, "desktop.state.get", json!({"connection_id":id})).await["state"]["windows"][0]["x"],
        20
    );
    tunnels(&cx, id).await;
    state.jobs.cancel_all();
    state.pool.close_all().await;
}
async fn tunnels(cx: &Cx, id: &str) {
    let echo = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dest_port = echo.local_addr().unwrap().port();
    let echo_task = tokio::spawn(async move {
        loop {
            let (mut stream, _) = echo.accept().await.unwrap();
            tokio::spawn(async move {
                let (mut r, mut w) = stream.split();
                let _ = tokio::io::copy(&mut r, &mut w).await;
            });
        }
    });
    for kind in ["local", "dynamic", "remote"] {
        let port = if kind == "remote" {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        } else {
            0
        };
        let job = call(
            cx,
            "tunnel.start",
            json!({"connection_id":id,"kind":kind,"bind_port":port,"target_host":"127.0.0.1","target_port":dest_port}),
        )
        .await;
        let tid = job["id"].as_str().unwrap();
        wait_job(cx, "tunnel", tid, "running").await;
        let mut tcp = tokio::net::TcpStream::connect(("127.0.0.1", job["bind_port"].as_u64().unwrap() as u16))
            .await
            .unwrap();
        if kind == "dynamic" {
            tcp.write_all(&[5, 1, 0]).await.unwrap();
            let mut response = [0; 2];
            tcp.read_exact(&mut response).await.unwrap();
            assert_eq!(response, [5, 0]);
            let mut req = vec![5, 1, 0, 1, 127, 0, 0, 1];
            req.extend(dest_port.to_be_bytes());
            tcp.write_all(&req).await.unwrap();
            let mut response = [0; 10];
            tcp.read_exact(&mut response).await.unwrap();
            assert_eq!(response[1], 0);
        }
        tcp.write_all(b"roundtrip").await.unwrap();
        let mut data = [0; 9];
        tokio::time::timeout(Duration::from_secs(5), tcp.read_exact(&mut data))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&data, b"roundtrip");
        call(cx, "tunnel.stop", json!({"id":tid})).await;
        wait_job(cx, "tunnel", tid, "stopped").await;
        let mut b = [0; 1];
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), tcp.read(&mut b))
                .await
                .unwrap()
                .unwrap_or(0),
            0
        );
    }
    echo_task.abort();
}
