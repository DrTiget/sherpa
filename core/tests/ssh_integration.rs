//! Требует работающий sshd: переменные SSH_TEST_HOST/PORT/KEY (задаёт scripts/integration.sh).
//! Без них тест пропускается.
use servergui_core::{
    known_hosts::KnownHosts,
    model::*,
    secrets::{MemoryStore, SecretStore},
    ssh::SshPool,
    store::ServerStore,
    terminal::TermEvent,
    CoreError,
};
use std::sync::Arc;

fn env() -> Option<(String, u16, String)> {
    Some((std::env::var("SSH_TEST_HOST").ok()?, std::env::var("SSH_TEST_PORT").ok()?.parse().ok()?, std::env::var("SSH_TEST_KEY").ok()?))
}

#[tokio::test]
async fn exec_with_password_and_key() {
    let Some((host, port, key)) = env() else {
        eprintln!("SSH_TEST_* не заданы — пропуск");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let secrets: Arc<MemoryStore> = Arc::new(MemoryStore::default());
    let store = ServerStore::new(dir.path().join("servers.json"), secrets.clone());
    let pool = SshPool::new(KnownHosts::new(dir.path().join("known_hosts")));
    let mk = |auth| ServerInput { name: "t".into(), host: host.clone(), port, username: "tester".into(), group: None, enhanced_shell: false, auth };
    let pw = store.create(mk(AuthInput::Password { password: Some("testpass123".into()) })).unwrap();
    let bad = store.create(mk(AuthInput::Password { password: Some("wrong".into()) })).unwrap();
    let by_key = store.create(mk(AuthInput::Key { key_path: Some(key.clone()), key_pem: None, passphrase: None })).unwrap();
    let by_pem = store
        .create(mk(AuthInput::Key { key_path: None, key_pem: Some(std::fs::read_to_string(&key).unwrap()), passphrase: None }))
        .unwrap();
    let s: &dyn SecretStore = secrets.as_ref();

    // 1. TOFU: первый контакт требует подтверждения
    let fp = match pool.test_connection(&pw, s).await {
        Err(CoreError::UnknownHost { fingerprint, .. }) => fingerprint,
        other => panic!("ожидался UnknownHost, получено {other:?}"),
    };
    assert!(fp.starts_with("SHA256:"));
    assert!(pool.trust_host(&host, port, "SHA256:bogus").is_err());
    pool.trust_host(&host, port, &fp).unwrap();

    // 2. пароль
    pool.test_connection(&pw, s).await.unwrap();
    let r = pool.exec(&pw, s, "echo hello; echo oops >&2; exit 3").await.unwrap();
    assert_eq!((r.stdout.trim(), r.stderr.trim(), r.exit_code), ("hello", "oops", Some(3)));
    assert!(pool.is_connected(&pw.id).await);
    // сессия переиспользуется
    assert_eq!(pool.exec(&pw, s, "whoami").await.unwrap().stdout.trim(), "tester");
    pool.disconnect(&pw.id).await.unwrap();
    assert!(!pool.is_connected(&pw.id).await);

    // 3. неверный пароль
    assert!(matches!(pool.test_connection(&bad, s).await, Err(CoreError::AuthFailed { .. })));

    // 4. ключ (файл и импортированный)
    for srv in [&by_key, &by_pem] {
        assert_eq!(pool.exec(srv, s, "echo key-ok").await.unwrap().stdout.trim(), "key-ok");
    }

    // 5. подмена ключа хоста → отказ
    pool.known_hosts().forget(&host, port).unwrap();
    let fake = russh::keys::PrivateKey::from(russh::keys::ssh_key::private::Ed25519Keypair::from_seed(&[7; 32]));
    pool.known_hosts().trust(&host, port, fake.public_key()).unwrap();
    pool.disconnect(&by_key.id).await.unwrap();
    match pool.test_connection(&by_key, s).await {
        Err(CoreError::HostKeyMismatch { .. }) => {}
        other => panic!("ожидался HostKeyMismatch, получено {other:?}"),
    }
}

async fn read_until(rx: &mut tokio::sync::mpsc::UnboundedReceiver<TermEvent>, needle: &str, acc: &mut String) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !acc.contains(needle) {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        match tokio::time::timeout(left, rx.recv()).await {
            Ok(Some(TermEvent::Data { data })) => acc.push_str(&String::from_utf8_lossy(&data)),
            Ok(Some(TermEvent::Closed { .. })) | Ok(None) => panic!("терминал закрыт, вывод: {acc:?}"),
            Err(_) => panic!("таймаут ожидания {needle:?}, вывод: {acc:?}"),
        }
    }
}

#[tokio::test]
async fn interactive_terminals() {
    let Some((host, port, key)) = env() else { return };
    let dir = tempfile::tempdir().unwrap();
    let secrets: Arc<MemoryStore> = Arc::new(MemoryStore::default());
    let store = ServerStore::new(dir.path().join("servers.json"), secrets.clone());
    let known = KnownHosts::new(dir.path().join("known_hosts"));
    let pool = SshPool::new(known);
    let srv = store
        .create(ServerInput { name: "t".into(), host: host.clone(), port, username: "tester".into(), group: None, enhanced_shell: false,
            auth: AuthInput::Key { key_path: Some(key), key_pem: None, passphrase: None } })
        .unwrap();
    let s: &dyn SecretStore = secrets.as_ref();
    let fp = match pool.connect(&srv, s).await {
        Err(CoreError::UnknownHost { fingerprint, .. }) => fingerprint,
        other => panic!("{other:?}"),
    };
    pool.trust_host(&host, port, &fp).unwrap();

    // два терминала на одной сессии
    let (t1, mut rx1) = pool.terminal_open(&srv, s, 100, 30).await.map(|(i, r)| (i.terminal_id, r)).unwrap();
    let (t2, mut rx2) = pool.terminal_open(&srv, s, 80, 24).await.map(|(i, r)| (i.terminal_id, r)).unwrap();
    assert_ne!(t1, t2);
    let (mut o1, mut o2) = (String::new(), String::new());
    pool.terminal_write(&t1, "echo hello; tty; echo $TERM\n".into()).unwrap();
    read_until(&mut rx1, "xterm-256color", &mut o1).await;
    assert!(o1.contains("hello") && o1.contains("/dev/pts/"), "{o1}");
    // resize
    pool.terminal_resize(&t1, 120, 40).unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    pool.terminal_write(&t1, "stty size\n".into()).unwrap();
    read_until(&mut rx1, "40 120", &mut o1).await;
    // терминалы независимы
    pool.terminal_write(&t2, "stty size\n".into()).unwrap();
    read_until(&mut rx2, "24 80", &mut o2).await;
    // exit shell -> Closed с кодом
    pool.terminal_write(&t2, "exit 5\n".into()).unwrap();
    loop {
        match rx2.recv().await {
            Some(TermEvent::Closed { exit_code, .. }) => { assert_eq!(exit_code, Some(5)); break; }
            Some(_) => {}
            None => panic!("поток закрылся без Closed"),
        }
    }
    assert!(pool.terminal_write(&t2, vec![]).is_err());
    // закрытие пользователем
    pool.terminal_close(&t1).unwrap();
    loop {
        match rx1.recv().await {
            Some(TermEvent::Closed { .. }) => break,
            Some(_) => {}
            None => panic!("нет Closed"),
        }
    }
    // disconnect закрывает терминалы сервера
    let (_t3, mut rx3) = pool.terminal_open(&srv, s, 80, 24).await.unwrap();
    pool.disconnect(&srv.id).await.unwrap();
    loop {
        match tokio::time::timeout(std::time::Duration::from_secs(5), rx3.recv()).await.expect("таймаут") {
            Some(TermEvent::Closed { .. }) => break,
            Some(_) => {}
            None => panic!("нет Closed"),
        }
    }
}
