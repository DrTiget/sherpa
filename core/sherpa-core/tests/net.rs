//! Network daemon contract against the embedded SSH server; no host firewall changes.
mod common;
use common::Env;
use serde_json::{json, Value};
use sherpa_lib::{
    config::{Overrides, Settings},
    daemon::{
        registry::{dispatch, Cx},
        state::{ClientHandle, DaemonState},
    },
    testserver::{self, Options, PASSWORD, USER},
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

async fn call(cx: &Cx, method: &str, p: Value) -> Value {
    dispatch(cx, method, p)
        .await
        .unwrap_or_else(|e| panic!("{method}: {e:?}"))
}

#[tokio::test(flavor = "multi_thread")]
async fn network_confirmations_readonly_and_stream() {
    let polls = Arc::new(AtomicUsize::new(0));
    let n = polls.clone();
    let srv = testserver::start(Options {
        exec: Some(Arc::new(move |cmd: &str| {
            if cmd.contains("ip -j address") {
                (r#"[{"ifindex":1,"ifname":"lo","addr_info":[{"family":"inet","local":"127.0.0.1","prefixlen":8}]}]"#.into(),0)
            } else if cmd.starts_with("id -u;") {
                ("0\nsudo=1\n".into(), 0)
            } else if cmd.contains("echo confirmed >") {
                ("confirmed\n".into(), 0)
            } else if cmd.contains("nohup sh") {
                ("pending\n".into(), 0)
            } else if cmd.contains("head -c") {
                if n.fetch_add(1, Ordering::SeqCst) == 0 {
                    ("running\nfirst probe\n".into(), 0)
                } else {
                    ("0\nfirst probe\nsecond probe\n".into(), 0)
                }
            } else {
                (String::new(), 127)
            }
        })),
        ..Default::default()
    });
    let env = Env::initialized();
    let id=env.create(&json!({"name":"net","host":"127.0.0.1","port":srv.port(),"user":USER,"auth":{"method":"password","password":PASSWORD}}).to_string());
    env.ok(&["conn", "check", &id, "--accept-new"]);
    let settings = Settings::load(&Overrides {
        config: Some(env.path("core.toml")),
        db: Some(env.path("sherpa.db")),
        key_file: Some(env.path("sherpa.key")),
    })
    .unwrap();
    let state = DaemonState::new(settings.clone(), sherpa_lib::daemon::paths(&settings));
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    let cx = Cx {
        state: state.clone(),
        client: ClientHandle { id: 1, tx },
    };
    let (tx, _rx) = tokio::sync::mpsc::channel(32);
    let other = Cx {
        state: state.clone(),
        client: ClientHandle { id: 2, tx },
    };
    assert_eq!(
        call(&cx, "net.iface.list", json!({"connection_id":id})).await["items"][0]["ifname"],
        "lo"
    );
    let mut p = json!({"connection_id":id,"backend":"nft","operation":"add","action":"allow","protocol":"tcp","port":443,"confirm":true});
    assert_eq!(
        dispatch(&cx, "net.apply", p.clone()).await.unwrap_err().code,
        "danger.confirmation_required"
    );
    let prepared = call(&cx, "danger.prepare", json!({"method":"net.apply","params":p})).await;
    p["confirm_token"] = prepared["confirm_token"].clone();
    assert_eq!(
        dispatch(&other, "net.apply", p.clone()).await.unwrap_err().code,
        "danger.confirmation_required"
    );
    let mut altered = p.clone();
    altered["port"] = json!(22);
    assert_eq!(
        dispatch(&cx, "net.apply", altered).await.unwrap_err().code,
        "danger.confirmation_required"
    );
    let applied = call(&cx, "net.apply", p.clone()).await;
    assert_eq!(applied["state"], "pending");
    assert_eq!(
        call(
            &cx,
            "net.confirm",
            json!({"connection_id":id,"transaction":applied["transaction"]})
        )
        .await["state"],
        "confirmed"
    );
    assert_eq!(
        dispatch(&cx, "net.apply", p.clone()).await.unwrap_err().code,
        "danger.confirmation_required"
    );
    call(&cx, "priv.readonly.set", json!({"connection_id":id,"readonly":true})).await;
    assert_eq!(dispatch(&cx, "net.apply", p).await.unwrap_err().code, "priv.readonly");
    // Diagnostic reads remain available while connection is read-only.
    let sub = call(&cx, "nettools.ping", json!({"connection_id":id,"target":"example.org"})).await;
    let mut records = Vec::new();
    loop {
        let ev = tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .unwrap()
            .unwrap();
        let ev: Value = serde_json::from_str(&ev).unwrap();
        assert_eq!(ev["subscription"], sub["subscription"]);
        match ev["event"].as_str().unwrap() {
            "nettools.output" => records.extend(ev["records"].as_array().unwrap().iter().cloned()),
            "nettools.done" => {
                assert_eq!(ev["exit_status"], 0);
                break;
            }
            _ => panic!("unexpected event {ev}"),
        }
    }
    assert_eq!(
        records,
        json!([{"text":"first probe"},{"text":"second probe"}])
            .as_array()
            .unwrap()
            .clone()
    );
    assert_eq!(
        dispatch(&other, "nettools.cancel", sub.clone()).await.unwrap_err().code,
        "subscription.not_found"
    );
    polls.store(0, Ordering::SeqCst);
    let sub = call(&cx, "nettools.ping", json!({"connection_id":id,"target":"example.org"})).await;
    // Keep the fake job running for ownership/cancellation checks.
    assert_eq!(
        dispatch(&other, "nettools.cancel", sub.clone()).await.unwrap_err().code,
        "subscription.not_found"
    );
    call(&cx, "nettools.cancel", sub).await;
    state.pool.close(&id).await;
}
