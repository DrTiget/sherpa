//! SSH-клиент на russh: TCP → handshake → host key (TOFU) → авторизация.

use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use russh::client::{self, AuthResult, Handle, KeyboardInteractiveAuthResponse};
use russh::keys::{PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh::MethodKind;
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;

use crate::error::CoreError;
use crate::repo::hostkey::{self, HostKey, Presented, Verdict};
use crate::ssh::keys;
use crate::ssh::target::{AuthMaterial, KeySource, Target};

#[derive(Debug, Clone)]
pub struct ConnectOptions {
    pub timeout: Duration,
    /// Сохранённые ключи этого (host, port).
    pub known: Vec<HostKey>,
    /// Принимать неизвестный (не изменённый!) ключ без вопроса.
    pub accept_new: bool,
    /// Принять ключ один раз, не сохраняя (ответ `accept_once` в демоне).
    pub accept_once_fingerprint: Option<String>,
    /// Только получить ключ сервера и разорвать соединение (для `hostkey accept`).
    pub probe_only: bool,
    /// Ограничить алгоритмы host key одним типом (для `hostkey accept --key-type`).
    pub key_type: Option<String>,
}

impl ConnectOptions {
    pub fn new(timeout: Duration, known: Vec<HostKey>) -> Self {
        Self {
            timeout,
            known,
            accept_new: false,
            accept_once_fingerprint: None,
            probe_only: false,
            key_type: None,
        }
    }
}

/// Открытое и авторизованное соединение.
pub type Forwarded = (russh::Channel<client::Msg>, tokio::sync::OwnedSemaphorePermit);
#[derive(Clone)]
pub struct ForwardRoute {
    pub tx: tokio::sync::mpsc::Sender<Forwarded>,
    pub slots: Arc<tokio::sync::Semaphore>,
}
pub type ForwardRoutes = Arc<Mutex<std::collections::HashMap<(String, u32), ForwardRoute>>>;
pub struct Session {
    pub handle: Handle<Verifier>,
    pub forwards: ForwardRoutes,
}

impl Session {
    pub async fn close(self) {
        let _ = self
            .handle
            .disconnect(russh::Disconnect::ByApplication, "bye", "en")
            .await;
    }
}

/// Итог попытки подключения. Даже при ошибке содержит предъявленный ключ —
/// вызывающий сохраняет новый ключ (`accepted_new`) и показывает отпечаток.
pub struct ConnectOutcome {
    pub result: Result<Session, CoreError>,
    pub presented: Option<Presented>,
    pub accepted_new: bool,
    pub server_banner: Option<String>,
    pub latency_ms: Option<i64>,
}

#[derive(Default)]
struct VerifyState {
    presented: Option<Presented>,
    verdict: Option<Verdict>,
    accepted_new: bool,
}

/// Обработчик russh: решает судьбу host key по сохранённым ключам.
pub struct Verifier {
    known: Vec<HostKey>,
    accept_new: bool,
    accept_once: Option<String>,
    probe_only: bool,
    state: Arc<Mutex<VerifyState>>,
    forwards: ForwardRoutes,
}

impl client::Handler for Verifier {
    type Error = russh::Error;

    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: russh::Channel<client::Msg>,
        address: &str,
        port: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        let route = self
            .forwards
            .lock()
            .expect("forward routes")
            .get(&(address.to_owned(), port))
            .cloned();
        if let Some(route) = route {
            if let (Ok(slot), Ok(permit)) = (route.slots.try_acquire_owned(), route.tx.try_reserve()) {
                reply.accept().await;
                permit.send((channel, slot));
            }
        }
        Ok(())
    }

    async fn check_server_key(&mut self, key: &PublicKeyOrCertificate) -> Result<bool, Self::Error> {
        let PublicKeyOrCertificate::PublicKey { key, .. } = key else {
            return Ok(false);
        };
        let p = Presented {
            key_type: keys::key_type(&key.algorithm()),
            fingerprint: keys::fingerprint(key),
        };
        let verdict = hostkey::verify(&self.known, &p);
        let mut st = self.state.lock().expect("verify state");
        st.presented = Some(p.clone());
        if self.probe_only {
            st.verdict = Some(verdict);
            return Ok(false);
        }
        let ok = match &verdict {
            Verdict::Trusted => true,
            _ if self.accept_once.as_deref() == Some(p.fingerprint.as_str()) => true,
            Verdict::Unknown if self.accept_new => {
                st.accepted_new = true;
                true
            }
            _ => false,
        };
        st.verdict = Some(verdict);
        Ok(ok)
    }
}

/// Поток, запоминающий первые байты — чтобы показать баннер сервера
/// и распознать «на порту не SSH».
struct BannerTap<S> {
    inner: S,
    buf: Arc<Mutex<Vec<u8>>>,
}

const BANNER_CAP: usize = 1024;

impl<S: AsyncRead + Unpin> AsyncRead for BannerTap<S> {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, out: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        let before = out.filled().len();
        let r = Pin::new(&mut self.inner).poll_read(cx, out);
        if let Poll::Ready(Ok(())) = &r {
            let mut b = self.buf.lock().expect("banner");
            if b.len() < BANNER_CAP {
                let new = &out.filled()[before..];
                let take = new.len().min(BANNER_CAP - b.len());
                b.extend_from_slice(&new[..take]);
            }
        }
        r
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for BannerTap<S> {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, data: &[u8]) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, data)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

fn parse_banner(raw: &[u8]) -> (Option<String>, Option<String>) {
    let text = String::from_utf8_lossy(raw);
    let ssh = text
        .lines()
        .find(|l| l.starts_with("SSH-"))
        .map(|l| l.trim_end().to_string());
    let first = text.lines().next().map(|l| l.trim_end().chars().take(128).collect());
    (ssh, first)
}

fn host_key_algorithms(opts: &ConnectOptions) -> Vec<russh::keys::Algorithm> {
    let default: Vec<_> = russh::Preferred::default().key.to_vec();
    let ty = |a: &russh::keys::Algorithm| keys::key_type(a);
    if let Some(t) = &opts.key_type {
        return default.into_iter().filter(|a| &ty(a) == t).collect();
    }
    // Сначала — типы, которые уже знаем: так сервер предъявит знакомый ключ.
    let known: Vec<&str> = opts
        .known
        .iter()
        .filter(|k| k.state == "trusted")
        .map(|k| k.key_type.as_str())
        .collect();
    let (mut first, rest): (Vec<_>, Vec<_>) = default.into_iter().partition(|a| known.contains(&ty(a).as_str()));
    first.extend(rest);
    first
}

fn io_error(host: &str, port: u16, e: &std::io::Error, timeout: Duration) -> CoreError {
    use std::io::ErrorKind as K;
    match e.kind() {
        K::ConnectionRefused => CoreError::new("net.refused", format!("Connection refused: {host}:{port}"))
            .with("host", host)
            .with("port", port),
        K::TimedOut => timeout_error(host, port, timeout),
        K::HostUnreachable | K::NetworkUnreachable | K::AddrNotAvailable => {
            CoreError::new("net.unreachable", format!("Host unreachable: {host}")).with("host", host)
        }
        K::ConnectionReset | K::ConnectionAborted | K::UnexpectedEof | K::BrokenPipe => {
            CoreError::new("net.reset", format!("Connection reset by {host}")).with("host", host)
        }
        _ => CoreError::new("net.unreachable", format!("Network error: {e}")).with("host", host),
    }
}

fn timeout_error(host: &str, port: u16, timeout: Duration) -> CoreError {
    CoreError::new("net.timeout", format!("Connection to {host}:{port} timed out"))
        .with("host", host)
        .with("port", port)
        .with("timeout_sec", timeout.as_secs())
}

async fn tcp_connect(host: &str, port: u16, timeout: Duration) -> Result<TcpStream, CoreError> {
    let addrs: Vec<_> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| CoreError::new("net.dns_failed", format!("Cannot resolve host {host}")).with("host", host))?
        .collect();
    if addrs.is_empty() {
        return Err(CoreError::new("net.dns_failed", format!("Cannot resolve host {host}")).with("host", host));
    }
    let mut last = None;
    for a in addrs {
        match TcpStream::connect(a).await {
            Ok(s) => {
                let _ = s.set_nodelay(true);
                return Ok(s);
            }
            Err(e) => last = Some(io_error(host, port, &e, timeout)),
        }
    }
    Err(last.expect("at least one address"))
}

fn methods_json(m: &russh::MethodSet) -> Value {
    Value::from(m.iter().map(|k| <&str>::from(k).to_string()).collect::<Vec<_>>())
}

/// Подключиться и авторизоваться (весь путь — в пределах `timeout`).
pub async fn connect(target: &Target, opts: &ConnectOptions) -> ConnectOutcome {
    let state = Arc::new(Mutex::new(VerifyState::default()));
    let banner = Arc::new(Mutex::new(Vec::new()));
    let started = Instant::now();
    let fut = connect_inner(target, opts, state.clone(), banner.clone(), started);
    let result = match tokio::time::timeout(opts.timeout, fut).await {
        Ok(r) => r,
        Err(_) => Err(timeout_error(&target.host, target.port, opts.timeout)),
    };
    let st = state.lock().expect("verify state");
    let (ssh_banner, _) = parse_banner(&banner.lock().expect("banner"));
    let latency = started.elapsed().as_millis() as i64;
    ConnectOutcome {
        result,
        presented: st.presented.clone(),
        accepted_new: st.accepted_new,
        server_banner: ssh_banner,
        latency_ms: Some(latency),
    }
}

async fn connect_inner(
    target: &Target,
    opts: &ConnectOptions,
    state: Arc<Mutex<VerifyState>>,
    banner: Arc<Mutex<Vec<u8>>>,
    _started: Instant,
) -> Result<Session, CoreError> {
    let (host, port) = (target.host.as_str(), target.port);
    let tcp = tcp_connect(host, port, opts.timeout).await?;
    let mut config = client::Config {
        inactivity_timeout: Some(opts.timeout.max(Duration::from_secs(30))),
        ..Default::default()
    };
    config.preferred.key = host_key_algorithms(opts).into();
    let forwards = ForwardRoutes::default();
    let handler = Verifier {
        known: opts.known.clone(),
        accept_new: opts.accept_new,
        accept_once: opts.accept_once_fingerprint.clone(),
        probe_only: opts.probe_only,
        state: state.clone(),
        forwards: forwards.clone(),
    };
    let tap = BannerTap {
        inner: tcp,
        buf: banner.clone(),
    };
    let handle = match client::connect_stream(Arc::new(config), tap, handler).await {
        Ok(h) => h,
        Err(e) => return Err(handshake_error(host, port, e, &state, &banner, opts.timeout)),
    };
    let mut session = Session { handle, forwards };
    authenticate(&mut session, target).await?;
    Ok(session)
}

fn handshake_error(
    host: &str,
    port: u16,
    e: russh::Error,
    state: &Arc<Mutex<VerifyState>>,
    banner: &Arc<Mutex<Vec<u8>>>,
    timeout: Duration,
) -> CoreError {
    let st = state.lock().expect("verify state");
    if let (Some(p), Some(v)) = (&st.presented, &st.verdict) {
        if let Some(err) = hostkey::verdict_error(host, port, p, v) {
            return err;
        }
    }
    let (ssh_banner, first_line) = parse_banner(&banner.lock().expect("banner"));
    match e {
        russh::Error::Version => CoreError::new("ssh.not_ssh", "The server is not an SSH server")
            .with("banner", first_line.unwrap_or_default()),
        russh::Error::NoCommonAlgo { kind, theirs, .. } => {
            CoreError::new("ssh.no_common_algorithm", format!("No common {kind:?} algorithm"))
                .with("kind", format!("{kind:?}").to_lowercase())
                .with(
                    "server",
                    theirs.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(","),
                )
        }
        russh::Error::IO(io) => {
            if ssh_banner.is_none() && first_line.is_some() {
                CoreError::new("ssh.not_ssh", "The server is not an SSH server")
                    .with("banner", first_line.unwrap_or_default())
            } else {
                io_error(host, port, &io, timeout)
            }
        }
        russh::Error::ConnectionTimeout | russh::Error::InactivityTimeout | russh::Error::Elapsed(_) => {
            timeout_error(host, port, timeout)
        }
        russh::Error::Disconnect | russh::Error::HUP => {
            if ssh_banner.is_none() && first_line.is_some() {
                CoreError::new("ssh.not_ssh", "The server is not an SSH server")
                    .with("banner", first_line.unwrap_or_default())
            } else {
                CoreError::new("ssh.disconnected", "The server closed the connection").with("reason", e.to_string())
            }
        }
        other => CoreError::new("ssh.handshake_failed", format!("SSH handshake failed: {other}"))
            .with("reason", other.to_string()),
    }
}

fn auth_io(e: russh::Error) -> CoreError {
    match e {
        russh::Error::Disconnect | russh::Error::HUP => {
            CoreError::new("ssh.disconnected", "The server closed the connection").with("reason", e.to_string())
        }
        other => CoreError::new(
            "ssh.handshake_failed",
            format!("Authentication exchange failed: {other}"),
        )
        .with("reason", other.to_string()),
    }
}

fn rejected(user: &str, remaining: &russh::MethodSet) -> CoreError {
    CoreError::new("auth.rejected", "Authentication rejected")
        .with("user", user)
        .with("methods", methods_json(remaining))
}

async fn authenticate(s: &mut Session, t: &Target) -> Result<(), CoreError> {
    let user = t.user.as_str();
    // Узнать разрешённые методы (и пустить, если сервер пускает без пароля).
    let methods = match s.handle.authenticate_none(user).await.map_err(auth_io)? {
        AuthResult::Success => return Ok(()),
        AuthResult::Failure { remaining_methods, .. } => remaining_methods,
    };
    let allowed = |k: MethodKind| methods.contains(&k);
    match &t.auth {
        AuthMaterial::Password(pw) => {
            let pw = pw.as_ref().ok_or_else(|| {
                CoreError::new("auth.secret_required", "Password is not saved").with("kind", "password")
            })?;
            if allowed(MethodKind::Password) {
                return match s
                    .handle
                    .authenticate_password(user, pw.expose())
                    .await
                    .map_err(auth_io)?
                {
                    AuthResult::Success => Ok(()),
                    AuthResult::Failure { remaining_methods, .. } => Err(rejected(user, &remaining_methods)),
                };
            }
            if allowed(MethodKind::KeyboardInteractive) {
                return keyboard_interactive(s, user, pw.expose()).await;
            }
            Err(not_allowed("password", &methods))
        }
        AuthMaterial::Key { source, passphrase } => {
            if !allowed(MethodKind::PublicKey) {
                return Err(not_allowed("publickey", &methods));
            }
            let text = match source {
                KeySource::File(p) => keys::read_key_file(p)?,
                KeySource::Text(t) => zeroize::Zeroizing::new(t.expose().to_string()),
            };
            let key = keys::decode_key(&text, passphrase.as_ref().map(|p| p.expose()))?;
            let hash = if matches!(key.algorithm(), russh::keys::Algorithm::Rsa { .. }) {
                s.handle.best_supported_rsa_hash().await.map_err(auth_io)?.flatten()
            } else {
                None
            };
            let key = PrivateKeyWithHashAlg::new(Arc::new(key), hash);
            match s.handle.authenticate_publickey(user, key).await.map_err(auth_io)? {
                AuthResult::Success => Ok(()),
                AuthResult::Failure { remaining_methods, .. } => Err(rejected(user, &remaining_methods)),
            }
        }
    }
}

fn not_allowed(method: &str, methods: &russh::MethodSet) -> CoreError {
    CoreError::new(
        "auth.method_not_allowed",
        format!("Server does not allow {method} authentication"),
    )
    .with("method", method)
    .with("methods", methods_json(methods))
}

/// Пароль через keyboard-interactive: отвечаем паролем на каждый скрытый запрос.
async fn keyboard_interactive(s: &mut Session, user: &str, password: &str) -> Result<(), CoreError> {
    let mut resp = s
        .handle
        .authenticate_keyboard_interactive_start(user, None::<String>)
        .await
        .map_err(auth_io)?;
    for _ in 0..5 {
        match resp {
            KeyboardInteractiveAuthResponse::Success => return Ok(()),
            KeyboardInteractiveAuthResponse::Failure { remaining_methods, .. } => {
                return Err(rejected(user, &remaining_methods))
            }
            KeyboardInteractiveAuthResponse::InfoRequest { prompts, .. } => {
                let answers = prompts.iter().map(|_| password.to_string()).collect();
                resp = s
                    .handle
                    .authenticate_keyboard_interactive_respond(answers)
                    .await
                    .map_err(auth_io)?;
            }
        }
    }
    Err(rejected(user, &russh::MethodSet::empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_parsing() {
        let (ssh, first) = parse_banner(b"hello\r\nSSH-2.0-OpenSSH_9.6\r\n");
        assert_eq!(ssh.as_deref(), Some("SSH-2.0-OpenSSH_9.6"));
        assert_eq!(first.as_deref(), Some("hello"));
        let (ssh, first) = parse_banner(b"HTTP/1.1 400 Bad Request\r\n");
        assert!(ssh.is_none());
        assert_eq!(first.as_deref(), Some("HTTP/1.1 400 Bad Request"));
    }
}
