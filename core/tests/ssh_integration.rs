//! Требует работающий sshd: переменные SSH_TEST_HOST/PORT/KEY (задаёт scripts/integration.sh).
//! Без них тест пропускается.
use servergui_core::{
    known_hosts::KnownHosts,
    model::*,
    secrets::{MemoryStore, SecretStore},
    ssh::SshPool,
    store::ServerStore,
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
    let mk = |auth| ServerInput { name: "t".into(), host: host.clone(), port, username: "tester".into(), group: None, auth };
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
