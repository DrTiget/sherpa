//! TCP и SOCKS5 (CONNECT, без авторизации). Жизненный цикл — в демоне.
use super::{files::number, jobs::Job};
use crate::{
    daemon::{
        proto::{opt_str, req_str},
        registry::{Cx, MethodGroup},
        state::DaemonState,
    },
    error::{CoreError, CoreResult},
    ssh::PooledConn,
};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinSet,
};
pub struct TunnelMethods;
fn error() -> CoreError {
    CoreError::new("tunnel.failed", "TCP forwarding failed")
}
#[async_trait]
impl MethodGroup for TunnelMethods {
    fn prefix(&self) -> &'static str {
        "tunnel"
    }
    fn methods(&self) -> &'static [&'static str] {
        &["tunnel.start", "tunnel.stop", "tunnel.list"]
    }
    async fn call(&self, cx: &Cx, method: &str, p: Value) -> CoreResult<Value> {
        if method == "tunnel.list" {
            return Ok(cx.state.jobs.list("tunnel"));
        }
        if method == "tunnel.stop" {
            let j = cx.state.jobs.get(&req_str(&p, "id")?, "tunnel")?;
            j.cancel.send_replace(true);
            return Ok(json!({"stopping":true}));
        }
        let kind = req_str(&p, "kind")?;
        if !matches!(kind.as_str(), "local" | "remote" | "dynamic") {
            return Err(CoreError::invalid("kind", "expected local, remote or dynamic"));
        }
        let bind = opt_str(&p, "bind_host")?.unwrap_or_else(|| "127.0.0.1".into());
        let ip: IpAddr = bind
            .parse()
            .map_err(|_| CoreError::invalid("bind_host", "expected IP address"))?;
        let port = number(&p, "bind_port", 65535)? as u16;
        let dest = if kind == "dynamic" {
            None
        } else {
            let port = number(&p, "target_port", 65535)?;
            if port == 0 {
                return Err(CoreError::invalid("target_port", "must be nonzero"));
            }
            Some((req_str(&p, "target_host")?, port))
        };
        if kind == "remote" && port == 0 {
            return Err(CoreError::invalid("bind_port", "remote port must be nonzero"));
        }
        let connection = req_str(&p, "connection_id")?;
        let conn = cx.conn(&connection).await?;
        conn.ensure_writable()?;
        if !ip.is_loopback() {
            cx.state.confirmations.consume(cx.client.id, method, &p)?;
        }
        let listener = if kind != "remote" {
            Some(TcpListener::bind((ip, port)).await.map_err(|_| error())?)
        } else {
            None
        };
        let actual = listener
            .as_ref()
            .map(|l| l.local_addr().map(|a| a.port()))
            .transpose()
            .map_err(|_| error())?
            .unwrap_or(port);
        let j = cx.state.jobs.insert(
            cx.state.new_id("t"),
            "tunnel",
            connection,
            json!({"status":"starting","kind":kind,"bind_host":bind,"bind_port":actual,"target":dest}),
        )?;
        let id = j.id.clone();
        let state = cx.state.clone();
        let mut cancel = j.cancel.subscribe();
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        if kind == "remote" {
            let mut routes = conn.forwards().lock().expect("forward routes");
            if routes.contains_key(&(bind.clone(), port as u32)) {
                cx.state.jobs.entries.lock().expect("jobs").remove(&id);
                return Err(CoreError::new("tunnel.exists", "Remote forwarding already exists"));
            }
            routes.insert((bind.clone(), port as u32), conn.forward_route(tx));
        }
        tokio::spawn(async move {
            // Удерживает соединение даже когда через слушатель нет трафика.
            conn.tunnel_opened();
            let result = {
                let work = async {
                    if kind == "remote" {
                        conn.handle()
                            .tcpip_forward(bind.clone(), port as u32)
                            .await
                            .map_err(|_| error())?;
                    }
                    j.update(
                        &state,
                        json!({"status":"running","kind":kind,"bind_host":bind,"bind_port":actual,"target":dest}),
                    )
                    .await;
                    serve(listener, rx, conn.clone(), state.clone(), j.clone(), dest).await
                };
                tokio::select! { biased; _ = cancel.wait_for(|v| *v) => Ok(()), result = work => result }
            };
            if kind == "remote" {
                conn.forwards()
                    .lock()
                    .expect("forward routes")
                    .remove(&(bind.clone(), port as u32));
                let _ = tokio::time::timeout(
                    Duration::from_secs(5),
                    conn.handle().cancel_tcpip_forward(bind.clone(), port as u32),
                )
                .await;
            }
            conn.tunnel_closed();
            match result {
                Ok(()) => j.update(&state, json!({"status":"stopped"})).await,
                Err(e) => j.update(&state, json!({"status":"failed","error":e})).await,
            }
        });
        Ok(
            json!({"id":id,"bind_host":p.get("bind_host").and_then(Value::as_str).unwrap_or("127.0.0.1"),"bind_port":actual,"status":"starting"}),
        )
    }
}
async fn serve(
    listener: Option<TcpListener>,
    mut incoming: tokio::sync::mpsc::Receiver<crate::ssh::client::Forwarded>,
    conn: Arc<PooledConn>,
    state: Arc<DaemonState>,
    job: Arc<Job>,
    dest: Option<(String, u32)>,
) -> CoreResult<()> {
    let mut tasks = JoinSet::new();
    let mut health = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = health.tick() => { if !conn.is_alive() { return Err(error()); } }
            Some(_) = tasks.join_next(), if !tasks.is_empty() => {}
            accepted = async { match &listener { Some(l) => l.accept().await, None => std::future::pending().await } }, if tasks.len() < 32 => {
                let (socket,_) = accepted.map_err(|_| error())?;
                let (conn,state,job,dest) = (conn.clone(),state.clone(),job.clone(),dest.clone());
                tasks.spawn(async move { let r = local(socket,&conn,dest).await; notify(r,&state,&job).await; });
            }
            channel = incoming.recv(), if listener.is_none() && tasks.len() < 32 => {
                let (channel, slot) = channel.ok_or_else(error)?;
                let (conn,state,job,dest) = (conn.clone(),state.clone(),job.clone(),dest.clone());
                tasks.spawn(async move {
                    let work = async {
                        conn.ensure_writable()?;
                        let _permit = slot;
                        let (host,port) = dest.ok_or_else(error)?;
                        let mut tcp = tokio::time::timeout(Duration::from_secs(15),TcpStream::connect((host.as_str(),port as u16))).await.map_err(|_| error())?.map_err(|_| error())?;
                        tokio::io::copy_bidirectional(&mut tcp,&mut channel.into_stream()).await.map_err(|_| error())?; Ok(())
                    };
                    notify(work.await,&state,&job).await;
                });
            }
        }
    }
}
async fn notify(result: CoreResult<()>, state: &DaemonState, job: &Job) {
    if let Err(e) = result {
        let mut data = job.state.lock().expect("job").clone();
        data["error"] = serde_json::to_value(e).unwrap_or(Value::Null);
        job.update(state, data).await;
    }
}
async fn local(mut tcp: TcpStream, conn: &PooledConn, dest: Option<(String, u32)>) -> CoreResult<()> {
    conn.ensure_writable()?;
    let dynamic = dest.is_none();
    let target = match dest {
        Some(v) => v,
        None => tokio::time::timeout(Duration::from_secs(15), socks(&mut tcp))
            .await
            .map_err(|_| error())??,
    };
    let _permit = conn.channel_permit().await?;
    let peer = tcp.peer_addr().map_err(|_| error())?;
    let channel = conn
        .handle()
        .channel_open_direct_tcpip(target.0, target.1, peer.ip().to_string(), peer.port() as u32)
        .await;
    let channel = match channel {
        Ok(c) => c,
        Err(_) => {
            if dynamic {
                let _ = tcp.write_all(&[5, 5, 0, 1, 0, 0, 0, 0, 0, 0]).await;
            }
            return Err(error());
        }
    };
    if dynamic {
        tcp.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])
            .await
            .map_err(|_| error())?;
    }
    tokio::io::copy_bidirectional(&mut tcp, &mut channel.into_stream())
        .await
        .map_err(|_| error())?;
    Ok(())
}
async fn socks(tcp: &mut TcpStream) -> CoreResult<(String, u32)> {
    let version = tcp.read_u8().await.map_err(|_| error())?;
    let n = tcp.read_u8().await.map_err(|_| error())?;
    if version != 5 || n == 0 {
        return Err(error());
    }
    let mut methods = vec![0; n as usize];
    tcp.read_exact(&mut methods).await.map_err(|_| error())?;
    if !methods.contains(&0) {
        let _ = tcp.write_all(&[5, 255]).await;
        return Err(error());
    }
    tcp.write_all(&[5, 0]).await.map_err(|_| error())?;
    let mut header = [0; 4];
    tcp.read_exact(&mut header).await.map_err(|_| error())?;
    if header[..3] != [5, 1, 0] {
        let _ = tcp.write_all(&[5, 7, 0, 1, 0, 0, 0, 0, 0, 0]).await;
        return Err(error());
    }
    let host = match header[3] {
        1 => {
            let mut b = [0; 4];
            tcp.read_exact(&mut b).await.map_err(|_| error())?;
            Ipv4Addr::from(b).to_string()
        }
        4 => {
            let mut b = [0; 16];
            tcp.read_exact(&mut b).await.map_err(|_| error())?;
            Ipv6Addr::from(b).to_string()
        }
        3 => {
            let n = tcp.read_u8().await.map_err(|_| error())?;
            if n == 0 {
                return Err(error());
            }
            let mut b = vec![0; n as usize];
            tcp.read_exact(&mut b).await.map_err(|_| error())?;
            String::from_utf8(b).map_err(|_| error())?
        }
        _ => {
            let _ = tcp.write_all(&[5, 8, 0, 1, 0, 0, 0, 0, 0, 0]).await;
            return Err(error());
        }
    };
    let port = tcp.read_u16().await.map_err(|_| error())?;
    if port == 0 {
        return Err(error());
    }
    Ok((host, port as u32))
}
