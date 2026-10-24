//! Терминальная PTY-сессия (CORE_API §4.3–4.4).

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use base64::Engine;
use russh::ChannelMsg;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::connect::{self, EventPrompter, EventSink};
use super::proto::event;
use super::state::{ClientHandle, DaemonState};
use crate::error::CoreError;
use crate::ssh::target::OneTimeSecrets;

/// Хвост вывода для `session.attach`.
pub const TAIL_CAP: usize = 64 * 1024;
/// Склейка вывода: не дольше 10 мс и не больше 32 КиБ на событие.
pub const COALESCE: Duration = Duration::from_millis(10);
pub const CHUNK: usize = 32 * 1024;

pub enum Ctrl {
    Input(Vec<u8>),
    Resize(u32, u32),
    Close(&'static str),
}

/// Кольцевой буфер последних байт вывода.
#[derive(Default)]
pub struct Tail {
    buf: VecDeque<u8>,
}

impl Tail {
    pub fn push(&mut self, data: &[u8]) {
        if data.len() >= TAIL_CAP {
            self.buf.clear();
            self.buf.extend(&data[data.len() - TAIL_CAP..]);
            return;
        }
        let overflow = (self.buf.len() + data.len()).saturating_sub(TAIL_CAP);
        self.buf.drain(..overflow);
        self.buf.extend(data);
    }

    pub fn snapshot(&self) -> Vec<u8> {
        self.buf.iter().copied().collect()
    }
}

#[derive(Default)]
struct Inner {
    attached: Option<ClientHandle>,
    detached_at: Option<Instant>,
    tail: Tail,
    /// Неотвеченный вопрос — повторяется при `attach`.
    pending_prompt: Option<Value>,
    state: &'static str,
    cols: u32,
    rows: u32,
}

pub struct TermSession {
    pub id: String,
    pub connection_id: String,
    pub opened_at: String,
    inner: Mutex<Inner>,
    pub ctrl: mpsc::UnboundedSender<Ctrl>,
}

pub struct OpenParams {
    pub connection_id: String,
    pub cols: u32,
    pub rows: u32,
    pub term: String,
    pub env: HashMap<String, String>,
    pub secrets: OneTimeSecrets,
}

impl TermSession {
    pub fn client(&self) -> Option<ClientHandle> {
        self.inner.lock().expect("session").attached.clone()
    }

    pub fn owned_by(&self, client: u64) -> bool {
        self.client().is_some_and(|c| c.id == client)
    }

    pub fn set_state(&self, st: &'static str) {
        self.inner.lock().expect("session").state = st;
    }

    pub fn detach_if(&self, client: u64) {
        let mut g = self.inner.lock().expect("session");
        if g.attached.as_ref().is_some_and(|c| c.id == client) {
            g.attached = None;
            g.detached_at = Some(Instant::now());
        }
    }

    pub fn detached_for(&self) -> Option<Duration> {
        self.inner.lock().expect("session").detached_at.map(|t| t.elapsed())
    }

    /// Подхватить сессию: вернуть хвост вывода и неотвеченный вопрос.
    pub fn attach(&self, client: ClientHandle) -> Result<(Vec<u8>, Option<Value>), CoreError> {
        let mut g = self.inner.lock().expect("session");
        if let Some(c) = &g.attached {
            if c.id != client.id {
                return Err(not_found(&self.id));
            }
        }
        g.attached = Some(client);
        g.detached_at = None;
        Ok((g.tail.snapshot(), g.pending_prompt.clone()))
    }

    pub fn info(&self) -> Value {
        let g = self.inner.lock().expect("session");
        json!({
            "session": self.id,
            "connection_id": self.connection_id,
            "state": g.state,
            "opened_at": self.opened_at,
            "cols": g.cols,
            "rows": g.rows,
            "attached": g.attached.is_some(),
        })
    }

    pub async fn emit(&self, name: &str, mut fields: Value) {
        fields["session"] = self.id.clone().into();
        let ev = event(name, fields);
        if let Some(c) = self.client() {
            if !c.send(&ev).await {
                self.detach_if(c.id);
            }
        }
    }

    fn output_event(&self, data: &[u8]) -> Value {
        event(
            "output",
            json!({ "session": self.id, "data_b64": base64::engine::general_purpose::STANDARD.encode(data) }),
        )
    }

    /// Записать вывод в хвост; вернуть событие, если есть кому его отдать.
    fn record_output(&self, data: &[u8]) -> Option<Value> {
        let mut g = self.inner.lock().expect("session");
        g.tail.push(data);
        g.attached.as_ref().map(|_| self.output_event(data))
    }
}

pub fn not_found(id: &str) -> CoreError {
    CoreError::new("session.not_found", format!("Session not found: {id}")).with("session", id)
}

#[async_trait]
impl EventSink for TermSession {
    async fn emit_prompt(&self, name: &str, fields: Value) {
        let mut f = fields;
        f["session"] = self.id.clone().into();
        let ev = event(name, f);
        self.inner.lock().expect("session").pending_prompt = Some(ev.clone());
        if let Some(c) = self.client() {
            if !c.send(&ev).await {
                self.detach_if(c.id);
            }
        }
    }

    async fn prompt_done(&self) {
        self.inner.lock().expect("session").pending_prompt = None;
    }
}

/// Создать сессию и запустить её задачу. Ответ клиенту — сразу, ход — событиями.
pub fn spawn(state: &Arc<DaemonState>, client: ClientHandle, p: OpenParams) -> Arc<TermSession> {
    let (tx, rx) = mpsc::unbounded_channel();
    let sess = Arc::new(TermSession {
        id: state.new_id("s"),
        connection_id: p.connection_id.clone(),
        opened_at: crate::util::now(),
        inner: Mutex::new(Inner {
            attached: Some(client),
            state: "connecting",
            cols: p.cols,
            rows: p.rows,
            ..Default::default()
        }),
        ctrl: tx,
    });
    state
        .sessions
        .lock()
        .expect("sessions")
        .insert(sess.id.clone(), sess.clone());
    let (st, s) = (state.clone(), sess.clone());
    tokio::spawn(async move { run(st, s, rx, p).await });
    sess
}

async fn finish(state: &DaemonState, sess: &TermSession, reason: &str, exit: Option<u32>, signal: Option<String>) {
    sess.set_state("closed");
    let mut f = json!({ "reason": reason });
    if let Some(e) = exit {
        f["exit_status"] = e.into();
    }
    if let Some(s) = signal {
        f["signal"] = s.into();
    }
    sess.emit("closed", f).await;
    state.sessions.lock().expect("sessions").remove(&sess.id);
    state
        .prompts
        .lock()
        .expect("prompts")
        .retain(|_, p| p.session.as_deref() != Some(&sess.id));
    state.touch();
}

async fn fatal(state: &DaemonState, sess: &TermSession, e: CoreError) {
    sess.emit("error", json!({ "error": e, "fatal": true })).await;
    finish(state, sess, "error", None, None).await;
}

async fn run(state: Arc<DaemonState>, sess: Arc<TermSession>, mut ctrl: mpsc::UnboundedReceiver<Ctrl>, p: OpenParams) {
    sess.emit("connecting", json!({ "connection_id": p.connection_id }))
        .await;
    let prompter = EventPrompter {
        state: &state,
        sink: sess.as_ref(),
        session: Some(sess.id.clone()),
    };
    let conn = match connect::pooled(&state, &p.connection_id, p.secrets.clone(), &prompter).await {
        Ok(c) => c,
        Err(e) => return fatal(&state, &sess, e).await,
    };
    let channel_err = |e: russh::Error| {
        CoreError::new("ssh.channel_failed", format!("Cannot open terminal: {e}")).with("reason", e.to_string())
    };
    let ch = match conn.handle().channel_open_session().await {
        Ok(c) => c,
        Err(e) => return fatal(&state, &sess, channel_err(e)).await,
    };
    if let Err(e) = ch.request_pty(true, &p.term, p.cols, p.rows, 0, 0, &[]).await {
        return fatal(&state, &sess, channel_err(e)).await;
    }
    for (k, v) in &p.env {
        let _ = ch.set_env(false, k.as_str(), v.as_str()).await;
    }
    if let Err(e) = ch.request_shell(true).await {
        return fatal(&state, &sess, channel_err(e)).await;
    }
    conn.terminal_opened();
    sess.set_state("connected");
    sess.emit(
        "connected",
        json!({ "connection_id": p.connection_id, "server_banner": conn.server_banner }),
    )
    .await;

    let (mut rd, wr) = ch.split();
    let mut pending: Option<Value> = None;
    let mut leftover: Option<ChannelMsg> = None;
    let (mut exit, mut signal) = (None::<u32>, None::<String>);
    let mut reason = "remote";

    'main: loop {
        // 1. Отдать накопленный вывод. Пока клиент не принял — SSH не читаем.
        if let Some(ev) = pending.take() {
            if let Some(c) = sess.client() {
                tokio::select! {
                    permit = c.tx.reserve() => match permit {
                        Ok(pm) => pm.send(ev.to_string()),
                        Err(_) => sess.detach_if(c.id),
                    },
                    cmd = ctrl.recv() => {
                        pending = Some(ev);
                        match handle_ctrl(&wr, cmd).await {
                            Ok(()) => {}
                            Err(r) => { reason = r; break 'main; }
                        }
                    }
                }
            }
            continue;
        }
        // 2. Читать канал или управляющие команды.
        let msg = match leftover.take() {
            Some(m) => Some(m),
            None => tokio::select! {
                m = rd.wait() => m,
                cmd = ctrl.recv() => {
                    match handle_ctrl(&wr, cmd).await {
                        Ok(()) => continue,
                        Err(r) => { reason = r; break 'main; }
                    }
                }
            },
        };
        match msg {
            None | Some(ChannelMsg::Close) => break,
            Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                let mut buf = data.to_vec();
                // Склеить то, что пришло за ~10 мс.
                let deadline = tokio::time::Instant::now() + COALESCE;
                while buf.len() < CHUNK {
                    match tokio::time::timeout_at(deadline, rd.wait()).await {
                        Ok(Some(ChannelMsg::Data { data })) | Ok(Some(ChannelMsg::ExtendedData { data, .. })) => {
                            buf.extend_from_slice(&data)
                        }
                        Ok(Some(other)) => {
                            leftover = Some(other);
                            break;
                        }
                        Ok(None) => {
                            leftover = Some(ChannelMsg::Close);
                            break;
                        }
                        Err(_) => break,
                    }
                }
                for part in buf.chunks(CHUNK) {
                    if let Some(ev) = sess.record_output(part) {
                        // Несколько частей подряд — отдаём по одной через pending.
                        match pending.take() {
                            None => pending = Some(ev),
                            Some(prev) => {
                                if let Some(c) = sess.client() {
                                    if !c.send(&prev).await {
                                        sess.detach_if(c.id);
                                    }
                                }
                                pending = Some(ev);
                            }
                        }
                    }
                }
            }
            Some(ChannelMsg::ExitStatus { exit_status }) => exit = Some(exit_status),
            Some(ChannelMsg::ExitSignal { signal_name, .. }) => signal = Some(format!("{signal_name:?}")),
            Some(_) => {}
        }
    }
    if let Some(ev) = pending.take() {
        if let Some(c) = sess.client() {
            let _ = c.send(&ev).await;
        }
    }
    if reason == "remote" && (exit.is_some() || signal.is_some()) {
        reason = "exit";
    }
    let _ = wr.close().await;
    conn.terminal_closed();
    finish(&state, &sess, reason, exit, signal).await;
}

/// Обработать команду. `Err(reason)` — сессию надо завершить.
async fn handle_ctrl<S>(wr: &russh::ChannelWriteHalf<S>, cmd: Option<Ctrl>) -> Result<(), &'static str>
where
    S: From<(russh::ChannelId, ChannelMsg)> + Send + Sync + 'static,
{
    match cmd {
        None => Err("client"),
        Some(Ctrl::Input(data)) => wr.data(&data[..]).await.map_err(|_| "error"),
        Some(Ctrl::Resize(c, r)) => {
            let _ = wr.window_change(c, r, 0, 0).await;
            Ok(())
        }
        Some(Ctrl::Close(reason)) => Err(reason),
    }
}

impl TermSession {
    pub fn set_size(&self, cols: u32, rows: u32) {
        let mut g = self.inner.lock().expect("session");
        g.cols = cols;
        g.rows = rows;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_keeps_last_bytes() {
        let mut t = Tail::default();
        t.push(b"hello ");
        t.push(b"world");
        assert_eq!(t.snapshot(), b"hello world");
        let big = vec![b'x'; TAIL_CAP + 10];
        t.push(&big);
        assert_eq!(t.snapshot().len(), TAIL_CAP);
        t.push(b"END");
        let s = t.snapshot();
        assert_eq!(s.len(), TAIL_CAP);
        assert!(s.ends_with(b"END"));
    }
}
