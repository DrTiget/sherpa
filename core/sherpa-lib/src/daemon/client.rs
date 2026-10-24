//! Клиент протокола демона: для CLI (`daemon status/stop`), тестов и как
//! образец для фронтов.

use std::collections::VecDeque;
use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf};

use super::transport::{self, BoxStream};
use crate::error::{CoreError, CoreResult};
use crate::protocol::PROTOCOL;

pub struct Client {
    reader: BufReader<ReadHalf<BoxStream>>,
    writer: WriteHalf<BoxStream>,
    next_id: u64,
    events: VecDeque<Value>,
    pub hello: Value,
}

fn io_err(e: std::io::Error) -> CoreError {
    CoreError::new("daemon.unreachable", format!("Daemon connection failed: {e}")).with("endpoint", "")
}

impl Client {
    /// Подключиться и выполнить `hello`.
    pub async fn connect(endpoint: &Path) -> CoreResult<Client> {
        let s = transport::connect(endpoint).await.map_err(|e| {
            CoreError::new("daemon.not_running", format!("Daemon is not running: {e}"))
                .with("endpoint", endpoint.display().to_string())
        })?;
        let (r, w) = tokio::io::split(s);
        let mut c = Client {
            reader: BufReader::new(r),
            writer: w,
            next_id: 1,
            events: VecDeque::new(),
            hello: Value::Null,
        };
        c.hello = c
            .request("hello", json!({ "protocol": PROTOCOL, "client": "sherpa-core-client" }))
            .await?;
        Ok(c)
    }

    pub async fn send_raw(&mut self, line: &str) -> CoreResult<()> {
        self.writer.write_all(line.as_bytes()).await.map_err(io_err)?;
        self.writer.write_all(b"\n").await.map_err(io_err)?;
        self.writer.flush().await.map_err(io_err)
    }

    async fn read_msg(&mut self) -> CoreResult<Value> {
        let mut line = String::new();
        let n = self.reader.read_line(&mut line).await.map_err(io_err)?;
        if n == 0 {
            return Err(CoreError::new("daemon.unreachable", "Daemon closed the connection").with("endpoint", ""));
        }
        serde_json::from_str(&line).map_err(|e| CoreError::internal(format!("bad daemon message: {e}")))
    }

    /// Отправить запрос, не дожидаясь ответа. Возвращает id.
    pub async fn send_request(&mut self, method: &str, params: Value) -> CoreResult<u64> {
        let id = self.next_id;
        self.next_id += 1;
        let msg = json!({ "id": id, "method": method, "params": params });
        self.send_raw(&msg.to_string()).await?;
        Ok(id)
    }

    /// Следующее сообщение (событие или ответ) без разбора.
    pub async fn next_message(&mut self, timeout: Duration) -> Option<Value> {
        if let Some(e) = self.events.pop_front() {
            return Some(e);
        }
        tokio::time::timeout(timeout, self.read_msg()).await.ok()?.ok()
    }

    /// Запрос и ожидание ответа; события по дороге складываются в очередь.
    pub async fn request(&mut self, method: &str, params: Value) -> CoreResult<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let msg = json!({ "id": id, "method": method, "params": params });
        self.send_raw(&msg.to_string()).await?;
        loop {
            let m = self.read_msg().await?;
            if m.get("event").is_some() {
                self.events.push_back(m);
                continue;
            }
            if m["id"] == id {
                if m["ok"] == true {
                    return Ok(m["data"].clone());
                }
                let e: CoreError = serde_json::from_value(m["error"].clone())
                    .unwrap_or_else(|_| CoreError::internal(format!("bad error: {m}")));
                return Err(e);
            }
        }
    }

    /// Следующее событие (сначала из очереди) или `None` по таймауту.
    pub async fn next_event(&mut self, timeout: Duration) -> Option<Value> {
        if let Some(e) = self.events.pop_front() {
            return Some(e);
        }
        loop {
            let m = tokio::time::timeout(timeout, self.read_msg()).await.ok()?.ok()?;
            if m.get("event").is_some() {
                return Some(m);
            }
        }
    }

    /// Ждать событие с данным именем (остальные пропускаются, кроме `keep`).
    pub async fn wait_event(&mut self, name: &str, timeout: Duration) -> Option<Value> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            if left.is_zero() {
                return None;
            }
            let e = self.next_event(left).await?;
            if e["event"] == name {
                return Some(e);
            }
        }
    }
}
