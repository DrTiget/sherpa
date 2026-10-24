//! Мультиплексирование: одно SSH-соединение на подключение, внутри — каналы
//! терминалов и утилит. Тут же — привилегии (sudo) и режим «только чтение».
//! Пул живёт в демоне; утилиты не открывают соединений сами.

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use russh::client::Handle;
use serde_json::{json, Value};
use tokio::sync::Semaphore;

use crate::error::{CoreError, CoreResult};
use crate::secret::Secret;
use crate::ssh::client::{Session, Verifier};
use crate::ssh::exec::{exec_on_io, shell_quote, ExecOutput, OutputTx, RemoteExec};

/// Лимит одновременных каналов `exec` утилит на одно соединение.
pub const DEFAULT_MAX_CHANNELS: usize = 8;

#[derive(Debug, Default, Clone, Copy)]
struct PrivState {
    uid: Option<u32>,
    sudo_present: Option<bool>,
    nopasswd: Option<bool>,
}

pub struct PooledConn {
    pub connection_id: String,
    session: Session,
    channels: Arc<Semaphore>,
    max_channels: usize,
    pub server_banner: Option<String>,
    pub opened_at: String,
    last_used: Mutex<Instant>,
    /// Кэш `desktop.capabilities`.
    pub capabilities: tokio::sync::Mutex<Option<Value>>,
    sudo_password: Mutex<Option<Secret>>,
    privs: Mutex<PrivState>,
    readonly: AtomicBool,
    terminals: AtomicUsize,
}

impl PooledConn {
    pub fn new(connection_id: &str, session: Session, server_banner: Option<String>, max_channels: usize) -> Self {
        Self {
            connection_id: connection_id.to_string(),
            session,
            channels: Arc::new(Semaphore::new(max_channels.max(1))),
            max_channels: max_channels.max(1),
            server_banner,
            opened_at: crate::util::now(),
            last_used: Mutex::new(Instant::now()),
            capabilities: tokio::sync::Mutex::new(None),
            sudo_password: Mutex::new(None),
            privs: Mutex::new(PrivState::default()),
            readonly: AtomicBool::new(false),
            terminals: AtomicUsize::new(0),
        }
    }

    pub fn forwards(&self) -> &crate::ssh::client::ForwardRoutes {
        &self.session.forwards
    }

    pub fn handle(&self) -> &Handle<Verifier> {
        &self.session.handle
    }

    pub fn is_alive(&self) -> bool {
        !self.session.handle.is_closed()
    }

    pub fn touch(&self) {
        *self.last_used.lock().expect("last_used") = Instant::now();
    }

    pub fn idle_for(&self) -> Duration {
        self.last_used.lock().expect("last_used").elapsed()
    }

    /// Учёт открытых терминалов (соединение с терминалами не закрывается по простою).
    pub fn terminal_opened(&self) {
        self.terminals.fetch_add(1, Ordering::SeqCst);
        self.touch();
    }

    pub fn terminal_closed(&self) {
        self.terminals.fetch_sub(1, Ordering::SeqCst);
        self.touch();
    }

    pub fn terminals(&self) -> usize {
        self.terminals.load(Ordering::SeqCst)
    }

    pub fn channels_in_use(&self) -> usize {
        self.max_channels.saturating_sub(self.channels.available_permits())
    }

    pub async fn channel_permit(&self) -> CoreResult<tokio::sync::OwnedSemaphorePermit> {
        self.touch();
        self.channels
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| CoreError::internal("channel semaphore closed"))
    }

    pub async fn sftp(&self) -> CoreResult<SftpChannel> {
        let permit = self.channel_permit().await?;
        let channel = self
            .handle()
            .channel_open_session()
            .await
            .map_err(|_| CoreError::new("fs.channel", "Cannot open SFTP channel"))?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|_| CoreError::new("fs.channel", "Cannot request SFTP subsystem"))?;
        let session = russh_sftp::client::SftpSession::new(channel.into_stream())
            .await
            .map_err(crate::daemon::methods::files::sftp_error)?;
        Ok(SftpChannel {
            session: Arc::new(session),
            permit: Some(permit),
        })
    }

    // ---------- привилегии ----------

    pub fn set_readonly(&self, on: bool) {
        self.readonly.store(on, Ordering::SeqCst);
    }

    pub fn readonly(&self) -> bool {
        self.readonly.load(Ordering::SeqCst)
    }

    /// Утилиты вызывают перед любым изменением на сервере.
    pub fn ensure_writable(&self) -> CoreResult<()> {
        if self.readonly() {
            return Err(CoreError::new("priv.readonly", "Connection is in read-only mode")
                .with("connection_id", self.connection_id.clone()));
        }
        Ok(())
    }

    async fn probe_privs(&self) -> CoreResult<PrivState> {
        let cur = *self.privs.lock().expect("privs");
        if cur.uid.is_some() && cur.sudo_present.is_some() && cur.nopasswd.is_some() {
            return Ok(cur);
        }
        let out = self
            .exec("id -u; if command -v sudo >/dev/null 2>&1; then echo sudo=1; sudo -n true >/dev/null 2>&1 && echo nopasswd=1; fi; true")
            .await?;
        let text = out.stdout_lossy();
        let mut st = PrivState {
            uid: text.lines().next().and_then(|l| l.trim().parse().ok()),
            sudo_present: Some(text.contains("sudo=1")),
            nopasswd: Some(text.contains("nopasswd=1")),
        };
        if st.uid.is_none() {
            st.uid = Some(u32::MAX);
        }
        *self.privs.lock().expect("privs") = st;
        Ok(st)
    }

    pub async fn privilege_status(&self) -> CoreResult<Value> {
        let st = self.probe_privs().await?;
        Ok(json!({
            "connection_id": self.connection_id,
            "is_root": st.uid == Some(0),
            "sudo_available": st.sudo_present.unwrap_or(false),
            "sudo_nopasswd": st.nopasswd.unwrap_or(false),
            "sudo_authenticated": self.sudo_password.lock().expect("sudo").is_some(),
            "readonly": self.readonly(),
        }))
    }

    /// Проверить пароль sudo и держать его в памяти демона до закрытия соединения.
    pub async fn sudo_auth(&self, password: Secret) -> CoreResult<Value> {
        let st = self.probe_privs().await?;
        if !st.sudo_present.unwrap_or(false) {
            return Err(CoreError::new("cap.missing", "sudo is not installed").with("capability", "sudo"));
        }
        let input = format!("{}\n", password.expose());
        let out = self.exec_input("sudo -S -k -p '' -v", Some(input.as_bytes())).await?;
        if !out.success() {
            return Err(CoreError::new("priv.auth_failed", "sudo rejected the password")
                .with("connection_id", self.connection_id.clone()));
        }
        *self.sudo_password.lock().expect("sudo") = Some(password);
        self.privilege_status().await
    }

    pub fn sudo_forget(&self) {
        *self.sudo_password.lock().expect("sudo") = None;
    }

    /// Выполнить команду с правами root: напрямую, `sudo -n` или `sudo -S` с паролем.
    pub async fn exec_privileged(&self, command: &str) -> CoreResult<ExecOutput> {
        self.exec_privileged_io(command, None, None).await
    }

    /// То же с `input` в stdin команды и потоковым выводом (`stream`, см. `exec_on_io`).
    /// Пароль sudo, если нужен, передаётся первой строкой stdin перед `input`.
    pub async fn exec_privileged_io(
        &self,
        command: &str,
        input: Option<&[u8]>,
        stream: Option<OutputTx>,
    ) -> CoreResult<ExecOutput> {
        let st = self.probe_privs().await?;
        if st.uid == Some(0) {
            return self.exec_io(command, input, stream).await;
        }
        let wrapped = shell_quote(command);
        if st.nopasswd.unwrap_or(false) {
            return self
                .exec_io(&format!("sudo -n -- sh -c {wrapped}"), input, stream)
                .await;
        }
        let pw = self.sudo_password.lock().expect("sudo").clone();
        match pw {
            Some(pw) => {
                let mut data = format!("{}\n", pw.expose()).into_bytes();
                data.extend_from_slice(input.unwrap_or_default());
                self.exec_io(&format!("sudo -S -p '' -- sh -c {wrapped}"), Some(&data), stream)
                    .await
            }
            None => Err(CoreError::new("priv.required", "Root privileges are required")
                .with("connection_id", self.connection_id.clone())
                .with("sudo_available", st.sudo_present.unwrap_or(false))),
        }
    }

    async fn exec_io(&self, command: &str, input: Option<&[u8]>, stream: Option<OutputTx>) -> CoreResult<ExecOutput> {
        let _permit = self
            .channels
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| CoreError::internal("channel semaphore closed"))?;
        self.touch();
        let r = exec_on_io(self.handle(), command, input, stream).await;
        self.touch();
        r
    }

    pub fn info(&self) -> Value {
        json!({
            "connection_id": self.connection_id,
            "opened_at": self.opened_at,
            "alive": self.is_alive(),
            "terminals": self.terminals(),
            "channels_in_use": self.channels_in_use(),
            "readonly": self.readonly(),
            "server_banner": self.server_banner,
        })
    }
}

impl RemoteExec for PooledConn {
    async fn exec_input(&self, command: &str, input: Option<&[u8]>) -> CoreResult<ExecOutput> {
        self.exec_io(command, input, None).await
    }
}

type Slot = Arc<tokio::sync::Mutex<Option<Arc<PooledConn>>>>;

/// Пул соединений демона по id подключения.
pub struct Pool {
    slots: Mutex<HashMap<String, Slot>>,
    max_channels: usize,
}

impl Default for Pool {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_CHANNELS)
    }
}

impl Pool {
    pub fn new(max_channels: usize) -> Self {
        Self {
            slots: Mutex::new(HashMap::new()),
            max_channels,
        }
    }

    fn slot(&self, id: &str) -> Slot {
        self.slots
            .lock()
            .expect("pool")
            .entry(id.to_string())
            .or_default()
            .clone()
    }

    /// Живое соединение или новое через `connect` (параллельные вызовы ждут одно).
    pub async fn get_or_connect<F, Fut>(&self, id: &str, connect: F) -> CoreResult<Arc<PooledConn>>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = CoreResult<(Session, Option<String>)>>,
    {
        let slot = self.slot(id);
        let mut guard = slot.lock().await;
        if let Some(c) = guard.as_ref() {
            if c.is_alive() {
                c.touch();
                return Ok(c.clone());
            }
        }
        let (session, banner) = connect().await?;
        let c = Arc::new(PooledConn::new(id, session, banner, self.max_channels));
        *guard = Some(c.clone());
        Ok(c)
    }

    /// Уже открытое живое соединение (без подключения).
    pub async fn get(&self, id: &str) -> Option<Arc<PooledConn>> {
        let slot = self.slots.lock().expect("pool").get(id).cloned()?;
        let g = slot.lock().await;
        g.as_ref().filter(|c| c.is_alive()).cloned()
    }

    pub async fn close(&self, id: &str) -> bool {
        let slot = self.slots.lock().expect("pool").remove(id);
        match slot {
            Some(s) => {
                let c = s.lock().await.take();
                if let Some(c) = c {
                    let _ = c
                        .handle()
                        .disconnect(russh::Disconnect::ByApplication, "bye", "en")
                        .await;
                    true
                } else {
                    false
                }
            }
            None => false,
        }
    }

    pub async fn list(&self) -> Vec<Value> {
        let slots: Vec<Slot> = self.slots.lock().expect("pool").values().cloned().collect();
        let mut out = Vec::new();
        for s in slots {
            if let Some(c) = s.lock().await.as_ref() {
                out.push(c.info());
            }
        }
        out
    }

    /// Закрыть соединения без терминалов и каналов, простаивающие дольше `max_idle`.
    pub async fn reap_idle(&self, max_idle: Duration) {
        let ids: Vec<String> = self.slots.lock().expect("pool").keys().cloned().collect();
        for id in ids {
            if let Some(c) = self.get(&id).await {
                if c.terminals() == 0 && c.channels_in_use() == 0 && c.idle_for() > max_idle {
                    self.close(&id).await;
                }
            } else {
                self.slots.lock().expect("pool").remove(&id);
            }
        }
    }

    pub async fn close_all(&self) {
        let ids: Vec<String> = self.slots.lock().expect("pool").keys().cloned().collect();
        for id in ids {
            self.close(&id).await;
        }
    }
}

/// Permit lives until the subsystem has been closed, including cancellation.
pub struct SftpChannel {
    session: Arc<russh_sftp::client::SftpSession>,
    permit: Option<tokio::sync::OwnedSemaphorePermit>,
}
impl std::ops::Deref for SftpChannel {
    type Target = russh_sftp::client::SftpSession;
    fn deref(&self) -> &Self::Target {
        &self.session
    }
}
impl Drop for SftpChannel {
    fn drop(&mut self) {
        let session = self.session.clone();
        let permit = self.permit.take();
        tokio::spawn(async move {
            let _permit = permit;
            let _ = session.close().await;
        });
    }
}
