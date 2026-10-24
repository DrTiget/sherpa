//! Демон: жизненный цикл, PTY-сессии с вопросами, attach, backpressure,
//! соединение рабочего стола, monitor, priv. SSH — встроенный тестовый сервер.

mod common;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use common::Env;
use serde_json::{json, Value};
use sherpa_lib::daemon::client::Client;
use sherpa_lib::testserver::{self, Options, PASSWORD, USER};

const T: Duration = Duration::from_secs(10);

/// Окружение с демоном; при удалении демон останавливается.
struct DEnv {
    env: Env,
    endpoint: PathBuf,
}

impl DEnv {
    fn new(idle_sec: u64) -> Self {
        let env = Env::with_config(&format!(
            "daemon_idle_timeout_sec = {idle_sec}\nconnect_timeout_sec = 5\n"
        ));
        let d = env.ok(&["daemon", "ensure"]);
        assert_eq!(d["started"], true, "{d}");
        let endpoint = PathBuf::from(d["endpoint"].as_str().unwrap());
        DEnv { env, endpoint }
    }

    async fn client(&self) -> Client {
        Client::connect(&self.endpoint).await.unwrap()
    }
}

impl Drop for DEnv {
    fn drop(&mut self) {
        let _ = self.env.run(&["daemon", "stop", "--force"]);
    }
}

fn b64(s: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(s)
}

fn decode(ev: &Value) -> String {
    String::from_utf8_lossy(
        &base64::engine::general_purpose::STANDARD
            .decode(ev["data_b64"].as_str().unwrap())
            .unwrap(),
    )
    .into_owned()
}

/// Читать вывод сессии, пока не встретится `needle`.
async fn read_until(c: &mut Client, session: &str, needle: &str) -> String {
    let mut acc = String::new();
    let deadline = tokio::time::Instant::now() + T;
    while !acc.contains(needle) {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        let ev = c
            .next_event(left)
            .await
            .unwrap_or_else(|| panic!("no '{needle}' in output: {acc:?}"));
        if ev["event"] == "output" && ev["session"] == session {
            acc += &decode(&ev);
        } else if ev["event"] == "closed" || ev["event"] == "error" {
            panic!("session ended while waiting for {needle}: {ev}");
        }
    }
    acc
}

fn conn(e: &Env, name: &str, port: u16, password: Option<&str>) -> String {
    let mut auth = json!({"method": "password"});
    if let Some(p) = password {
        auth["password"] = p.into();
    }
    e.create(&json!({"name": name, "host": "127.0.0.1", "port": port, "user": USER, "auth": auth}).to_string())
}

#[test]
fn lifecycle_ensure_status_stop() {
    let env = Env::with_config("daemon_idle_timeout_sec = 60\n");
    assert_eq!(env.ok(&["daemon", "status"])["running"], false);
    assert_eq!(env.err(&["daemon", "stop"]).0, "daemon.not_running");
    let d = env.ok(&["daemon", "ensure"]);
    assert_eq!(d["started"], true);
    assert_eq!(d["protocol"], 1);
    let d2 = env.ok(&["daemon", "ensure"]);
    assert_eq!(d2["started"], false);
    assert_eq!(d2["pid"], d["pid"]);
    let st = env.ok(&["daemon", "status"]);
    assert_eq!(st["running"], true);
    assert_eq!(st["sessions"], 0);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(st["endpoint"].as_str().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
    // Второй экземпляр не стартует.
    let (code, params, _) = env.err(&["daemon", "run"]);
    assert_eq!(code, "daemon.already_running");
    assert_eq!(params["pid"], d["pid"]);
    let s = env.ok(&["daemon", "stop"]);
    assert_eq!(s["stopped"], true);
    assert_eq!(env.ok(&["daemon", "status"])["running"], false);
}

#[test]
fn exits_when_idle() {
    let env = Env::with_config("daemon_idle_timeout_sec = 1\n");
    env.ok(&["daemon", "ensure"]);
    // Любой клиент (и `status`) продлевает жизнь демону — ждём без опроса.
    std::thread::sleep(Duration::from_millis(3000));
    assert_eq!(
        env.ok(&["daemon", "status"])["running"],
        false,
        "daemon did not exit when idle"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn protocol_basics() {
    let d = DEnv::new(60);
    let mut raw = sherpa_lib::daemon::transport::connect(&d.endpoint).await.unwrap();
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    raw.write_all(b"{\"id\":1,\"method\":\"ping\"}\n").await.unwrap();
    let mut r = BufReader::new(raw);
    let mut line = String::new();
    r.read_line(&mut line).await.unwrap();
    let v: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(v["error"]["code"], "protocol.hello_required");

    let mut c = d.client().await;
    assert_eq!(c.hello["protocol"], 1);
    assert_eq!(c.request("ping", json!({})).await.unwrap()["pong"], true);
    assert_eq!(
        c.request("nope.x", json!({})).await.unwrap_err().code,
        "protocol.unknown_method"
    );
    let m = c.request("daemon.methods", json!({})).await.unwrap();
    assert!(m["items"].as_array().unwrap().iter().any(|x| x == "monitor.subscribe"));
    let e = c
        .request("session.open", json!({"connection_id": "missing"}))
        .await
        .unwrap_err();
    assert_eq!(e.code, "conn.not_found");
    let e = c
        .request("session.input", json!({"session": "s999", "data_b64": ""}))
        .await
        .unwrap_err();
    assert_eq!(e.code, "session.not_found");
    let mut c2 = Client::connect(&d.endpoint).await.unwrap();
    let e = c2.request("hello", json!({"protocol": 99})).await.unwrap_err();
    assert_eq!(e.code, "protocol.unsupported");
}

#[tokio::test(flavor = "multi_thread")]
async fn terminal_with_prompts_resize_close() {
    let srv = testserver::start(Options::default());
    let d = DEnv::new(60);
    let id = conn(&d.env, "srv", srv.port(), None);
    let mut c = d.client().await;

    let r = c
        .request("session.open", json!({"connection_id": id, "cols": 100, "rows": 30}))
        .await
        .unwrap();
    let s = r["session"].as_str().unwrap().to_string();
    assert_eq!(c.wait_event("connecting", T).await.unwrap()["session"], s.as_str());

    // TOFU: неизвестный ключ → вопрос → принять.
    let hk = c.wait_event("hostkey_prompt", T).await.unwrap();
    assert_eq!(hk["kind"], "unknown");
    assert_eq!(hk["fingerprint"], srv.fingerprint().as_str());
    let e = c
        .request(
            "session.hostkey_answer",
            json!({"session": s, "prompt_id": "p0", "decision": "accept"}),
        )
        .await
        .unwrap_err();
    assert_eq!(e.code, "session.prompt_expired");
    c.request(
        "session.hostkey_answer",
        json!({"session": s, "prompt_id": hk["prompt_id"], "decision": "accept"}),
    )
    .await
    .unwrap();

    // Пароль не сохранён → вопрос; неверный → повтор; верный с сохранением.
    let a1 = c.wait_event("auth_prompt", T).await.unwrap();
    assert_eq!(
        (a1["kind"].as_str(), a1["attempt"].as_u64()),
        (Some("password"), Some(1))
    );
    c.request(
        "session.auth_answer",
        json!({"session": s, "prompt_id": a1["prompt_id"], "responses": ["wrong"]}),
    )
    .await
    .unwrap();
    let a2 = c.wait_event("auth_prompt", T).await.unwrap();
    assert_eq!(a2["attempt"], 2);
    c.request(
        "session.auth_answer",
        json!({"session": s, "prompt_id": a2["prompt_id"], "responses": [PASSWORD], "save": true}),
    )
    .await
    .unwrap();
    let ok = c.wait_event("connected", T).await.unwrap();
    assert!(ok["server_banner"].as_str().unwrap().starts_with("SSH-2.0-"));
    read_until(&mut c, &s, "READY 100x30").await;

    c.request("session.input", json!({"session": s, "data_b64": b64("hello\r")}))
        .await
        .unwrap();
    read_until(&mut c, &s, "hello").await;
    c.request("session.resize", json!({"session": s, "cols": 120, "rows": 40}))
        .await
        .unwrap();
    read_until(&mut c, &s, "RESIZE 120x40").await;
    let list = c.request("session.list", json!({})).await.unwrap();
    assert_eq!(list["items"][0]["state"], "connected");
    assert_eq!(list["items"][0]["cols"], 120);

    // Активная сессия — остановка без --force отклоняется.
    assert_eq!(d.env.err(&["daemon", "stop"]).0, "daemon.sessions_active");

    c.request("session.close", json!({"session": s})).await.unwrap();
    let closed = c.wait_event("closed", T).await.unwrap();
    assert_eq!(closed["reason"], "client");

    // Пароль и ключ сохранились, статус — online.
    let got = d.env.ok(&["conn", "get", &id]);
    assert_eq!(got["connection"]["auth"]["has_password"], true);
    assert_eq!(got["connection"]["status"]["state"], "online");
    let hk = d.env.ok(&["hostkey", "list"]);
    assert_eq!(hk["items"][0]["fingerprint"], srv.fingerprint().as_str());
}

#[tokio::test(flavor = "multi_thread")]
async fn auth_cancel_and_hostkey_reject() {
    let srv = testserver::start(Options::default());
    let d = DEnv::new(60);
    let id = conn(&d.env, "srv", srv.port(), None);
    let mut c = d.client().await;

    let s = c.request("session.open", json!({"connection_id": id})).await.unwrap()["session"].clone();
    let hk = c.wait_event("hostkey_prompt", T).await.unwrap();
    c.request(
        "session.hostkey_answer",
        json!({"prompt_id": hk["prompt_id"], "decision": "accept_once"}),
    )
    .await
    .unwrap();
    let a = c.wait_event("auth_prompt", T).await.unwrap();
    c.request(
        "session.auth_answer",
        json!({"session": s, "prompt_id": a["prompt_id"], "cancel": true}),
    )
    .await
    .unwrap();
    let err = c.wait_event("error", T).await.unwrap();
    assert_eq!(err["error"]["code"], "auth.cancelled");
    assert_eq!(err["fatal"], true);
    assert_eq!(c.wait_event("closed", T).await.unwrap()["reason"], "error");
    // accept_once не сохраняет ключ.
    assert_eq!(d.env.ok(&["hostkey", "list"])["items"], json!([]));

    c.request("session.open", json!({"connection_id": id})).await.unwrap();
    let hk = c.wait_event("hostkey_prompt", T).await.unwrap();
    c.request(
        "session.hostkey_answer",
        json!({"prompt_id": hk["prompt_id"], "decision": "reject"}),
    )
    .await
    .unwrap();
    let err = c.wait_event("error", T).await.unwrap();
    assert_eq!(err["error"]["code"], "hostkey.unknown");
    let hk = d.env.ok(&["hostkey", "list"]);
    assert_eq!(hk["items"][0]["state"], "rejected");
}

#[tokio::test(flavor = "multi_thread")]
async fn attach_after_disconnect_and_exit() {
    let srv = testserver::start(Options::default());
    let d = DEnv::new(60);
    let id = conn(&d.env, "srv", srv.port(), Some(PASSWORD));
    d.env.ok(&["conn", "check", &id, "--accept-new"]);

    let s;
    {
        let mut c1 = d.client().await;
        s = c1.request("session.open", json!({"connection_id": id})).await.unwrap()["session"]
            .as_str()
            .unwrap()
            .to_string();
        c1.wait_event("connected", T).await.unwrap();
        read_until(&mut c1, &s, "READY").await;
    } // c1 отключился — сессия ждёт attach.

    let mut c2 = d.client().await;
    let mut attached = false;
    for _ in 0..50 {
        let l = c2.request("session.list", json!({})).await.unwrap();
        if l["items"][0]["attached"] == false {
            attached = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(attached, "session did not become detached");
    let e = c2
        .request("session.input", json!({"session": s, "data_b64": b64("x")}))
        .await
        .unwrap_err();
    assert_eq!(e.code, "session.not_found", "not owned before attach");
    let r = c2.request("session.attach", json!({"session": s})).await.unwrap();
    assert!(r["replay_bytes"].as_u64().unwrap() > 0);
    let replay = c2.wait_event("output", T).await.unwrap();
    assert_eq!(replay["replay"], true);
    assert!(decode(&replay).contains("READY"));

    c2.request("session.input", json!({"session": s, "data_b64": b64("exit\r")}))
        .await
        .unwrap();
    let closed = c2.wait_event("closed", T).await.unwrap();
    assert_eq!(closed["reason"], "exit");
    assert_eq!(closed["exit_status"], 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn backpressure_loses_nothing() {
    let srv = testserver::start(Options::default());
    let d = DEnv::new(60);
    let id = conn(&d.env, "srv", srv.port(), Some(PASSWORD));
    d.env.ok(&["conn", "check", &id, "--accept-new"]);
    let mut c = d.client().await;
    let s = c.request("session.open", json!({"connection_id": id})).await.unwrap()["session"]
        .as_str()
        .unwrap()
        .to_string();
    read_until(&mut c, &s, "READY").await;
    const N: usize = 3 * 1024 * 1024;
    c.request(
        "session.input",
        json!({"session": s, "data_b64": b64(&format!("flood:{N}"))}),
    )
    .await
    .unwrap();
    // Клиент «спит» — демон должен придержать чтение, а не терять данные.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let out = read_until(&mut c, &s, "FLOOD-END").await;
    assert_eq!(out.bytes().filter(|b| *b == b'x').count(), N);
}

fn fake_remote() -> testserver::ExecResponder {
    const SEP: &str = "@@SHERPA@@";
    let stat1 = "cpu  100 0 100 800 0 0 0 0 0 0\ncpu0 100 0 100 800 0 0 0 0 0 0\n";
    let stat2 = "cpu  200 0 200 1000 0 0 0 0 0 0\ncpu0 200 0 200 1000 0 0 0 0 0 0\n";
    let rest = format!(
        "MemTotal: 1000 kB\nMemAvailable: 750 kB\n{SEP}\n0.10 0.20 0.30 1/100 5\n{SEP}\n100.5 1.0\n{SEP}\n    1     0 root  0.0  0.1  1000 100 Ss init\n"
    );
    Arc::new(move |cmd: &str| {
        if cmd.contains("os-release") {
            (
                "os_id=debian\nuid=1000\nuser=u\nsystemd=1\nprocfs=1\ntool_sudo=1\ntool_apt-get=1\n".into(),
                0,
            )
        } else if cmd.contains("/proc/stat") {
            let out = if cmd.contains("sleep") {
                format!("{stat1}{SEP}\n{stat2}{SEP}\n{rest}")
            } else {
                format!("{stat2}{SEP}\n{rest}")
            };
            (out, 0)
        } else if cmd.starts_with("id -u;") {
            ("1000\nsudo=1\n".into(), 0)
        } else if cmd.contains("sudo -S -k") {
            (String::new(), 1)
        } else {
            (String::new(), 127)
        }
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn desktop_monitor_priv() {
    let srv = testserver::start(Options {
        exec: Some(fake_remote()),
        ..Default::default()
    });
    let d = DEnv::new(60);
    let id = conn(&d.env, "srv", srv.port(), Some(PASSWORD));
    let mut c = d.client().await;

    // Без принятого ключа утилиты не подключаются молча.
    let e = c
        .request("monitor.snapshot", json!({"connection_id": id}))
        .await
        .unwrap_err();
    assert_eq!(e.code, "hostkey.unknown");

    // desktop.open спрашивает (события — этому же клиенту) и открывает соединение.
    let open_id = c
        .send_request("desktop.open", json!({"connection_id": id}))
        .await
        .unwrap();
    let r = loop {
        let m = c.next_message(T).await.expect("desktop.open timed out");
        if m["event"] == "hostkey_prompt" {
            assert_eq!(m["connection_id"], id.as_str());
            assert!(m.get("session").is_none());
            c.send_request(
                "session.hostkey_answer",
                json!({"prompt_id": m["prompt_id"], "decision": "accept"}),
            )
            .await
            .unwrap();
        } else if m["id"] == open_id {
            assert_eq!(m["ok"], true, "{m}");
            break m["data"].clone();
        }
    };
    assert_eq!(r["capabilities"]["os"]["id"], "debian");
    assert_eq!(r["capabilities"]["package_manager"], "apt");
    assert_eq!(r["capabilities"]["utilities"]["monitor"], true);
    assert_eq!(r["connection"]["alive"], true);

    let caps = c
        .request("desktop.capabilities", json!({"connection_id": id}))
        .await
        .unwrap();
    assert_eq!(caps["user"]["name"], "u");
    let list = c.request("desktop.list", json!({})).await.unwrap();
    assert_eq!(list["items"].as_array().unwrap().len(), 1);

    // monitor.snapshot
    let m = c
        .request("monitor.snapshot", json!({"connection_id": id, "processes": 5}))
        .await
        .unwrap();
    assert_eq!(m["cpu"]["usage_pct"], 50.0);
    assert_eq!(m["memory"]["usage_pct"], 25.0);
    assert_eq!(m["processes"][0]["command"], "init");

    // monitor.subscribe → события → отписка.
    let sub = c
        .request("monitor.subscribe", json!({"connection_id": id, "interval_ms": 1000}))
        .await
        .unwrap();
    let sid = sub["subscription"].clone();
    for _ in 0..2 {
        let ev = c.wait_event("monitor", T).await.unwrap();
        assert_eq!(ev["subscription"], sid);
        assert!(ev["data"]["load"]["1m"].is_number());
    }
    c.request("monitor.unsubscribe", json!({"subscription": sid}))
        .await
        .unwrap();
    assert_eq!(
        c.request("monitor.unsubscribe", json!({"subscription": sid}))
            .await
            .unwrap_err()
            .code,
        "subscription.not_found"
    );

    // priv.*
    let p = c
        .request("priv.sudo.check", json!({"connection_id": id}))
        .await
        .unwrap();
    assert_eq!(
        (p["is_root"].as_bool(), p["sudo_available"].as_bool()),
        (Some(false), Some(true))
    );
    assert_eq!(p["sudo_authenticated"], false);
    let e = c
        .request("priv.sudo.auth", json!({"connection_id": id, "password": "bad"}))
        .await
        .unwrap_err();
    assert_eq!(e.code, "priv.auth_failed");
    let p = c
        .request("priv.readonly.set", json!({"connection_id": id, "readonly": true}))
        .await
        .unwrap();
    assert_eq!(p["readonly"], true);

    let closed = c.request("desktop.close", json!({"connection_id": id})).await.unwrap();
    assert_eq!(closed["closed"], true);
    assert_eq!(c.request("desktop.list", json!({})).await.unwrap()["items"], json!([]));
}
