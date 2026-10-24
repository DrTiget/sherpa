//! Сквозной контракт svc/pkg/sys: подтверждения, readonly, потоковые события.
mod common;
use base64::{engine::general_purpose::STANDARD, Engine};
use common::Env;
use serde_json::json;
use sherpa_lib::daemon::client::Client;
use sherpa_lib::testserver::{self, Options, PASSWORD, USER};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct Daemon(Env);
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.run(&["daemon", "stop", "--force"]);
    }
}

#[tokio::test]
async fn system_methods_confirmations_readonly_and_package_output() {
    let calls = Arc::new(Mutex::new(Vec::<String>::new()));
    let observed = calls.clone();
    let srv = testserver::start(Options {
        exec: Some(Arc::new(move |cmd| {
            observed.lock().unwrap().push(cmd.into());
            let text = if cmd.contains("/etc/os-release") {
                "os_id=debian\nuid=0\nuser=root\nsystemd=1\ntool_systemctl=1\ntool_apt-get=1\n"
            } else if cmd.starts_with("id -u;") {
                "0\n"
            } else if cmd.ends_with("hostname") {
                "test-server\n"
            } else if cmd.contains("systemctl show") {
                "Id=nginx.service\nLoadState=loaded\nActiveState=active\n"
            } else if cmd.contains("apt-get -y") {
                "Reading package lists...\nSetting up nginx\n"
            } else if cmd.contains("dpkg-query") {
                "nginx\t1.22.1-9\tamd64\tinstalled\n"
            } else if cmd.contains("hostname && uname") {
                "test-server\nLinux\n6.1\nx86_64\n"
            } else {
                ""
            };
            (text.to_string(), 0)
        })),
        ..Default::default()
    });
    let d = Daemon(Env::with_config(
        "daemon_idle_timeout_sec = 60\nconnect_timeout_sec = 5\n",
    ));
    let port = srv.port().to_string();
    d.0.ok(&[
        "hostkey",
        "accept",
        "--host",
        "127.0.0.1",
        "--port",
        &port,
        "--key-type",
        "ssh-ed25519",
        "--fingerprint",
        &srv.fingerprint(),
    ]);
    let id=d.0.create(&json!({"name":"system-test","host":"127.0.0.1","port":srv.port(),"user":USER,"auth":{"method":"password","password":PASSWORD}}).to_string());
    let daemon = d.0.ok(&["daemon", "ensure"]);
    let mut client = Client::connect(std::path::Path::new(daemon["endpoint"].as_str().unwrap()))
        .await
        .unwrap();
    let status = client
        .request("svc.status", json!({"connection_id":id,"unit":"nginx.service"}))
        .await
        .unwrap();
    assert_eq!(status["active_state"], "active");
    let mut p = json!({"connection_id":id,"unit":"nginx.service","confirm_hostname":"test-server"});
    assert_eq!(
        client.request("svc.stop", p.clone()).await.unwrap_err().code,
        "danger.confirmation_required"
    );
    let ticket = client
        .request("danger.prepare", json!({"method":"svc.stop","params":p}))
        .await
        .unwrap();
    p["confirm_token"] = ticket["confirm_token"].clone();
    let mut wrong = p.clone();
    wrong["unit"] = "ssh.service".into();
    assert_eq!(
        client.request("svc.stop", wrong).await.unwrap_err().code,
        "danger.confirmation_required"
    );
    client
        .request("priv.readonly.set", json!({"connection_id":id,"readonly":true}))
        .await
        .unwrap();
    assert_eq!(
        client.request("svc.stop", p.clone()).await.unwrap_err().code,
        "priv.readonly"
    );
    assert_eq!(
        client
            .request("sys.user.add", json!({"connection_id":id,"name":"deploy"}))
            .await
            .unwrap_err()
            .code,
        "priv.readonly"
    );
    assert!(client
        .request("svc.status", json!({"connection_id":id,"unit":"nginx.service"}))
        .await
        .is_ok());
    client
        .request("priv.readonly.set", json!({"connection_id":id,"readonly":false}))
        .await
        .unwrap();
    assert_eq!(client.request("svc.stop", p.clone()).await.unwrap()["changed"], true);
    assert_eq!(
        client.request("svc.stop", p).await.unwrap_err().code,
        "danger.confirmation_required"
    );
    let mut p = json!({"connection_id":id,"action":"reboot","confirm_hostname":"wrong-server"});
    let t = client
        .request("danger.prepare", json!({"method":"sys.power","params":p}))
        .await
        .unwrap();
    p["confirm_token"] = t["confirm_token"].clone();
    assert_eq!(
        client.request("sys.power", p).await.unwrap_err().code,
        "danger.hostname_mismatch"
    );
    assert_eq!(
        client
            .request("pkg.installed", json!({"connection_id":id}))
            .await
            .unwrap()["items"][0]["name"],
        "nginx"
    );
    let mut p = json!({"connection_id":id,"packages":["nginx"]});
    let t = client
        .request("danger.prepare", json!({"method":"pkg.install","params":p}))
        .await
        .unwrap();
    p["confirm_token"] = t["confirm_token"].clone();
    let operation = client.request("pkg.install", p).await.unwrap()["operation"].clone();
    let mut output = Vec::new();
    loop {
        let ev = client.next_event(Duration::from_secs(10)).await.unwrap();
        assert_ne!(ev["event"], "error", "{ev}");
        if ev["event"] == "pkg.output" {
            assert_eq!(ev["operation"], operation);
            output.extend(STANDARD.decode(ev["data_b64"].as_str().unwrap()).unwrap());
        }
        if ev["event"] == "pkg.done" {
            assert_eq!(ev["operation"], operation);
            break;
        }
    }
    assert!(String::from_utf8(output).unwrap().contains("Setting up nginx"));
    let commands = calls.lock().unwrap();
    assert_eq!(
        commands
            .iter()
            .filter(|c| c.contains("systemctl --no-ask-password stop"))
            .count(),
        1
    );
    assert!(!commands.iter().any(|c| c.contains("shutdown -r")));
}
