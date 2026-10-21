use russh::{client::Msg, Channel, ChannelMsg};
use serde::Serialize;
use std::{collections::HashMap, sync::{Arc, Mutex}};
use tokio::sync::mpsc;

/// События терминала для фронта. Данные — сырые байты (UTF-8 декодирует xterm.js, поэтому границы чанков безопасны).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum TermEvent {
    Data { data: Vec<u8> },
    /// Shell завершился или соединение упало.
    Closed { exit_code: Option<u32>, reason: String },
}

enum Cmd {
    Write(Vec<u8>),
    Resize(u32, u32),
    Close,
}

#[derive(Clone)]
pub struct TerminalHandle {
    tx: mpsc::UnboundedSender<Cmd>,
}

impl TerminalHandle {
    pub fn write(&self, data: Vec<u8>) -> bool {
        self.tx.send(Cmd::Write(data)).is_ok()
    }
    pub fn resize(&self, cols: u32, rows: u32) -> bool {
        self.tx.send(Cmd::Resize(cols, rows)).is_ok()
    }
    pub fn close(&self) {
        let _ = self.tx.send(Cmd::Close);
    }
}

/// terminal_id -> (server_id, handle)
pub type Registry = Arc<Mutex<HashMap<String, (String, TerminalHandle)>>>;

/// Запускает задачу, которая гоняет данные между каналом и фронтом. Возвращает хэндл управления.
pub fn spawn(
    mut ch: Channel<Msg>,
    terminal_id: String,
    events: mpsc::UnboundedSender<TermEvent>,
    registry: Registry,
) -> TerminalHandle {
    let (tx, mut rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let mut exit_code = None;
        let mut reason = "соединение закрыто".to_string();
        loop {
            tokio::select! {
                cmd = rx.recv() => match cmd {
                    Some(Cmd::Write(d)) => {
                        if ch.data_bytes(d).await.is_err() { reason = "ошибка записи".into(); break; }
                    }
                    Some(Cmd::Resize(c, r)) => { let _ = ch.window_change(c, r, 0, 0).await; }
                    Some(Cmd::Close) | None => { reason = "закрыт пользователем".into(); let _ = ch.close().await; break; }
                },
                msg = ch.wait() => match msg {
                    Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                        if events.send(TermEvent::Data { data: data.to_vec() }).is_err() { let _ = ch.close().await; break; }
                    }
                    Some(ChannelMsg::ExitStatus { exit_status }) => exit_code = Some(exit_status),
                    Some(ChannelMsg::Close) | None => { if exit_code.is_some() { reason = "shell завершился".into(); } break; }
                    _ => {}
                },
            }
        }
        registry.lock().unwrap().remove(&terminal_id);
        let _ = events.send(TermEvent::Closed { exit_code, reason });
    });
    TerminalHandle { tx }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_serialize_for_frontend() {
        let d = serde_json::to_string(&TermEvent::Data { data: vec![0xd0, 0x9f] }).unwrap();
        assert_eq!(d, r#"{"event":"data","data":[208,159]}"#);
        let c = serde_json::to_string(&TermEvent::Closed { exit_code: Some(0), reason: "x".into() }).unwrap();
        assert_eq!(c, r#"{"event":"closed","exit_code":0,"reason":"x"}"#);
    }
}
