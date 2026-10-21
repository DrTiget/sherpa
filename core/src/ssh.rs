use crate::{
    enhanced,
    known_hosts::{fingerprint, HostCheck, KnownHosts},
    model::{AuthConfig, ExecResult, Server},
    secrets::SecretStore,
    monitor::{self, MonitorEvent},
    terminal::{self, Registry, TermEvent, TerminalHandle},
    CoreError, CoreResult,
};
use russh::{
    client::{self, AuthResult, Handle},
    keys::{PrivateKey, PrivateKeyWithHashAlg, PublicKey},
    ChannelMsg,
};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::Mutex;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ShellStatus {
    /// "zsh" | "bash" | "default"
    pub shell: String,
    /// true — файлы были (пере)залиты в этот раз.
    pub uploaded: bool,
    /// Команда установки zsh, если его на сервере нет.
    pub install_hint: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TerminalInfo {
    pub terminal_id: String,
    pub shell: String,
    pub install_hint: Option<String>,
}

/// Результат проверки ключа хоста, который handler передаёт наружу.
#[derive(Debug, Clone)]
enum HostVerdict {
    Unknown(PublicKey),
    Mismatch { expected: String, actual: String },
}

struct ClientHandler {
    host: String,
    port: u16,
    known: Arc<KnownHosts>,
    verdict: Arc<std::sync::Mutex<Option<HostVerdict>>>,
}

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(&mut self, key: &russh::keys::PublicKeyOrCertificate) -> Result<bool, Self::Error> {
        let pk = match key {
            russh::keys::PublicKeyOrCertificate::PublicKey { key, .. } => key.clone(),
            russh::keys::PublicKeyOrCertificate::Certificate(c) => PublicKey::from(c.public_key().clone()),
        };
        match self.known.check(&self.host, self.port, &pk) {
            HostCheck::Trusted => Ok(true),
            HostCheck::Unknown => {
                *self.verdict.lock().unwrap() = Some(HostVerdict::Unknown(pk));
                Ok(false)
            }
            HostCheck::Mismatch { expected } => {
                *self.verdict.lock().unwrap() = Some(HostVerdict::Mismatch { expected, actual: fingerprint(&pk) });
                Ok(false)
            }
        }
    }
}

type Session = Arc<Handle<ClientHandler>>;

/// Пул активных SSH-сессий + ожидающие подтверждения ключи хостов.
pub struct SshPool {
    known: Arc<KnownHosts>,
    sessions: Mutex<HashMap<String, Session>>,
    pending: std::sync::Mutex<HashMap<(String, u16), PublicKey>>,
    terminals: Registry,
    monitors: monitor::Registry,
}

impl SshPool {
    pub fn new(known: KnownHosts) -> Self {
        Self { known: Arc::new(known), sessions: Mutex::new(HashMap::new()), pending: Default::default(), terminals: Default::default(), monitors: Default::default() }
    }

    pub fn known_hosts(&self) -> &KnownHosts {
        &self.known
    }

    /// Подтверждение отпечатка пользователем: сохраняет ожидающий ключ, только если отпечаток совпал.
    pub fn trust_host(&self, host: &str, port: u16, fp: &str) -> CoreResult<()> {
        let key = self.pending.lock().unwrap().get(&(host.to_lowercase(), port)).cloned();
        match key {
            Some(k) if fingerprint(&k) == fp => {
                self.known.trust(host, port, &k)?;
                self.pending.lock().unwrap().remove(&(host.to_lowercase(), port));
                Ok(())
            }
            Some(_) => Err(CoreError::msg("Отпечаток изменился с момента запроса — повторите подключение")),
            None => Err(CoreError::msg("Нет ожидающего подтверждения для этого хоста")),
        }
    }

    pub async fn is_connected(&self, id: &str) -> bool {
        self.sessions.lock().await.get(id).is_some_and(|s| !s.is_closed())
    }

    pub async fn connected_ids(&self) -> Vec<String> {
        self.sessions.lock().await.iter().filter(|(_, s)| !s.is_closed()).map(|(k, _)| k.clone()).collect()
    }

    /// Устанавливает новое соединение и аутентифицируется (в пул не кладёт).
    async fn open(&self, server: &Server, secrets: &dyn SecretStore) -> CoreResult<Handle<ClientHandler>> {
        let config = Arc::new(client::Config {
            keepalive_interval: Some(Duration::from_secs(30)),
            keepalive_max: 3,
            ..Default::default()
        });
        let verdict = Arc::new(std::sync::Mutex::new(None));
        let handler = ClientHandler { host: server.host.clone(), port: server.port, known: self.known.clone(), verdict: verdict.clone() };
        let addr = (server.host.as_str(), server.port);
        let connect = tokio::time::timeout(Duration::from_secs(15), client::connect(config, addr, handler))
            .await
            .map_err(|_| CoreError::msg(format!("Таймаут подключения к {}:{}", server.host, server.port)))?;
        let mut handle = match connect {
            Ok(h) => h,
            Err(e) => {
                let v = verdict.lock().unwrap().take();
                return Err(match v {
                    Some(HostVerdict::Unknown(k)) => {
                        let err = CoreError::UnknownHost {
                            host: server.host.clone(),
                            port: server.port,
                            fingerprint: fingerprint(&k),
                            algorithm: k.algorithm().to_string(),
                        };
                        self.pending.lock().unwrap().insert((server.host.to_lowercase(), server.port), k);
                        err
                    }
                    Some(HostVerdict::Mismatch { expected, actual }) => {
                        CoreError::HostKeyMismatch { host: server.host.clone(), port: server.port, expected, actual }
                    }
                    None => CoreError::msg(format!("Не удалось подключиться к {}:{}: {e}", server.host, server.port)),
                });
            }
        };
        let res = self.authenticate(&mut handle, server, secrets).await?;
        if !res {
            return Err(CoreError::AuthFailed { message: "сервер отклонил учётные данные".into() });
        }
        Ok(handle)
    }

    async fn authenticate(&self, h: &mut Handle<ClientHandler>, s: &Server, secrets: &dyn SecretStore) -> CoreResult<bool> {
        let user = s.username.clone();
        let ok = |r: AuthResult| matches!(r, AuthResult::Success);
        match &s.auth {
            AuthConfig::Password { password_ref } => {
                let pw = match password_ref {
                    Some(r) => secrets.get(r)?.ok_or_else(|| CoreError::msg("Пароль не найден в системном хранилище"))?,
                    None => return Err(CoreError::msg("Пароль не задан")),
                };
                Ok(ok(h.authenticate_password(user, pw).await?))
            }
            AuthConfig::Key { key_path, key_ref, passphrase_ref } => {
                let pass = match passphrase_ref {
                    Some(r) => secrets.get(r)?,
                    None => None,
                };
                let key: PrivateKey = if let Some(r) = key_ref {
                    let pem = secrets.get(r)?.ok_or_else(|| CoreError::msg("Импортированный ключ не найден в системном хранилище"))?;
                    russh::keys::decode_secret_key(&pem, pass.as_deref())?
                } else if let Some(p) = key_path {
                    russh::keys::load_secret_key(expand_home(p), pass.as_deref())
                        .map_err(|e| CoreError::msg(format!("Не удалось прочитать ключ {p}: {e}")))?
                } else {
                    return Err(CoreError::msg("Ключ не указан"));
                };
                let hash = h.best_supported_rsa_hash().await?.flatten();
                Ok(ok(h.authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key), hash)).await?))
            }
            AuthConfig::Agent => {
                let mut agent = russh::keys::agent::client::AgentClient::connect_env()
                    .await
                    .map_err(|e| CoreError::msg(format!("ssh-agent недоступен: {e}")))?;
                let ids = agent.request_identities().await?;
                if ids.is_empty() {
                    return Err(CoreError::msg("В ssh-agent нет ключей"));
                }
                let hash = h.best_supported_rsa_hash().await?.flatten();
                for id in ids {
                    let pk = id.public_key();
                    let r = h
                        .authenticate_publickey_with(user.clone(), pk.into_owned(), hash, &mut agent)
                        .await
                        .map_err(|e| CoreError::msg(e.to_string()))?;
                    if ok(r) {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }

    /// Проверка: подключиться, выполнить `true`-подобную команду и закрыть. Пул не затрагивает.
    pub async fn test_connection(&self, server: &Server, secrets: &dyn SecretStore) -> CoreResult<()> {
        let h = self.open(server, secrets).await?;
        let _ = h.disconnect(russh::Disconnect::ByApplication, "", "en").await;
        Ok(())
    }

    pub async fn connect(&self, server: &Server, secrets: &dyn SecretStore) -> CoreResult<()> {
        if self.is_connected(&server.id).await {
            return Ok(());
        }
        let h = self.open(server, secrets).await?;
        self.sessions.lock().await.insert(server.id.clone(), Arc::new(h));
        Ok(())
    }

    pub async fn disconnect(&self, id: &str) -> CoreResult<()> {
        self.close_server_terminals(id);
        self.close_server_monitors(id);
        if let Some(s) = self.sessions.lock().await.remove(id) {
            let _ = s.disconnect(russh::Disconnect::ByApplication, "", "en").await;
        }
        Ok(())
    }

    fn close_server_terminals(&self, server_id: &str) {
        let hs: Vec<_> = self.terminals.lock().unwrap().values().filter(|(s, _)| s == server_id).map(|(_, h)| h.clone()).collect();
        for h in hs {
            h.close();
        }
    }

    fn close_server_monitors(&self, server_id: &str) {
        let hs: Vec<_> = self.monitors.lock().unwrap().values().filter(|(s, _)| s == server_id).map(|(_, h)| h.clone()).collect();
        for h in hs {
            h.close();
        }
    }

    /// Запускает сбор метрик (долгоживущий exec-канал) на соединении из пула. Возвращает id монитора и поток событий.
    pub async fn monitor_start(
        &self,
        server: &Server,
        secrets: &dyn SecretStore,
        interval_ms: u64,
    ) -> CoreResult<(String, tokio::sync::mpsc::UnboundedReceiver<MonitorEvent>)> {
        self.connect(server, secrets).await?;
        let session = self.sessions.lock().await.get(&server.id).cloned().ok_or(CoreError::NotFound)?;
        let ch = match session.channel_open_session().await {
            Ok(c) => c,
            Err(e) => {
                self.sessions.lock().await.remove(&server.id);
                return Err(CoreError::msg(format!("Сессия потеряна: {e}")));
            }
        };
        ch.exec(true, monitor::command(interval_ms).into_bytes()).await?;
        let id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let h = monitor::spawn(ch, id.clone(), tx, self.monitors.clone());
        self.monitors.lock().unwrap().insert(id.clone(), (server.id.clone(), h));
        Ok((id, rx))
    }

    pub fn monitor_stop(&self, monitor_id: &str) {
        let h = self.monitors.lock().unwrap().get(monitor_id).map(|(_, h)| h.clone());
        if let Some(h) = h {
            h.close();
        }
    }

    /// Послать сигнал процессу на сервере. Ошибка (нет прав, нет процесса) возвращается текстом.
    pub async fn process_kill(&self, server: &Server, secrets: &dyn SecretStore, pid: u32, signal: &str) -> CoreResult<()> {
        let cmd = monitor::kill_command(pid, signal)?;
        let r = self.exec(server, secrets, &cmd).await?;
        if r.exit_code == Some(0) {
            Ok(())
        } else {
            let t = format!("{}{}", r.stdout, r.stderr);
            let t = t.trim();
            Err(CoreError::msg(if t.is_empty() { format!("kill завершился с кодом {:?}", r.exit_code) } else { t.to_string() }))
        }
    }

    fn terminal(&self, tid: &str) -> CoreResult<TerminalHandle> {
        self.terminals.lock().unwrap().get(tid).map(|(_, h)| h.clone()).ok_or_else(|| CoreError::msg("Терминал не найден или уже закрыт"))
    }

    /// Открывает PTY + shell на соединении из пула (подключается при необходимости).
    /// Возвращает id терминала и поток событий.
    pub async fn terminal_open(
        &self,
        server: &Server,
        secrets: &dyn SecretStore,
        cols: u32,
        rows: u32,
    ) -> CoreResult<(TerminalInfo, tokio::sync::mpsc::UnboundedReceiver<TermEvent>)> {
        self.connect(server, secrets).await?;
        let (shell, hint, launch) = if server.enhanced_shell {
            let (st, launch) = self.prepare_shell(server, secrets).await?;
            (st.shell, st.install_hint, launch)
        } else {
            ("default".to_string(), None, None)
        };
        let session = self.sessions.lock().await.get(&server.id).cloned().ok_or(CoreError::NotFound)?;
        let ch = match session.channel_open_session().await {
            Ok(c) => c,
            Err(e) => {
                self.sessions.lock().await.remove(&server.id);
                return Err(CoreError::msg(format!("Сессия потеряна: {e}")));
            }
        };
        ch.request_pty(false, "xterm-256color", cols, rows, 0, 0, &[]).await?;
        match launch {
            Some(cmd) => ch.exec(false, cmd.into_bytes()).await?,
            None => ch.request_shell(false).await?,
        }
        let tid = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let h = terminal::spawn(ch, tid.clone(), tx, self.terminals.clone());
        self.terminals.lock().unwrap().insert(tid.clone(), (server.id.clone(), h));
        Ok((TerminalInfo { terminal_id: tid, shell, install_hint: hint }, rx))
    }

    pub fn terminal_write(&self, tid: &str, data: Vec<u8>) -> CoreResult<()> {
        if self.terminal(tid)?.write(data) { Ok(()) } else { Err(CoreError::msg("Терминал закрыт")) }
    }

    pub fn terminal_resize(&self, tid: &str, cols: u32, rows: u32) -> CoreResult<()> {
        self.terminal(tid)?.resize(cols, rows);
        Ok(())
    }

    pub fn terminal_close(&self, tid: &str) -> CoreResult<()> {
        if let Ok(h) = self.terminal(tid) {
            h.close();
        }
        Ok(())
    }

    /// Выполнить команду в активной сессии (подключается автоматически, если сессии нет).
    pub async fn exec(&self, server: &Server, secrets: &dyn SecretStore, command: &str) -> CoreResult<ExecResult> {
        self.run(server, secrets, command, None).await
    }

    async fn run(&self, server: &Server, secrets: &dyn SecretStore, command: &str, stdin: Option<&[u8]>) -> CoreResult<ExecResult> {
        self.connect(server, secrets).await?;
        let session = self.sessions.lock().await.get(&server.id).cloned().ok_or(CoreError::NotFound)?;
        let mut ch = match session.channel_open_session().await {
            Ok(c) => c,
            Err(e) => {
                // соединение умерло — убираем из пула, следующий вызов переподключится
                self.sessions.lock().await.remove(&server.id);
                return Err(CoreError::msg(format!("Сессия потеряна: {e}")));
            }
        };
        ch.exec(true, command.as_bytes().to_vec()).await?;
        if let Some(data) = stdin {
            ch.data_bytes(data.to_vec()).await?;
            ch.eof().await?;
        }
        let (mut out, mut err, mut code) = (Vec::new(), Vec::new(), None);
        while let Some(msg) = ch.wait().await {
            match msg {
                ChannelMsg::Data { data } => out.extend_from_slice(&data),
                ChannelMsg::ExtendedData { data, ext: 1 } => err.extend_from_slice(&data),
                ChannelMsg::ExitStatus { exit_status } => code = Some(exit_status),
                _ => {}
            }
        }
        Ok(ExecResult { stdout: String::from_utf8_lossy(&out).into(), stderr: String::from_utf8_lossy(&err).into(), exit_code: code })
    }

    /// Заливает набор «улучшенной оболочки» (если версия на сервере отличается) и выбирает zsh или bash.
    /// Возвращает статус и команду запуска (None — подходящей оболочки нет).
    async fn prepare_shell(&self, server: &Server, secrets: &dyn SecretStore) -> CoreResult<(ShellStatus, Option<String>)> {
        let probe = enhanced::parse_probe(&self.run(server, secrets, enhanced::PROBE_CMD, None).await?.stdout);
        let mut uploaded = false;
        if probe.remote_version.as_deref() != Some(enhanced::version().as_str()) {
            let r = self
                .run(
                    server,
                    secrets,
                    "d=\"$HOME/.servergui\"; mkdir -p \"$d\" && chmod 700 \"$d\" && rm -rf \"$d/shell\" && mkdir -p \"$d/shell\" && tar xf - -C \"$d/shell\"",
                    Some(&enhanced::build_tar()),
                )
                .await?;
            if r.exit_code != Some(0) {
                return Err(CoreError::msg(format!("Не удалось залить файлы оболочки на сервер (нужен tar): {}", r.stderr.trim())));
            }
            uploaded = true;
        }
        let launch = enhanced::launch_cmd(&probe);
        let status = ShellStatus {
            shell: launch.as_ref().map_or("default", |l| l.0).to_string(),
            uploaded,
            install_hint: (!probe.has_zsh).then(|| enhanced::zsh_install_hint(&probe.os_id, server.username == "root")),
        };
        Ok((status, launch.map(|l| l.1)))
    }

    /// Включение опции: залить файлы и сообщить, какая оболочка будет использована.
    pub async fn shell_install(&self, server: &Server, secrets: &dyn SecretStore) -> CoreResult<ShellStatus> {
        Ok(self.prepare_shell(server, secrets).await?.0)
    }

    /// «Удалить с сервера»: стирает ~/.servergui целиком (файлы оболочки и история).
    pub async fn shell_uninstall(&self, server: &Server, secrets: &dyn SecretStore) -> CoreResult<()> {
        let r = self.run(server, secrets, "rm -rf \"$HOME/.servergui\"", None).await?;
        if r.exit_code != Some(0) {
            return Err(CoreError::msg(format!("Не удалось удалить ~/.servergui: {}", r.stderr.trim())));
        }
        Ok(())
    }
}

fn expand_home(p: &str) -> std::path::PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            return std::path::Path::new(&home).join(rest);
        }
    }
    p.into()
}
