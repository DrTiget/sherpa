//! SSH-сценарии через CLI против встроенного тестового сервера (russh::server).

mod common;

use std::io::Write;
use std::time::Instant;

use common::Env;
use serde_json::{json, Value};
use sherpa_lib::testserver::{self, Options, PASSWORD, USER};

fn pw_conn(e: &Env, name: &str, port: u16, password: Option<&str>) -> String {
    let mut auth = json!({"method": "password"});
    if let Some(p) = password {
        auth["password"] = p.into();
    }
    e.create(&json!({"name": name, "host": "127.0.0.1", "port": port, "user": USER, "auth": auth}).to_string())
}

fn check(e: &Env, args: &[&str]) -> Value {
    let mut a = vec!["conn", "check", "--timeout", "5"];
    a.extend_from_slice(args);
    e.ok(&a)
}

fn accept(e: &Env, port: u16, fp: &str, extra: &[&str]) -> (Value, i32) {
    let port = port.to_string();
    let mut a = vec![
        "hostkey",
        "accept",
        "--host",
        "127.0.0.1",
        "--port",
        &port,
        "--key-type",
        "ssh-ed25519",
        "--fingerprint",
        fp,
    ];
    a.extend_from_slice(extra);
    e.run(&a)
}

#[test]
fn tofu_password_and_status() {
    let srv = testserver::start(Options::default());
    let e = Env::initialized();
    let id = pw_conn(&e, "srv", srv.port(), Some(PASSWORD));

    // Первый раз: ключ неизвестен, авторизация не выполняется.
    let r = check(&e, &[&id]);
    let it = &r["items"][0];
    assert_eq!(it["state"], "error");
    assert_eq!(it["error"]["code"], "hostkey.unknown");
    assert_eq!(it["error"]["params"]["fingerprint"], srv.fingerprint().as_str());
    assert_eq!(it["error"]["params"]["key_type"], "ssh-ed25519");
    assert_eq!(it["error"]["params"]["port"], srv.port());

    // Принятие с неверным отпечатком отклоняется.
    let (v, _) = accept(&e, srv.port(), "SHA256:wrong", &[]);
    assert_eq!(v["error"]["code"], "hostkey.fingerprint_changed");
    let (v, code) = accept(&e, srv.port(), &srv.fingerprint(), &[]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["data"]["host_key"]["state"], "trusted");

    let r = check(&e, &[&id]);
    let it = &r["items"][0];
    assert_eq!(it["state"], "online", "{r}");
    assert!(it["latency_ms"].as_i64().unwrap() >= 0);
    assert!(it["server_banner"].as_str().unwrap().starts_with("SSH-2.0-"));
    assert_eq!(it["error"], Value::Null);
    let st = e.ok(&["status", "list", &id]);
    assert_eq!(st["items"][0]["state"], "online");
    let l = e.ok(&["conn", "list", "--status", "online"]);
    assert_eq!(l["total"], 1);

    // Неверный пароль → auth_failed.
    let bad = pw_conn(&e, "bad", srv.port(), Some("nope"));
    let r = check(&e, &[&bad]);
    assert_eq!(r["items"][0]["state"], "auth_failed");
    assert_eq!(r["items"][0]["error"]["code"], "auth.rejected");
    assert_eq!(r["items"][0]["error"]["params"]["user"], USER);

    // Пароль не сохранён → нужен секрет; разовый секрет через stdin.
    let nopw = pw_conn(&e, "nopw", srv.port(), None);
    let r = check(&e, &[&nopw]);
    assert_eq!(r["items"][0]["error"]["code"], "auth.secret_required");
    assert_eq!(r["items"][0]["error"]["params"]["kind"], "password");
    let body = json!({"secrets": {nopw.clone(): {"password": PASSWORD}}}).to_string();
    let r = e.ok_in(&["conn", "check", &nopw], Some(&body));
    assert_eq!(r["items"][0]["state"], "online");
    // разовый пароль не сохранился
    assert_eq!(
        e.ok(&["conn", "get", &nopw])["connection"]["auth"]["has_password"],
        false
    );

    let hk = e.ok(&[
        "hostkey",
        "get",
        "--host",
        "127.0.0.1",
        "--port",
        &srv.port().to_string(),
    ]);
    assert_eq!(hk["items"][0]["fingerprint"], srv.fingerprint().as_str());
}

#[test]
fn key_auth_file_and_text() {
    let key = sherpa_lib::ssh::keys::generate_ed25519_openssh(Some("pp")).unwrap();
    let pubkey = sherpa_lib::ssh::keys::decode_key(&key, Some("pp"))
        .unwrap()
        .public_key()
        .clone();
    let srv = testserver::start(Options {
        allowed_key: Some(pubkey),
        password_allowed: false,
        ..Default::default()
    });
    let e = Env::initialized();
    let key_path = e.path("id_test");
    std::fs::File::create(&key_path)
        .unwrap()
        .write_all(key.as_bytes())
        .unwrap();

    let body = json!({"name":"kf","host":"127.0.0.1","port":srv.port(),"user":USER,
        "auth":{"method":"key_file","key_path":key_path,"passphrase":"pp"}});
    let kf = e.create(&body.to_string());
    let body = json!({"name":"kt","host":"127.0.0.1","port":srv.port(),"user":USER,
        "auth":{"method":"key_text","key_text":key.as_str(),"passphrase":"pp"}});
    let kt = e.create(&body.to_string());
    let body = json!({"name":"missing","host":"127.0.0.1","port":srv.port(),"user":USER,
        "auth":{"method":"key_file","key_path":e.path("nope")}});
    let missing = e.create(&body.to_string());
    let body = json!({"name":"nopass","host":"127.0.0.1","port":srv.port(),"user":USER,
        "auth":{"method":"key_file","key_path":key_path}});
    let nopass = e.create(&body.to_string());
    let pw = pw_conn(&e, "pw", srv.port(), Some(PASSWORD));

    // --accept-new: первый массовый прогон сохраняет новые ключи.
    let r = check(&e, &[&kf, &kt, &missing, &nopass, &pw, "--accept-new"]);
    let st: Vec<(&str, &str)> = r["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| (i["state"].as_str().unwrap(), i["error"]["code"].as_str().unwrap_or("")))
        .collect();
    assert_eq!(
        st,
        vec![
            ("online", ""),
            ("online", ""),
            ("auth_failed", "auth.key_file_not_found"),
            ("auth_failed", "auth.secret_required"),
            ("auth_failed", "auth.method_not_allowed"),
        ],
        "{r}"
    );
    let keys = e.ok(&["hostkey", "list"]);
    assert_eq!(keys["items"].as_array().unwrap().len(), 1);

    // Неверная фраза.
    e.ok_in(&["conn", "update", &kf], Some(r#"{"auth":{"passphrase":"wrong"}}"#));
    let r = check(&e, &[&kf]);
    assert_eq!(r["items"][0]["error"]["code"], "auth.passphrase_wrong");
    // Для key_text неверная фраза отлавливается уже при сохранении.
    let (code, _, _) = e.err_in(&["conn", "update", &kt], Some(r#"{"auth":{"passphrase":"wrong"}}"#));
    assert_eq!(code, "auth.passphrase_wrong");
}

#[test]
fn host_key_change_reject_forget() {
    let srv = testserver::start(Options {
        seed: 7,
        ..Default::default()
    });
    let port = srv.port();
    let old_fp = srv.fingerprint();
    let e = Env::initialized();
    let id = pw_conn(&e, "srv", port, Some(PASSWORD));
    assert_eq!(check(&e, &[&id, "--accept-new"])["items"][0]["state"], "online");
    drop(srv);

    // Тот же адрес, другой ключ.
    let srv = testserver::start(Options {
        port,
        seed: 8,
        ..Default::default()
    });
    let r = check(&e, &[&id, "--accept-new"]);
    let err = &r["items"][0]["error"];
    assert_eq!(err["code"], "hostkey.mismatch", "{r}");
    assert_eq!(err["params"]["known_fingerprint"], old_fp.as_str());
    assert_eq!(err["params"]["fingerprint"], srv.fingerprint().as_str());
    assert!(err["params"]["known_accepted_at"].is_string());

    let (v, _) = accept(&e, port, &srv.fingerprint(), &[]);
    assert_eq!(v["error"]["code"], "hostkey.mismatch");
    let (v, code) = accept(&e, port, &srv.fingerprint(), &["--replace"]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["data"]["host_key"]["replaced_fingerprint"], old_fp.as_str());
    assert_eq!(check(&e, &[&id])["items"][0]["state"], "online");

    // Отклонение: дальше сразу hostkey.rejected.
    let p = port.to_string();
    e.ok(&["hostkey", "forget", "--host", "127.0.0.1", "--port", &p]);
    e.ok(&[
        "hostkey",
        "reject",
        "--host",
        "127.0.0.1",
        "--port",
        &p,
        "--fingerprint",
        &srv.fingerprint(),
    ]);
    let r = check(&e, &[&id, "--accept-new"]);
    assert_eq!(r["items"][0]["error"]["code"], "hostkey.rejected");
    e.ok(&["hostkey", "forget", "--host", "127.0.0.1", "--port", &p]);
    assert_eq!(
        e.err(&["hostkey", "get", "--host", "127.0.0.1", "--port", &p]).0,
        "hostkey.not_found"
    );
    assert_eq!(check(&e, &[&id])["items"][0]["error"]["code"], "hostkey.unknown");
}

#[test]
fn batch_parallel_and_stream() {
    let srv = testserver::start(Options::default());
    let e = Env::initialized();
    let (v, _) = accept(&e, srv.port(), &srv.fingerprint(), &[]);
    assert_eq!(v["ok"], true);
    for i in 0..30 {
        pw_conn(&e, &format!("s{i:02}"), srv.port(), Some(PASSWORD));
    }
    let dead = e.create(r#"{"name":"dead","host":"127.0.0.1","port":1}"#);
    let t = Instant::now();
    let r = check(&e, &["--all", "--concurrency", "16"]);
    assert_eq!(
        r["summary"],
        json!({"online": 30, "offline": 1, "auth_failed": 0, "error": 0, "total": 31})
    );
    assert!(t.elapsed().as_secs() < 20);

    // Поток: строка на сервер сразу, итог последней строкой.
    let (lines, code) = e.lines(&["conn", "check", "--all", "--stream", "--timeout", "5"]);
    assert_eq!(code, 0);
    assert_eq!(lines.len(), 32);
    for l in &lines[..31] {
        assert_eq!(l["event"], "result");
        assert!(l["item"]["id"].is_string());
    }
    let last = &lines[31];
    assert_eq!(last["ok"], true);
    assert_eq!(last["data"]["summary"]["total"], 31);
    assert!(last["data"].get("items").is_none());
    assert!(lines.iter().any(|l| l["item"]["id"] == dead.as_str()));
}

#[test]
fn not_ssh_and_timeout() {
    use std::net::TcpListener;
    let http = TcpListener::bind("127.0.0.1:0").unwrap();
    let http_port = http.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for s in http.incoming().flatten() {
            let mut s = s;
            let _ = s.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n");
        }
    });
    let silent = TcpListener::bind("127.0.0.1:0").unwrap();
    let silent_port = silent.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let mut keep = Vec::new();
        for s in silent.incoming().flatten() {
            keep.push(s);
        }
    });
    let e = Env::initialized();
    let a = pw_conn(&e, "http", http_port, Some("x"));
    let b = pw_conn(&e, "silent", silent_port, Some("x"));
    let r = e.ok(&["conn", "check", &a, &b, "--timeout", "1"]);
    assert_eq!(r["items"][0]["error"]["code"], "ssh.not_ssh", "{r}");
    assert_eq!(r["items"][0]["state"], "error");
    assert!(r["items"][0]["error"]["params"]["banner"]
        .as_str()
        .unwrap()
        .starts_with("HTTP/1.1"));
    assert_eq!(r["items"][1]["error"]["code"], "net.timeout");
    assert_eq!(r["items"][1]["state"], "offline");
    assert_eq!(r["items"][1]["error"]["params"]["timeout_sec"], 1);
}
