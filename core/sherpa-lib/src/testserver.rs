//! Тестовый SSH-сервер на russh::server (только для тестов, фича `test-util`).
//! Пускает пользователя `u` по паролю `pw` или по заданному публичному ключу;
//! `exec` отвечает эхом команды (или `Options::exec`), PTY-шелл — эхом ввода:
//! `READY <cols>x<rows>` при старте, `RESIZE …` при смене размера, `exit` —
//! выход с кодом 0, `flood:<N>` — N байт `x` и строка `FLOOD-END`.

use std::net::SocketAddr;
use std::sync::Arc;

use russh::keys::ssh_key::private::Ed25519Keypair;
use russh::keys::{PrivateKey, PublicKey};
use russh::server::{Auth, ChannelOpenHandle, Msg, Session};
use russh::{Channel, ChannelId, MethodKind, MethodSet};
use tokio::sync::oneshot;

pub const USER: &str = "u";
pub const PASSWORD: &str = "pw";

/// Ответ на `exec`: команда → (stdout, код выхода).
pub type ExecResponder = Arc<dyn Fn(&str) -> (String, u32) + Send + Sync>;

#[derive(Clone)]
struct H {
    allowed_key: Option<PublicKey>,
    password_allowed: bool,
    exec: Option<ExecResponder>,
    size: (u32, u32),
}

impl russh::server::Handler for H {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        Ok(if self.password_allowed && user == USER && password == PASSWORD {
            Auth::Accept
        } else {
            Auth::reject()
        })
    }

    async fn auth_publickey(&mut self, user: &str, key: &PublicKey) -> Result<Auth, Self::Error> {
        let ok = user == USER
            && self
                .allowed_key
                .as_ref()
                .is_some_and(|k| k.key_data() == key.key_data());
        Ok(if ok { Auth::Accept } else { Auth::reject() })
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        _channel: ChannelId,
        _term: &str,
        cols: u32,
        rows: u32,
        _pw: u32,
        _ph: u32,
        _modes: &[(russh::Pty, u32)],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.size = (cols, rows);
        Ok(())
    }

    async fn shell_request(&mut self, channel: ChannelId, session: &mut Session) -> Result<(), Self::Error> {
        session.data(
            channel,
            format!("READY {}x{}\r\n", self.size.0, self.size.1).into_bytes(),
        )?;
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        channel: ChannelId,
        cols: u32,
        rows: u32,
        _pw: u32,
        _ph: u32,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.size = (cols, rows);
        session.data(channel, format!("RESIZE {cols}x{rows}\r\n").into_bytes())?;
        Ok(())
    }

    async fn data(&mut self, channel: ChannelId, data: &[u8], session: &mut Session) -> Result<(), Self::Error> {
        let text = String::from_utf8_lossy(data).to_string();
        if let Some(n) = text.trim().strip_prefix("flood:") {
            let n: usize = n.parse().unwrap_or(0);
            let handle = session.handle();
            tokio::spawn(async move {
                let chunk = vec![b'x'; 16 * 1024];
                let mut left = n;
                while left > 0 {
                    let k = left.min(chunk.len());
                    if handle.data(channel, chunk[..k].to_vec()).await.is_err() {
                        return;
                    }
                    left -= k;
                }
                let _ = handle.data(channel, b"\r\nFLOOD-END\r\n".to_vec()).await;
            });
            return Ok(());
        }
        session.data(channel, data.to_vec())?;
        if text.trim() == "exit" {
            session.exit_status_request(channel, 0)?;
            session.eof(channel)?;
            session.close(channel)?;
        }
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let (out, code) = match &self.exec {
            Some(f) => f(&String::from_utf8_lossy(data)),
            None => (String::from_utf8_lossy(data).into_owned(), 0),
        };
        session.data(channel, out.into_bytes())?;
        session.exit_status_request(channel, code)?;
        session.eof(channel)?;
        session.close(channel)?;
        Ok(())
    }
}

pub struct TestServer {
    pub addr: SocketAddr,
    pub host_key: PrivateKey,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl TestServer {
    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// Отпечаток host key сервера (`SHA256:…`).
    pub fn fingerprint(&self) -> String {
        crate::ssh::keys::fingerprint(self.host_key.public_key())
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(s) = self.stop.take() {
            let _ = s.send(());
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

pub struct Options {
    /// 0 — любой свободный порт.
    pub port: u16,
    /// Зерно host key: одно зерно — один и тот же ключ.
    pub seed: u8,
    pub allowed_key: Option<PublicKey>,
    pub password_allowed: bool,
    /// Ответы на `exec` (по умолчанию — эхо команды).
    pub exec: Option<ExecResponder>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            port: 0,
            seed: 1,
            allowed_key: None,
            password_allowed: true,
            exec: None,
        }
    }
}

pub fn start(opts: Options) -> TestServer {
    let host_key = PrivateKey::from(Ed25519Keypair::from_seed(&[opts.seed; 32]));
    let mut methods = MethodSet::empty();
    methods.push(MethodKind::PublicKey);
    if opts.password_allowed {
        methods.push(MethodKind::Password);
    }
    let config = Arc::new(russh::server::Config {
        keys: vec![host_key.clone()],
        methods,
        auth_rejection_time: std::time::Duration::from_millis(10),
        auth_rejection_time_initial: Some(std::time::Duration::from_millis(0)),
        ..Default::default()
    });
    let handler = H {
        allowed_key: opts.allowed_key,
        password_allowed: opts.password_allowed,
        exec: opts.exec,
        size: (80, 24),
    };
    let std_listener = std::net::TcpListener::bind(("127.0.0.1", opts.port)).expect("bind test server");
    std_listener.set_nonblocking(true).expect("nonblocking");
    let addr = std_listener.local_addr().expect("addr");
    let (stop_tx, mut stop_rx) = oneshot::channel::<()>();
    let thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("rt");
        rt.block_on(async move {
            let listener = tokio::net::TcpListener::from_std(std_listener).expect("listener");
            loop {
                tokio::select! {
                    _ = &mut stop_rx => break,
                    acc = listener.accept() => {
                        let Ok((sock, _)) = acc else { continue };
                        let cfg = config.clone();
                        let h = handler.clone();
                        tokio::spawn(async move {
                            if let Ok(s) = russh::server::run_stream(cfg, sock, h).await {
                                let _ = s.await;
                            }
                        });
                    }
                }
            }
        });
        rt.shutdown_background();
    });
    TestServer {
        addr,
        host_key,
        stop: Some(stop_tx),
        thread: Some(thread),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::hostkey::{HostKey, Presented};
    use crate::secret::Secret;
    use crate::ssh::{connect, AuthMaterial, ConnectOptions, KeySource, RemoteExec, Target};
    use std::time::Duration;

    fn target(port: u16, auth: AuthMaterial) -> Target {
        Target {
            host: "127.0.0.1".into(),
            port,
            user: USER.into(),
            auth,
        }
    }

    fn trusted(srv: &TestServer) -> Vec<HostKey> {
        vec![HostKey {
            host: "127.0.0.1".into(),
            port: srv.port(),
            key_type: "ssh-ed25519".into(),
            fingerprint: srv.fingerprint(),
            state: "trusted".into(),
            first_seen_at: String::new(),
            accepted_at: None,
            replaced_fingerprint: None,
        }]
    }

    #[tokio::test]
    async fn connect_auth_exec() {
        let key_text = crate::ssh::keys::generate_ed25519_openssh(Some("pp")).unwrap();
        let pubkey = crate::ssh::keys::decode_key(&key_text, Some("pp"))
            .unwrap()
            .public_key()
            .clone();
        let srv = start(Options {
            allowed_key: Some(pubkey),
            ..Default::default()
        });
        let t = Duration::from_secs(5);

        // Неизвестный ключ сервера — отказ с отпечатком.
        let out = connect(
            &target(srv.port(), AuthMaterial::Password(Some(Secret::new(PASSWORD.into())))),
            &ConnectOptions::new(t, vec![]),
        )
        .await;
        let e = out.result.err().unwrap();
        assert_eq!(e.code, "hostkey.unknown");
        assert_eq!(
            out.presented,
            Some(Presented {
                key_type: "ssh-ed25519".into(),
                fingerprint: srv.fingerprint()
            })
        );

        // Пароль.
        let out = connect(
            &target(srv.port(), AuthMaterial::Password(Some(Secret::new(PASSWORD.into())))),
            &ConnectOptions::new(t, trusted(&srv)),
        )
        .await;
        let s = out.result.unwrap();
        let r = s.exec("echo hi").await.unwrap();
        assert_eq!(r.stdout_lossy(), "echo hi");
        assert_eq!(r.exit_status, Some(0));
        assert!(out.server_banner.unwrap().starts_with("SSH-2.0-"));
        s.close().await;

        // Неверный пароль.
        let out = connect(
            &target(srv.port(), AuthMaterial::Password(Some(Secret::new("bad".into())))),
            &ConnectOptions::new(t, trusted(&srv)),
        )
        .await;
        assert_eq!(out.result.err().unwrap().code, "auth.rejected");

        // Ключ с фразой.
        let auth = AuthMaterial::Key {
            source: KeySource::Text(Secret::new(key_text.to_string())),
            passphrase: Some(Secret::new("pp".into())),
        };
        assert!(
            connect(&target(srv.port(), auth), &ConnectOptions::new(t, trusted(&srv)))
                .await
                .result
                .is_ok()
        );

        // Ключ без фразы.
        let auth = AuthMaterial::Key {
            source: KeySource::Text(Secret::new(key_text.to_string())),
            passphrase: None,
        };
        assert_eq!(
            connect(&target(srv.port(), auth), &ConnectOptions::new(t, trusted(&srv)))
                .await
                .result
                .err()
                .unwrap()
                .code,
            "auth.secret_required"
        );
    }
}
