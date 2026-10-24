//! Сервер демона: приём клиентов, разбор JSON Lines, рукопожатие, диспетчер,
//! фоновая уборка (отцепленные сессии, простой, пул).

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

use super::proto::{self, Request, MAX_LINE};
use super::registry::{dispatch, Cx};
use super::session::Ctrl;
use super::state::{ClientHandle, DaemonState, CLIENT_QUEUE, DETACH_GRACE, POOL_IDLE};
use super::transport::{BoxStream, Listener};
use crate::error::CoreError;
use crate::protocol::{PROTOCOL, PROTOCOL_MIN};

/// Работать до запроса остановки или простоя. Возвращает причину остановки.
pub async fn serve(state: Arc<DaemonState>, mut listener: Listener) -> String {
    let mut stop = state.shutdown.subscribe();
    tokio::spawn(reaper(state.clone()));
    loop {
        tokio::select! {
            acc = listener.accept() => match acc {
                Ok(stream) => { tokio::spawn(client_loop(state.clone(), stream)); }
                Err(e) => { tracing::warn!("accept failed: {e}"); tokio::time::sleep(Duration::from_millis(100)).await; }
            },
            _ = stop.changed() => break,
        }
    }
    let reason = state.shutdown.borrow().clone().unwrap_or_else(|| "stop".into());
    shutdown(&state, &reason).await;
    reason
}

async fn shutdown(state: &Arc<DaemonState>, reason: &str) {
    let ev = proto::event("daemon_shutdown", json!({ "reason": reason }));
    let clients: Vec<ClientHandle> = state.clients.lock().expect("clients").values().cloned().collect();
    for c in &clients {
        let _ = tokio::time::timeout(Duration::from_millis(200), c.send(&ev)).await;
    }
    let sessions: Vec<_> = state.sessions.lock().expect("sessions").values().cloned().collect();
    for s in &sessions {
        let _ = s.ctrl.send(Ctrl::Close("client"));
    }
    for _ in 0..20 {
        if state.sessions.lock().expect("sessions").is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    state.pool.close_all().await;
    #[cfg(unix)]
    let _ = std::fs::remove_file(&state.paths.endpoint);
}

async fn reaper(state: Arc<DaemonState>) {
    let mut tick = 0u64;
    loop {
        tokio::time::sleep(Duration::from_millis(250)).await;
        tick += 1;
        // Сессии без клиента дольше DETACH_GRACE закрываются.
        let expired: Vec<_> = state
            .sessions
            .lock()
            .expect("sessions")
            .values()
            .filter(|s| s.detached_for().is_some_and(|d| d > DETACH_GRACE))
            .cloned()
            .collect();
        for s in expired {
            let _ = s.ctrl.send(Ctrl::Close("client"));
        }
        // Простой: ни сессий, ни клиентов.
        let idle = state.settings.daemon_idle_timeout_sec;
        let empty =
            state.sessions.lock().expect("sessions").is_empty() && state.clients.lock().expect("clients").is_empty();
        if idle > 0 && empty && state.idle_for() > Duration::from_secs(idle) {
            tracing::info!("idle for {idle}s, shutting down");
            state.request_shutdown("idle");
            return;
        }
        if tick.is_multiple_of(40) {
            state.pool.reap_idle(POOL_IDLE).await;
        }
    }
}

async fn client_loop(state: Arc<DaemonState>, stream: BoxStream) {
    let (rd, mut wr) = tokio::io::split(stream);
    let (tx, mut rx) = mpsc::channel::<String>(CLIENT_QUEUE);
    let client = ClientHandle {
        id: state.new_client_id(),
        tx,
    };
    state.clients.lock().expect("clients").insert(client.id, client.clone());
    state.touch();
    let writer = tokio::spawn(async move {
        while let Some(line) = rx.recv().await {
            if wr.write_all(line.as_bytes()).await.is_err() || wr.write_all(b"\n").await.is_err() {
                break;
            }
            if rx.is_empty() && wr.flush().await.is_err() {
                break;
            }
        }
        let _ = wr.shutdown().await;
    });

    let mut reader = BufReader::new(rd);
    let mut hello = false;
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        if buf.len() > MAX_LINE {
            let e = CoreError::new("request.invalid_json", "Line is too long").with("offset", MAX_LINE);
            client.send(&proto::err(&Value::Null, &e)).await;
            break;
        }
        let line = String::from_utf8_lossy(&buf);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let req: Request = match serde_json::from_str(line) {
            Ok(r) => r,
            Err(e) => {
                let err =
                    CoreError::new("request.invalid_json", format!("Invalid request: {e}")).with("offset", e.column());
                client.send(&proto::err(&Value::Null, &err)).await;
                continue;
            }
        };
        state.touch();
        if req.method == "hello" {
            let r = handle_hello(&state, &req.params);
            hello = r.is_ok();
            let msg = match r {
                Ok(v) => proto::ok(&req.id, v),
                Err(e) => proto::err(&req.id, &e),
            };
            client.send(&msg).await;
            continue;
        }
        if !hello {
            let e = CoreError::new("protocol.hello_required", "Send hello first");
            client.send(&proto::err(&req.id, &e)).await;
            continue;
        }
        if req.method == "ping" {
            client.send(&proto::ok(&req.id, json!({ "pong": true }))).await;
            continue;
        }
        let cx = Cx {
            state: state.clone(),
            client: client.clone(),
        };
        tokio::spawn(async move {
            let msg = match dispatch(&cx, &req.method, req.params).await {
                Ok(v) => proto::ok(&req.id, v),
                Err(e) => proto::err(&req.id, &e),
            };
            cx.client.send(&msg).await;
            cx.state.touch();
        });
    }
    state.client_gone(client.id);
    drop(client);
    let _ = writer.await;
}

fn handle_hello(state: &DaemonState, p: &Value) -> Result<Value, CoreError> {
    let v = p
        .get("protocol")
        .and_then(Value::as_u64)
        .ok_or_else(|| CoreError::required("protocol"))? as u32;
    if !(PROTOCOL_MIN..=PROTOCOL).contains(&v) {
        return Err(
            CoreError::new("protocol.unsupported", format!("Protocol {v} is not supported"))
                .with("required", v)
                .with("min", PROTOCOL_MIN)
                .with("max", PROTOCOL),
        );
    }
    Ok(json!({
        "protocol": PROTOCOL,
        "core_version": env!("CARGO_PKG_VERSION"),
        "daemon_pid": std::process::id(),
        "sessions": state.session_ids(),
    }))
}
